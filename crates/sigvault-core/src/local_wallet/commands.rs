//! Tauri command surface for local-mode wallets.
//!
//! Every `cmd_local_*` here gates on `require_local_mode` so the calls
//! cleanly fail when invoked from cloud mode. Implementations are kept
//! minimal — heavy lifting (wallet sync, PSBT pipeline, HW signing) lands
//! in QBL-218 / QBL-219 / QBL-220 and slots in via the manager + state
//! abstractions this module already exposes.

use std::str::FromStr;
use std::sync::Arc;

use crate::host::{AppHandle, State};
use bdk_wallet::bitcoin::bip32::{Fingerprint, Xpriv};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::{Address, Amount, FeeRate, Network, Psbt, Transaction};
use bdk_wallet::chain::ChainPosition;
use bdk_wallet::miniscript::psbt::PsbtExt;
use bdk_wallet::KeychainKind;
use serde::{Deserialize, Serialize};
use wallet_runtime::{
    add_psbt_signers, add_xprv_signer, analyze_for_signing, list_spending_paths, sign_psbt,
    ElectrumClient, SpendingPath, Wallet,
};

use super::manager::{
    derive_account_at_path, derive_master_xpriv, multipath_descriptor, LocalWalletManager,
    LocalWalletMetadata, ManagerError, SlotInput, SlotMeta, SlotPolicyInput, TimelockedKeyInput,
    TimelockedRecoveryPath, TimelockedSpendingPath, WalletSummary,
};
use super::slots::{
    self, DerivedKey, DescriptorAnalysis, KeyBackupContents, WalletBackup, WalletBackupSlot,
};
use super::storage::HotSecret;
use policy_core::KeyUtils;

fn wallet_runtime_default_account_path(network: Network) -> String {
    KeyUtils::get_primary_derivation_path(network).to_string()
}
use super::settings::{LocalSettings, SettingsStore};
use super::storage::{read_seed_file, WalletDirLayout, WalletId};
use super::sync::{run_sync, ClosureSink, ProgressSink, SyncProgress, SyncSummary};
use crate::state::ApplicationState;

// ---------- request / response payloads ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWalletRequest {
    pub name: String,
    pub network: String,
    /// Only `singlesig_hot` flows through this generic command. Multisig,
    /// timelocked, and watch-only have dedicated commands
    /// (`cmd_local_create_multisig`, `cmd_local_create_timelocked`,
    /// `cmd_local_create_watch_only`) — they don't share enough request
    /// shape with hot singlesig to fit one struct.
    pub policy_type: String,
    /// Passphrase used to encrypt the seed material. Required for hot
    /// wallets, ignored for watch-only / hardware-only.
    pub passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecoverFromMnemonicRequest {
    pub name: String,
    pub network: String,
    pub mnemonic: String,
    /// Wallet-encryption passphrase used to encrypt `seed.enc` on
    /// disk. Separate from `bip39_passphrase` below.
    pub passphrase: String,
    /// Optional BIP39 "25th word" passphrase. Empty string when the
    /// wallet was created without one (the common case). The
    /// cryptographic seed differs between empty and non-empty values
    /// so this MUST match what the original wallet used, otherwise
    /// the recovered descriptor won't match the original wallet.
    #[serde(default)]
    pub bip39_passphrase: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateHardwareWalletRequest {
    pub name: String,
    pub network: String,
    pub fingerprint: String,
    pub xpub: String,
    pub derivation_path: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWatchOnlyWalletRequest {
    pub name: String,
    pub network: String,
    pub external_descriptor: String,
    pub internal_descriptor: String,
    /// Caller-best-effort: any fingerprints the frontend parsed out of
    /// the descriptor's key origins. Purely informational, surfaced in
    /// the wallet list metadata column.
    #[serde(default)]
    pub fingerprints: Vec<String>,
    /// `false` (default) — read-only wallet, no Send button.
    /// `true` (QBL-234) — descriptor-spendable; user intends to sign with
    /// a hardware wallet or external PSBT round-trip. The wallet looks
    /// identical on disk, just tagged differently in metadata so the
    /// dashboard surfaces Send.
    #[serde(default)]
    pub spendable: bool,
}

#[derive(Debug, Deserialize)]
pub struct MultisigCosignerInput {
    /// Descriptor key expression up through the xpub: `[fp/path]xpub` if
    /// origin info is known, bare `xpub` otherwise.
    pub key: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMultisigWalletRequest {
    pub name: String,
    pub network: String,
    pub threshold: u32,
    pub cosigners: Vec<MultisigCosignerInput>,
}

#[derive(Debug, Deserialize)]
pub struct TimelockedKeyInputDto {
    pub fingerprint: String,
    pub xpub: String,
    pub derivation_path: String,
}

#[derive(Debug, Deserialize)]
pub struct TimelockedSpendingPathDto {
    pub keys: Vec<TimelockedKeyInputDto>,
    pub threshold: u32,
}

#[derive(Debug, Deserialize)]
pub struct TimelockedRecoveryPathDto {
    pub timelock_blocks: u16,
    pub path: TimelockedSpendingPathDto,
}

/// The primary spending path for a timelocked-policy wallet. `keys` is the
/// standard case (user-controlled keys + threshold). `unspendable`
/// (QBL-235) tags the wallet recovery-only — backend substitutes a
/// deterministic NUMS-derived xpub. The wire form is internally
/// tagged so a missing variant gives a clear deserialization error
/// rather than silently accepting empty input.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelockedPrimaryDto {
    Keys { path: TimelockedSpendingPathDto },
    Unspendable,
}

#[derive(Debug, Deserialize)]
pub struct CreateTimelockedWalletRequest {
    pub name: String,
    pub network: String,
    pub primary: TimelockedPrimaryDto,
    pub recoveries: Vec<TimelockedRecoveryPathDto>,
}

#[derive(Debug, Deserialize)]
pub struct UnlockWalletRequest {
    pub wallet_id: String,
    pub passphrase: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteWalletRequest {
    pub wallet_id: String,
    /// Required for hot wallets (validates the user can decrypt
    /// `seed.enc` before destroying the directory). Watch-only / HW-only
    /// wallets accept `None` here.
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReceiveAddress {
    pub address: String,
    pub keychain: String,
    pub index: u32,
}

/// Returned by `cmd_local_create_wallet`. For hot creates, `mnemonic_words`
/// carries the freshly generated BIP39 phrase so the wizard can show it
/// once for backup. Empty for watch-only / hardware-only flows that don't
/// produce on-device key material.
#[derive(Debug, Serialize)]
pub struct CreateWalletResponse {
    pub wallet_id: WalletId,
    pub mnemonic_words: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct LocalBalance {
    pub confirmed_sat: u64,
    /// Sum of trusted_pending + untrusted_pending + immature. Includes
    /// incoming change and unmatured coinbase; the dashboard renders this
    /// as a separate "pending" line under the spendable balance.
    pub unconfirmed_sat: u64,
}

/// One row in the wallet history view. `net_sat` is positive for incoming
/// (received > sent) and negative for outgoing. `fee_sat` is 0 for
/// transactions we did not author. `block_time` / `block_height` are
/// `None` for unconfirmed transactions.
#[derive(Debug, Serialize)]
pub struct LocalTxRecord {
    pub txid: String,
    pub net_sat: i64,
    pub fee_sat: u64,
    pub confirmed: bool,
    pub block_height: Option<u32>,
    pub block_time: Option<u64>,
}

// ---------- helpers ----------

fn manager_for(
    app: &AppHandle,
    app_state: &ApplicationState,
) -> Result<LocalWalletManager, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    Ok(LocalWalletManager::new(
        app_data_dir,
        app_state.local_wallet_state.clone(),
    ))
}

fn parse_network(s: &str) -> Result<Network, String> {
    Network::from_str(s).map_err(|_| format!("unsupported network: {s}"))
}

fn map_err(e: ManagerError) -> String {
    e.to_string()
}

// ---------- commands ----------

pub async fn cmd_local_list_wallets(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<Vec<WalletSummary>, String> {
    app_state.require_local_mode().await?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.list_wallets().await.map_err(map_err)
}

pub async fn cmd_local_create_wallet(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateWalletRequest,
) -> Result<CreateWalletResponse, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;

    match request.policy_type.as_str() {
        "singlesig_hot" => {
            let pass = request
                .passphrase
                .ok_or_else(|| "passphrase required for hot wallets".to_string())?;
            let (wallet_id, mnemonic_words) = mgr
                .create_singlesig_hot(&request.name, network, pass.as_bytes())
                .await
                .map_err(map_err)?;
            Ok(CreateWalletResponse {
                wallet_id,
                mnemonic_words,
            })
        }
        "multisig" | "timelocked" | "watch_only" => Err(format!(
            "policy type '{}' uses a dedicated command — call cmd_local_create_{} instead",
            request.policy_type, request.policy_type
        )),
        other => Err(format!("unknown policy_type '{other}'")),
    }
}

pub async fn cmd_local_recover_from_mnemonic(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: RecoverFromMnemonicRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.recover_singlesig_hot(
        &request.name,
        network,
        &request.mnemonic,
        &request.bip39_passphrase,
        request.passphrase.as_bytes(),
    )
    .await
    .map_err(map_err)
}

/// Recover a multisig or timelocked-policy wallet where one of the cosigner slots
/// is a hot key (QBL-230). The user supplies their mnemonic + the
/// wallet's descriptor pair + which policy type it is. The backend
/// derives the master fingerprint from the mnemonic and verifies that
/// fingerprint actually appears in the descriptor — if not, the user
/// has the wrong seed for the wallet and we reject before persisting
/// anything.
#[derive(Debug, Deserialize)]
pub struct RecoverCosignerRequest {
    pub name: String,
    pub network: String,
    pub mnemonic: String,
    /// Optional BIP39 passphrase. Empty string when the seed wasn't
    /// passphrase-protected. The passphrase changes the derived
    /// master, so it MUST match what the original wallet used or the
    /// fingerprint check will fail.
    #[serde(default)]
    pub bip39_passphrase: String,
    /// Wallet-encryption passphrase used to protect `seed.enc` on
    /// disk. Separate from the BIP39 passphrase.
    pub encrypt_passphrase: String,
    pub external_descriptor: String,
    pub internal_descriptor: String,
    /// Either `"multisig"` or `"timelocked"`.
    pub policy_type: String,
}

pub async fn cmd_local_recover_cosigner(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: RecoverCosignerRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.recover_descriptor_hot_cosigner(
        &request.name,
        network,
        &request.mnemonic,
        &request.bip39_passphrase,
        request.encrypt_passphrase.as_bytes(),
        &request.external_descriptor,
        &request.internal_descriptor,
        &request.policy_type,
    )
    .await
    .map_err(map_err)
}

/// Create a singlesig wallet from a hardware device's xpub (QBL-220).
/// The frontend collects fingerprint + xpub + derivation path via the
/// existing cloud-mode HW commands (`cmd_discover_hardware_wallets`,
/// `cmd_unlock_device`, `cmd_get_device_xpub`) and then hands off here.
pub async fn cmd_local_create_singlesig_hw(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateHardwareWalletRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.create_singlesig_hardware(
        &request.name,
        network,
        &request.fingerprint,
        &request.xpub,
        &request.derivation_path,
    )
    .await
    .map_err(map_err)
}

/// Create an M-of-N multisig watch-only wallet from cosigner key
/// expressions (QBL-224). The wallet sees balance + history across all
/// addresses derived from the sortedmulti descriptor. Signing requires
/// at least M cosigners to sign in turn (HW via
/// `cmd_local_sign_psbt_hardware`, or hot/external via PSBT export).
pub async fn cmd_local_create_multisig(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateMultisigWalletRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;
    let cosigners = request
        .cosigners
        .into_iter()
        .map(|c| super::manager::MultisigCosigner {
            key: c.key,
            fingerprint: c.fingerprint,
        })
        .collect();
    mgr.create_multisig(&request.name, network, request.threshold, cosigners)
        .await
        .map_err(map_err)
}

/// Create a timelocked-policy wallet (QBL-225). Primary path is
/// spendable immediately; each recovery path becomes spendable after its
/// block-count timelock elapses. The primary path can be either a
/// user-driven key set or an unspendable NUMS-derived sentinel
/// (QBL-235) for recovery-only wallets.
pub async fn cmd_local_create_timelocked(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateTimelockedWalletRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;

    fn map_path(dto: TimelockedSpendingPathDto) -> TimelockedSpendingPath {
        TimelockedSpendingPath {
            keys: dto
                .keys
                .into_iter()
                .map(|k| TimelockedKeyInput {
                    fingerprint: k.fingerprint,
                    xpub: k.xpub,
                    derivation_path: k.derivation_path,
                })
                .collect(),
            threshold: dto.threshold,
        }
    }

    let primary = match request.primary {
        TimelockedPrimaryDto::Keys { path } => {
            super::manager::TimelockedPrimary::Keys(map_path(path))
        }
        TimelockedPrimaryDto::Unspendable => super::manager::TimelockedPrimary::Unspendable,
    };
    let recoveries: Vec<TimelockedRecoveryPath> = request
        .recoveries
        .into_iter()
        .map(|r| TimelockedRecoveryPath {
            timelock_blocks: r.timelock_blocks,
            path: map_path(r.path),
        })
        .collect();

    mgr.create_timelocked(&request.name, network, primary, recoveries)
        .await
        .map_err(map_err)
}

/// Create a watch-only wallet from descriptor strings the user pasted
/// (QBL-226). Useful for monitoring an external wallet without holding
/// the keys — e.g. an existing Sparrow / Liana / Specter setup imported here so
/// you can see balance and history. Cannot sign.
pub async fn cmd_local_create_watch_only(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateWatchOnlyWalletRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.create_watch_only(
        &request.name,
        network,
        &request.external_descriptor,
        &request.internal_descriptor,
        request.fingerprints,
        request.spendable,
    )
    .await
    .map_err(map_err)
}

pub async fn cmd_local_unlock_wallet(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: UnlockWalletRequest,
) -> Result<(), String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(request.wallet_id);
    let mgr = manager_for(&app, &app_state)?;
    mgr.unlock_wallet(&id, request.passphrase.as_bytes())
        .await
        .map_err(map_err)?;

    // Auto-sync after unlock — fire-and-forget. Progress events flow
    // through the standard `local_wallet_sync_progress` channel; the
    // frontend's useLocalWalletSync hook surfaces them. Failures are
    // logged but not propagated to the unlock call (the wallet is
    // unlocked + readable even if sync fails).
    let app_for_task = app.clone();
    let state_for_task = app_state.local_wallet_state.clone();
    let id_for_task = id.clone();
    tokio::spawn(async move {
        if let Err(e) = run_sync_for(&app_for_task, &state_for_task, id_for_task).await {
            log::warn!("auto-sync after unlock failed: {e}");
        }
    });

    Ok(())
}

pub async fn cmd_local_lock_wallet(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<bool, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);
    let mgr = manager_for(&app, &app_state)?;
    Ok(mgr.lock_wallet(&id).await)
}

pub async fn cmd_local_delete_wallet(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: DeleteWalletRequest,
) -> Result<(), String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(request.wallet_id);
    let mgr = manager_for(&app, &app_state)?;
    mgr.delete_wallet(&id, request.passphrase.as_deref().map(str::as_bytes))
        .await
        .map_err(map_err)
}

pub async fn cmd_local_get_receive_address(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<ReceiveAddress, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);
    let mgr = manager_for(&app, &app_state)?;
    let address = mgr
        .peek_address(&id, KeychainKind::External, 0)
        .await
        .map_err(map_err)?;
    Ok(ReceiveAddress {
        address,
        keychain: "external".to_string(),
        index: 0,
    })
}

/// Read the live balance off the unlocked BDK wallet. Reflects whatever
/// the most recent sync (`cmd_local_sync` / auto-sync after unlock) wrote
/// to the persister; if no sync has run yet the totals are zero.
pub async fn cmd_local_get_balance(
    _app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<LocalBalance, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);
    let handle = app_state
        .local_wallet_state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
    let guard = handle.lock().await;
    let balance = guard.wallet.balance();
    Ok(LocalBalance {
        confirmed_sat: balance.confirmed.to_sat(),
        unconfirmed_sat: (balance.trusted_pending + balance.untrusted_pending + balance.immature)
            .to_sat(),
    })
}

/// Project the BDK wallet's transaction graph into a flat history list
/// the dashboard can render directly. Sorted newest-first by confirmation
/// time (or last-seen for unconfirmed).
pub async fn cmd_local_get_history(
    _app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<Vec<LocalTxRecord>, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);
    let handle = app_state
        .local_wallet_state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
    let guard = handle.lock().await;
    let wallet = &guard.wallet;

    let mut rows: Vec<LocalTxRecord> = wallet
        .transactions()
        .map(|canonical| {
            let tx = canonical.tx_node.tx.as_ref();
            let (sent, received) = wallet.sent_and_received(tx);
            let net_sat = received.to_sat() as i64 - sent.to_sat() as i64;
            let fee_sat = wallet.calculate_fee(tx).map(|f| f.to_sat()).unwrap_or(0);
            let (confirmed, block_height, block_time) = match canonical.chain_position {
                ChainPosition::Confirmed { anchor, .. } => (
                    true,
                    Some(anchor.block_id.height),
                    Some(anchor.confirmation_time),
                ),
                ChainPosition::Unconfirmed { last_seen } => (false, None, last_seen),
            };
            LocalTxRecord {
                txid: canonical.tx_node.txid.to_string(),
                net_sat,
                fee_sat,
                confirmed,
                block_height,
                block_time,
            }
        })
        .collect();

    // Newest first. Confirmed ranks above unconfirmed at the same time;
    // among confirmed, higher height wins; among unconfirmed, higher
    // last_seen wins. Stable secondary sort on txid keeps the order
    // deterministic across calls.
    rows.sort_by(|a, b| {
        b.confirmed
            .cmp(&a.confirmed)
            .then_with(|| b.block_height.cmp(&a.block_height))
            .then_with(|| b.block_time.cmp(&a.block_time))
            .then_with(|| a.txid.cmp(&b.txid))
    });

    Ok(rows)
}

pub async fn cmd_local_get_settings(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<LocalSettings, String> {
    app_state.require_local_mode().await?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    SettingsStore::new(app_data_dir)
        .load()
        .map_err(|e| e.to_string())
}

pub async fn cmd_local_set_settings(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    settings: LocalSettings,
) -> Result<(), String> {
    app_state.require_local_mode().await?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    SettingsStore::new(app_data_dir)
        .save(&settings)
        .map_err(|e| e.to_string())
}

/// Tauri event channel name for sync progress. Frontend's
/// `useLocalWalletSync` hook listens on this; payload is `SyncProgress`.
const SYNC_PROGRESS_EVENT: &str = "local_wallet_sync_progress";

/// Run an Electrum full_scan against the unlocked wallet, apply the
/// update, persist, and return a summary. Emits progress events on the
/// `local_wallet_sync_progress` channel for the UI to render.
pub async fn cmd_local_sync(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<SyncSummary, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);
    run_sync_for(&app, &app_state.local_wallet_state, id)
        .await
        .map_err(|e| e.to_string())
}

/// Shared sync entry point used by both `cmd_local_sync` (foreground)
/// and the post-unlock auto-sync task (background). Resolves the
/// network from metadata, the electrs URL from settings, the unlocked
/// handle from state, then delegates to `sync::run_sync` with a closure
/// sink that emits to the `local_wallet_sync_progress` channel.
async fn run_sync_for(
    app: &AppHandle,
    state: &Arc<crate::local_wallet::state::LocalWalletState>,
    id: WalletId,
) -> Result<SyncSummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    let mgr = LocalWalletManager::new(app_data_dir.clone(), state.clone());
    let meta = mgr.read_metadata(&id).map_err(map_err)?;
    let network = Network::from_str(&meta.network)
        .map_err(|_| format!("unsupported network in metadata: {}", meta.network))?;

    let settings = SettingsStore::new(app_data_dir)
        .load()
        .map_err(|e| e.to_string())?;
    let electrs_url = settings
        .electrs_url_for(network)
        .map_err(|e| e.to_string())?;

    let handle = state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;

    let app_for_emit = app.clone();
    let sink: Arc<dyn ProgressSink> = Arc::new(ClosureSink(move |p: SyncProgress| {
        let _ = app_for_emit.emit(SYNC_PROGRESS_EVENT, &p);
    }));

    run_sync(handle, network, electrs_url, id, sink)
        .await
        .map_err(|e| e.to_string())
}

// ---------- Wallet detail (QBL-234) ----------

/// Full wallet metadata view, including descriptor strings. Used by
/// the Send flow to build a `WalletConfig` for `cmd_discover_hardware_wallets`
/// / `cmd_unlock_device` so on-device policy registration (Ledger HMAC,
/// BitBox script-config, Jade descriptor) happens transparently before
/// signing. The wallet list (`cmd_local_list_wallets`) intentionally
/// omits descriptors to keep its payload small; the dashboard fetches
/// them lazily via this command when needed.
#[derive(Debug, Serialize)]
pub struct WalletDetails {
    pub id: WalletId,
    pub name: String,
    pub network: String,
    pub policy_type: String,
    pub external_descriptor: String,
    pub internal_descriptor: String,
    /// Receive and change descriptors as one `<0;1>` descriptor, the form
    /// hardware devices register as a wallet policy. `None` when the two
    /// do not merge.
    pub multipath_descriptor: Option<String>,
    pub fingerprints: Vec<String>,
    pub has_hot_keys: bool,
    pub slots: Vec<SlotMeta>,
}

pub async fn cmd_local_get_wallet_details(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<WalletDetails, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);
    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr.read_metadata(&id).map_err(map_err)?;
    Ok(WalletDetails {
        id: meta.id,
        name: meta.name,
        network: meta.network,
        policy_type: meta.policy_type,
        multipath_descriptor: multipath_descriptor(
            &meta.external_descriptor,
            &meta.internal_descriptor,
        )
        .ok(),
        external_descriptor: meta.external_descriptor,
        internal_descriptor: meta.internal_descriptor,
        fingerprints: meta.fingerprints,
        has_hot_keys: meta.has_hot_keys,
        slots: meta.slots,
    })
}

// ---------- Spending-path enumeration (QBL-234) ----------
//
// Surfaces the available spending paths for a wallet so the SendWizard
// can present a path picker for multi-path descriptors (timelocked
// primary vs recovery, taproot multi-leaf). Single-path wallets get
// one entry the wizard auto-selects.

/// Wire form of `wallet_runtime::SpendingPath`. The `policy_path` map
/// is encoded as a `BTreeMap<String, Vec<u32>>` (instead of `usize`) so
/// it round-trips cleanly through Tauri's JSON serializer; the desktop
/// side and the runtime both treat the indices as bounded small
/// integers.
#[derive(Debug, Serialize)]
pub struct SpendingPathDto {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub threshold: u32,
    pub fingerprints: Vec<String>,
    pub timelock_blocks: Option<u32>,
    pub policy_path: Option<std::collections::BTreeMap<String, Vec<u32>>>,
}

impl From<SpendingPath> for SpendingPathDto {
    fn from(p: SpendingPath) -> Self {
        SpendingPathDto {
            id: p.id,
            label: p.label,
            description: p.description,
            threshold: p.threshold,
            fingerprints: p.fingerprints,
            timelock_blocks: p.timelock_blocks,
            policy_path: p.policy_path.map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, v.into_iter().map(|i| i as u32).collect()))
                    .collect()
            }),
        }
    }
}

/// Enumerate the spending paths available for a wallet. Returns an
/// empty list for watch-only wallets (which cannot spend) and a
/// single-entry list for single-sig / multisig (which have one path).
/// Timelocked-policy wallets surface one path per branch (primary + each recovery
/// in ascending-timelock order). The wallet must be unlocked — the
/// command needs BDK's external-policy id to populate `policy_path`
/// for multi-branch descriptors.
pub async fn cmd_local_list_spending_paths(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    wallet_id: String,
) -> Result<Vec<SpendingPathDto>, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(wallet_id);

    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr.read_metadata(&id).map_err(map_err)?;

    // BDK's external policy id is needed to construct `policy_path`
    // entries for timelocked branches (TxBuilder::policy_path keys on it).
    // Singlesig / multisig don't need it, but reading it is cheap and
    // keeps the code path uniform.
    let handle = app_state
        .local_wallet_state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
    let guard = handle.lock().await;
    let policy_id = guard
        .wallet
        .policies(KeychainKind::External)
        .map_err(|e| format!("read external policy: {e}"))?
        .map(|p| p.id);
    drop(guard);

    let paths = spending_paths_for(&meta, policy_id.as_deref())?;

    Ok(paths.into_iter().map(SpendingPathDto::from).collect())
}

/// The wallet's spending paths. `list_spending_paths` reads a policy
/// descriptor only in its multipath form, which the metadata does not
/// store, so it is rebuilt from the receive and change descriptors.
pub fn spending_paths_for(
    meta: &LocalWalletMetadata,
    external_policy_id: Option<&str>,
) -> Result<Vec<SpendingPath>, String> {
    let multipath = multipath_descriptor(&meta.external_descriptor, &meta.internal_descriptor);
    let descriptor = match meta.policy_type.as_str() {
        "timelocked" => multipath.map_err(map_err)?,
        // An imported descriptor need not be a policy; the receive
        // descriptor still yields its single default path.
        "descriptor" => multipath.unwrap_or_else(|_| meta.external_descriptor.clone()),
        _ => meta.external_descriptor.clone(),
    };
    list_spending_paths(
        &meta.policy_type,
        &descriptor,
        &meta.fingerprints,
        external_policy_id,
    )
    .map_err(|e| format!("enumerate spending paths: {e}"))
}

// ---------- PSBT pipeline (QBL-219) ----------
//
// Three commands that together cover the local-mode send flow:
//   1. cmd_local_build_psbt — assemble a PSBT from recipients + fee rate
//      using BDK's TxBuilder. Coin selection and change derivation are
//      delegated to BDK; the wallet is persisted afterward so the
//      revealed change index sticks.
//   2. cmd_local_sign_psbt_software — re-decrypt the on-disk seed with
//      a fresh passphrase prompt (defense in depth — even if the wallet
//      is unlocked, signing requires explicit re-auth), derive the
//      account xprv, register per-derivation signers via wallet-runtime,
//      and sign every input we have keys for.
//   3. cmd_local_broadcast_psbt — finalize via miniscript, extract the
//      transaction, push it through wallet-runtime's ElectrumClient
//      against the network's configured electrs endpoint.

#[derive(Debug, Deserialize)]
pub struct PsbtRecipient {
    pub address: String,
    pub amount_sat: u64,
}

#[derive(Debug, Deserialize)]
pub struct BuildPsbtRequest {
    pub wallet_id: String,
    pub recipients: Vec<PsbtRecipient>,
    /// Fee rate in sat/vbyte. Floats accepted so callers can pass the
    /// fractional values typical fee estimators emit; we round up to
    /// the next whole sat/vbyte for the BDK builder (overpaying slightly
    /// is the safe direction).
    pub fee_rate_sat_vb: f32,
    /// Optional `TxBuilder::policy_path` map — required for multi-branch
    /// descriptors (timelocked primary vs recovery, taproot multi-leaf) so
    /// BDK knows which spending condition to satisfy. Single-path
    /// descriptors (singlesig, multisig) ignore this — leaving it
    /// empty selects the only available path. Sourced from
    /// `cmd_local_list_spending_paths`'s `SpendingPath::policy_path`.
    #[serde(default)]
    pub policy_path: Option<std::collections::BTreeMap<String, Vec<u32>>>,
}

#[derive(Debug, Serialize)]
pub struct BuildPsbtResponse {
    pub psbt_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct SignPsbtSoftwareRequest {
    pub wallet_id: String,
    pub psbt_base64: String,
    pub passphrase: String,
}

#[derive(Debug, Serialize)]
pub struct SignPsbtResponse {
    pub psbt_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct BroadcastPsbtRequest {
    pub wallet_id: String,
    pub psbt_base64: String,
}

#[derive(Debug, Serialize)]
pub struct BroadcastPsbtResponse {
    pub txid: String,
}

pub async fn cmd_local_build_psbt(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: BuildPsbtRequest,
) -> Result<BuildPsbtResponse, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(request.wallet_id);

    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr.read_metadata(&id).map_err(map_err)?;
    let network = parse_network(&meta.network)?;

    if request.recipients.is_empty() {
        return Err("at least one recipient is required".to_string());
    }
    let fee_rate_sat_vb = request.fee_rate_sat_vb.ceil().max(1.0) as u64;
    let fee_rate = FeeRate::from_sat_per_vb(fee_rate_sat_vb)
        .ok_or_else(|| format!("invalid fee rate: {}", request.fee_rate_sat_vb))?;

    let handle = app_state
        .local_wallet_state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
    let mut guard = handle.lock().await;

    // Borrow-split so persist after build_tx can take &mut on both
    // wallet and persister fields without the borrow checker thinking
    // they overlap (same pattern as sync.rs).
    let super::state::UnlockedHandle {
        wallet, persister, ..
    } = &mut *guard;

    let psbt_b64 = {
        let mut builder = wallet.build_tx();
        builder.fee_rate(fee_rate);

        // Inject the chosen spending path for multi-branch descriptors
        // (timelocked primary vs recovery, taproot multi-leaf). The same
        // map is applied to both keychains so change addresses go through
        // the matching path — for timelocked wallets this means change
        // inherits the primary's selection, which is what the user
        // expects (you wouldn't want recovery-only change).
        if let Some(pp) = &request.policy_path {
            let pp_usize: std::collections::BTreeMap<String, Vec<usize>> = pp
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().map(|i| *i as usize).collect()))
                .collect();
            builder.policy_path(pp_usize.clone(), KeychainKind::External);
            builder.policy_path(pp_usize, KeychainKind::Internal);
        }

        for r in &request.recipients {
            let address = Address::from_str(&r.address)
                .map_err(|e| format!("invalid address '{}': {}", r.address, e))?
                .require_network(network)
                .map_err(|e| {
                    format!(
                        "address '{}' not on {} network: {}",
                        r.address, meta.network, e
                    )
                })?;
            builder.add_recipient(address.script_pubkey(), Amount::from_sat(r.amount_sat));
        }
        let psbt = builder.finish().map_err(|e| format!("build_tx: {}", e))?;
        psbt.to_string()
    };

    // build_tx may have advanced the change keychain index; persist so
    // the revealed index survives a lock/unlock cycle.
    wallet
        .persist(persister)
        .map_err(|e| format!("persist after build_tx: {}", e))?;

    Ok(BuildPsbtResponse {
        psbt_base64: psbt_b64,
    })
}

/// Register `account_xpriv`'s signers for `psbt` on the wallet: one per
/// key the PSBT names for `fingerprint`, each as key path / script path /
/// SegWit v0 by that key's own origin. A PSBT that names no key of this
/// device falls back to one signer kind for the wallet's own derivations.
pub fn register_software_signers(
    wallet: &mut Wallet,
    account_xpriv: &Xpriv,
    psbt: &Psbt,
    fingerprint: &Fingerprint,
) -> Result<(), String> {
    let named = add_psbt_signers(wallet, account_xpriv, psbt, fingerprint)
        .map_err(|e| format!("add_psbt_signers: {}", e))?;
    if named > 0 {
        return Ok(());
    }
    let analysis = analyze_for_signing(wallet, psbt, fingerprint);
    for (keychain, index) in &analysis.required_derivations {
        add_xprv_signer(
            wallet,
            account_xpriv,
            *keychain,
            *index,
            analysis.signer_kind,
        )
        .map_err(|e| format!("add_xprv_signer: {}", e))?;
    }
    Ok(())
}

pub async fn cmd_local_sign_psbt_software(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: SignPsbtSoftwareRequest,
) -> Result<SignPsbtResponse, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(request.wallet_id);

    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr.read_metadata(&id).map_err(map_err)?;
    let network = parse_network(&meta.network)?;

    // Defense-in-depth re-decrypt: even though the wallet may be
    // unlocked, signing requires the user to re-prove the passphrase.
    // Wrong passphrase here surfaces as SeedStoreError::AuthFailed.
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    let layout = WalletDirLayout::for_wallet(app_data_dir.join("local"), &id);
    let payload = read_seed_file(&layout, request.passphrase.as_bytes())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "wallet has no on-device seed (watch-only or HW-only)".to_string())?;

    // Keyring wallets (slot-based create / recover) may hold several
    // keys: register signers for each and sign once.
    if !payload.keys.is_empty() {
        let mut psbt =
            Psbt::from_str(&request.psbt_base64).map_err(|e| format!("parse psbt: {}", e))?;
        let mut keys = Vec::with_capacity(payload.keys.len());
        for entry in &payload.keys {
            let resolved = slots::resolve_secret(
                network,
                &entry.secret,
                Some(&entry.derivation_path),
                Some(&entry.fingerprint),
                Some(&entry.derivation_path),
            )
            .map_err(|e| e.to_string())?;
            keys.push((resolved.account_xpriv, resolved.fingerprint));
        }
        let handle = app_state
            .local_wallet_state
            .get(&id)
            .await
            .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
        let mut guard = handle.lock().await;
        for (account_xpriv, fingerprint) in &keys {
            let analysis = analyze_for_signing(&guard.wallet, &psbt, fingerprint);
            for (keychain, index) in &analysis.required_derivations {
                add_xprv_signer(
                    &mut guard.wallet,
                    account_xpriv,
                    *keychain,
                    *index,
                    analysis.signer_kind,
                )
                .map_err(|e| format!("add_xprv_signer: {}", e))?;
            }
        }
        // See the legacy branch below for why the bool is ignored.
        let _ = sign_psbt(&guard.wallet, &mut psbt).map_err(|e| format!("sign_psbt: {}", e))?;
        return Ok(SignPsbtResponse {
            psbt_base64: psbt.to_string(),
        });
    }

    let mnemonic_str = payload.mnemonic.as_str();
    let bip39_passphrase = payload.bip39_passphrase.as_str();

    // Wallets created via the hot-creation flow leave
    // `derivation_path = None` — those still derive at the network's
    // default singlesig path. QBL-230 cosigner-recovered wallets
    // persist the path their descriptor's origin block specified
    // (e.g. `48'/1'/0'/2'` for a BIP48 multisig slot); we derive at
    // exactly that path so the produced xprv matches the slot. The
    // fingerprint the signers are matched on differs by case too: the
    // singlesig descriptor has no origin block, so its PSBT key origins
    // carry the account-xpub fingerprint; the cosigner case uses the
    // master fingerprint, which is what `bip32_derivation` /
    // `tap_key_origins` name. The BIP39 passphrase (if any) flows
    // through derive_master_xpriv / derive_account_at_path so the
    // 25th-word case Just Works™.
    let (account_xpriv, fingerprint) = if let Some(path) = meta.derivation_path.as_deref() {
        let account = derive_account_at_path(network, mnemonic_str, bip39_passphrase, path)
            .map_err(|e| e.to_string())?;
        let master = derive_master_xpriv(network, mnemonic_str, bip39_passphrase)
            .map_err(|e| e.to_string())?;
        let secp = Secp256k1::new();
        let fp = master.fingerprint(&secp);
        (account, fp)
    } else {
        let path = wallet_runtime_default_account_path(network);
        let account = derive_account_at_path(network, mnemonic_str, bip39_passphrase, &path)
            .map_err(|e| e.to_string())?;
        let secp = Secp256k1::new();
        let fp = account.fingerprint(&secp);
        (account, fp)
    };

    let mut psbt =
        Psbt::from_str(&request.psbt_base64).map_err(|e| format!("parse psbt: {}", e))?;

    let handle = app_state
        .local_wallet_state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
    let mut guard = handle.lock().await;

    register_software_signers(&mut guard.wallet, &account_xpriv, &psbt, &fingerprint)?;

    // wallet_runtime::sign_psbt hardcodes try_finalize: false, and BDK
    // 1.x's wallet.sign() returns Ok(false) unconditionally in that
    // case (see bdk_wallet 1.1.0 wallet/mod.rs:1785). The bool is
    // therefore meaningless for our purposes — we drop it here. Whether
    // a PSBT is "fully signed" is determined at broadcast time, when
    // miniscript's finalize_mut is the authoritative check.
    let _ = sign_psbt(&guard.wallet, &mut psbt).map_err(|e| format!("sign_psbt: {}", e))?;

    Ok(SignPsbtResponse {
        psbt_base64: psbt.to_string(),
    })
}

#[derive(Debug, Deserialize)]
pub struct SignPsbtHardwareRequest {
    pub wallet_id: String,
    pub psbt_base64: String,
    /// `id` from the DiscoveredDevice the user selected — same shape as
    /// the cloud-mode HW flow uses. The frontend collects this through
    /// `cmd_discover_hardware_wallets` + `cmd_unlock_device`.
    pub device_id: String,
}

/// Sign a PSBT against a connected hardware device (QBL-220, QBL-234).
/// Delegates to the existing HW manager, which speaks to Ledger /
/// Trezor / BitBox / Jade / Coldcard via async-hwi.
///
/// Pre-flight: read the wallet's required fingerprints from metadata
/// and confirm the chosen device's master fingerprint is one of them.
/// This catches a class of mistakes the device itself can't (a singlesig
/// HW wallet fed someone else's PSBT, or a multisig cosigner trying to
/// sign a different multisig's PSBT) before we hit the device with a
/// transaction it has no business with — the device would either
/// silently produce an empty signature set or, on stricter signers,
/// throw an opaque error. Surfacing the fingerprint mismatch up front
/// is a much friendlier failure mode.
///
/// The wallet's BDK descriptors carry the device's key origin metadata,
/// so for inputs the device DOES own, async-hwi finds them by matching
/// fingerprint + derivation in the PSBT input map.
pub async fn cmd_local_sign_psbt_hardware(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: SignPsbtHardwareRequest,
) -> Result<SignPsbtResponse, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(request.wallet_id);

    // Resolve the wallet's required fingerprint set so we can
    // pre-validate before delegating to the HW manager.
    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr.read_metadata(&id).map_err(map_err)?;
    let required: std::collections::HashSet<String> =
        meta.fingerprints.iter().map(|f| f.to_lowercase()).collect();

    let hw_manager = app_state
        .require_hw_manager()
        .await
        .map_err(|e| format!("hardware manager unavailable: {e}"))?;

    // Only enforce fingerprint matching when the wallet metadata
    // actually carries fingerprints. Watch-only descriptor wallets that
    // were imported without origin info fall back to "trust the user"
    // — the device will refuse anything it doesn't have keys for.
    if !required.is_empty() {
        let device = hw_manager
            .get_device(&request.device_id)
            .await
            .ok_or_else(|| {
                format!(
                    "device '{}' not connected. Run device discovery first.",
                    request.device_id
                )
            })?;
        let device_fp = device
            .get_master_fingerprint()
            .await
            .map_err(|e| format!("read device fingerprint: {e}"))?
            .to_string()
            .to_lowercase();
        if !required.contains(&device_fp) {
            let want = meta.fingerprints.join(", ");
            return Err(format!(
                "this device's fingerprint ({}) is not part of the wallet. Wallet expects one of: {}",
                device_fp, want
            ));
        }
    }

    let signed = hw_manager
        .sign_psbt(&request.device_id, &request.psbt_base64)
        .await
        .map_err(|e| format!("hw sign: {e}"))?;

    Ok(SignPsbtResponse {
        psbt_base64: signed.psbt,
    })
}

#[derive(Debug, Deserialize)]
pub struct FinalizePsbtRequest {
    pub psbt_base64: String,
}

/// What a signed PSBT would put on the network, without broadcasting it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FinalizePsbtResponse {
    /// Every input carries enough signatures: the transaction can be
    /// broadcast as is.
    pub complete: bool,
    /// The final transaction's id (only when `complete`).
    pub txid: Option<String>,
    /// Virtual size of the final transaction (only when `complete`).
    pub vsize: Option<u64>,
    /// Absolute fee, when every input carries its UTXO.
    pub fee_sat: Option<u64>,
    /// Why the PSBT can't be finalized yet (missing signatures, …).
    pub detail: Option<String>,
}

/// Finalize a copy of `psbt` via miniscript and extract the transaction.
/// Nothing is broadcast and `psbt` itself is left untouched, so the caller
/// can still hand the signed (not finalized) PSBT to
/// [`cmd_local_broadcast_psbt`] or to another signer.
///
/// `wallet.sign(.., try_finalize: true)` does not handle multisig PSBTs
/// correctly; `PsbtExt::finalize_mut` is the canonical path used by
/// walletrs's server too.
pub fn finalize_psbt(psbt: &Psbt) -> Result<Transaction, String> {
    let mut psbt = psbt.clone();
    let secp = Secp256k1::verification_only();
    if let Err(errors) = psbt.finalize_mut(&secp) {
        return Err(format!("finalize: {:?}", errors));
    }
    psbt.extract_tx().map_err(|e| format!("extract_tx: {}", e))
}

/// [`FinalizePsbtResponse`] for a base64 PSBT. Errors only when the PSBT
/// can't be parsed; missing signatures are reported as `complete: false`.
pub fn inspect_signed_psbt(psbt_base64: &str) -> Result<FinalizePsbtResponse, String> {
    let psbt = Psbt::from_str(psbt_base64.trim()).map_err(|e| format!("parse psbt: {}", e))?;
    let fee_sat = psbt.fee().ok().map(|f| f.to_sat());
    Ok(match finalize_psbt(&psbt) {
        Ok(tx) => FinalizePsbtResponse {
            complete: true,
            txid: Some(tx.compute_txid().to_string()),
            vsize: Some(tx.vsize() as u64),
            fee_sat,
            detail: None,
        },
        Err(detail) => FinalizePsbtResponse {
            complete: false,
            txid: None,
            vsize: None,
            fee_sat,
            detail: Some(detail),
        },
    })
}

/// Upper bound on the virtual size the transaction in an unsigned PSBT
/// will have once `descriptor` (the spending wallet's) satisfies every
/// input: the same estimate the wallet used when it picked the fee. Lets
/// the send review show the fee rate the built PSBT actually pays rather
/// than the rate that was typed in. `None` if either can't be parsed.
pub fn estimate_signed_vsize(psbt_base64: &str, descriptor: &str) -> Option<u64> {
    use bdk_wallet::bitcoin::Weight;
    use bdk_wallet::miniscript::{Descriptor, DescriptorPublicKey};
    let psbt = Psbt::from_str(psbt_base64.trim()).ok()?;
    let desc = Descriptor::<DescriptorPublicKey>::from_str(descriptor.trim()).ok()?;
    let single = desc.into_single_descriptors().ok()?.into_iter().next()?;
    let per_input = single.max_weight_to_satisfy().ok()?;
    let tx = &psbt.unsigned_tx;
    // Unsigned weight + segwit marker/flag + each input's witness.
    let weight = tx.weight() + Weight::from_wu(2) + per_input * tx.input.len() as u64;
    Some(weight.to_vbytes_ceil())
}

/// Check that a signed PSBT is ready to broadcast and describe the final
/// transaction (txid, size, fee) without broadcasting it. The send flow
/// shows this between signing and [`cmd_local_broadcast_psbt`].
pub async fn cmd_local_finalize_psbt(
    app_state: State<'_, ApplicationState>,
    request: FinalizePsbtRequest,
) -> Result<FinalizePsbtResponse, String> {
    app_state.require_local_mode().await?;
    inspect_signed_psbt(&request.psbt_base64)
}

pub async fn cmd_local_broadcast_psbt(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: BroadcastPsbtRequest,
) -> Result<BroadcastPsbtResponse, String> {
    app_state.require_local_mode().await?;
    let id = WalletId::from(request.wallet_id);

    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr.read_metadata(&id).map_err(map_err)?;
    let network = parse_network(&meta.network)?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    let settings = SettingsStore::new(app_data_dir)
        .load()
        .map_err(|e| e.to_string())?;
    let electrs_url = settings
        .electrs_url_for(network)
        .map_err(|e| e.to_string())?;

    let psbt = Psbt::from_str(&request.psbt_base64).map_err(|e| format!("parse psbt: {}", e))?;
    let tx = finalize_psbt(&psbt)?;

    // Electrum I/O blocks; offload to a blocking thread so the runtime
    // stays responsive.
    let txid = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let client = ElectrumClient::connect(&electrs_url)
            .map_err(|e| format!("electrum connect {}: {}", electrs_url, e))?;
        client
            .broadcast(&tx)
            .map_err(|e| format!("broadcast: {}", e))
    })
    .await
    .map_err(|e| format!("broadcast task: {}", e))??;

    Ok(BroadcastPsbtResponse { txid })
}

// ---------- Slot-based create / recover ----------
//
// The create and recover wizards model a wallet as a policy with key
// slots. These commands generate and derive slot keys, analyse a pasted
// descriptor into its slots, persist the wallet, and read / write the
// two backup files (encrypted single key, secret-free wallet backup).

#[derive(Debug, Deserialize)]
pub struct GenerateMnemonicRequest {
    pub word_count: u32,
}

/// Generate a fresh BIP39 mnemonic for a slot. Nothing is persisted —
/// the wizard shows the words for backup and sends them back as the
/// slot's secret on create.
pub async fn cmd_local_generate_mnemonic(
    app_state: State<'_, ApplicationState>,
    request: GenerateMnemonicRequest,
) -> Result<Vec<String>, String> {
    app_state.require_local_mode().await?;
    slots::generate_mnemonic(request.word_count).map_err(map_err)
}

#[derive(Debug, Deserialize)]
pub struct DeriveKeyRequest {
    pub network: String,
    pub secret: HotSecret,
    /// Account path to derive at. Ignored for account-level xprvs.
    #[serde(default)]
    pub derivation_path: Option<String>,
    /// Origin of an account-level xprv pasted without `[fp/path]`.
    #[serde(default)]
    pub origin_fingerprint: Option<String>,
    #[serde(default)]
    pub origin_path: Option<String>,
}

/// Derive the public slot key (fingerprint, path, xpub) a secret yields.
pub async fn cmd_local_derive_key(
    app_state: State<'_, ApplicationState>,
    request: DeriveKeyRequest,
) -> Result<DerivedKey, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let resolved = slots::resolve_secret(
        network,
        &request.secret,
        request.derivation_path.as_deref(),
        request.origin_fingerprint.as_deref(),
        request.origin_path.as_deref(),
    )
    .map_err(map_err)?;
    Ok(DerivedKey::from(&resolved))
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeDescriptorRequest {
    pub network: String,
    pub text: String,
}

/// Parse a descriptor (multipath, receive/change pair, receive-only) or
/// a sigvault wallet backup into its policy, spending paths and slots.
pub async fn cmd_local_analyze_descriptor(
    app_state: State<'_, ApplicationState>,
    request: AnalyzeDescriptorRequest,
) -> Result<DescriptorAnalysis, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    slots::analyze_descriptor(network, &request.text).map_err(map_err)
}

#[derive(Debug, Deserialize)]
pub struct CreateFromSlotsRequest {
    pub name: String,
    pub network: String,
    pub policy: SlotPolicyInput,
    pub slots: Vec<SlotInput>,
    /// Required (8+ characters) when any slot carries a secret.
    #[serde(default)]
    pub encrypt_passphrase: Option<String>,
}

pub async fn cmd_local_create_from_slots(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateFromSlotsRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.create_from_slots(
        &request.name,
        network,
        request.policy,
        request.slots,
        request.encrypt_passphrase.as_deref(),
    )
    .await
    .map_err(map_err)
}

#[derive(Debug, Deserialize)]
pub struct EncryptKeyBackupRequest {
    pub secret: HotSecret,
    pub password: String,
    pub fingerprint: String,
    pub derivation_path: String,
    pub xpub: String,
}

/// Produce the contents of a password-encrypted `.sigvault-key` file.
pub async fn cmd_local_encrypt_key_backup(
    app_state: State<'_, ApplicationState>,
    request: EncryptKeyBackupRequest,
) -> Result<String, String> {
    app_state.require_local_mode().await?;
    slots::encrypt_key_backup(
        &request.secret,
        &request.password,
        &request.fingerprint,
        &request.derivation_path,
        &request.xpub,
    )
    .map_err(map_err)
}

#[derive(Debug, Deserialize)]
pub struct DecryptKeyBackupRequest {
    pub contents: String,
    pub password: String,
}

pub async fn cmd_local_decrypt_key_backup(
    app_state: State<'_, ApplicationState>,
    request: DecryptKeyBackupRequest,
) -> Result<KeyBackupContents, String> {
    app_state.require_local_mode().await?;
    slots::decrypt_key_backup(&request.contents, &request.password).map_err(map_err)
}

#[derive(Debug, Deserialize)]
pub struct ExportWalletBackupRequest {
    pub wallet_id: String,
}

/// Secret-free wallet backup JSON (descriptors + slot labels). Reads
/// metadata only, so it works on locked wallets.
pub async fn cmd_local_export_wallet_backup(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: ExportWalletBackupRequest,
) -> Result<String, String> {
    app_state.require_local_mode().await?;
    let mgr = manager_for(&app, &app_state)?;
    let meta = mgr
        .read_metadata(&WalletId::from(request.wallet_id))
        .map_err(map_err)?;
    let backup = WalletBackup::new(
        meta.name,
        meta.network,
        meta.policy_type,
        meta.external_descriptor,
        meta.internal_descriptor,
        meta.slots
            .into_iter()
            .map(|s| WalletBackupSlot {
                label: s.label,
                fingerprint: s.fingerprint,
                derivation_path: s.derivation_path,
                xpub: s.xpub,
                source: s.source,
            })
            .collect(),
        meta.created_at,
    );
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_wallet::bitcoin::hashes::Hash;
    use bdk_wallet::bitcoin::{absolute, transaction, Amount, OutPoint, ScriptBuf, Sequence};
    use bdk_wallet::bitcoin::{TxIn, TxOut, Txid, Witness};
    use bdk_wallet::{SignOptions, Wallet};

    const DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
    const CHANGE: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

    /// A wallet holding one unconfirmed 100k sat coin, and an unsigned
    /// PSBT paying 40k of it to an outside address.
    fn wallet_and_psbt() -> (Wallet, Psbt) {
        let mut wallet = Wallet::create(DESC, CHANGE)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .unwrap();
        let addr = wallet.reveal_next_address(KeychainKind::External).address;
        let funding = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::new(Txid::from_byte_array([7; 32]), 0),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: addr.script_pubkey(),
            }],
        };
        wallet.apply_unconfirmed_txs([(funding, 1)]);
        let dest = Address::from_str("bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080")
            .unwrap()
            .require_network(Network::Regtest)
            .unwrap();
        let mut b = wallet.build_tx();
        b.fee_rate(FeeRate::from_sat_per_vb(2).unwrap());
        b.add_recipient(dest.script_pubkey(), Amount::from_sat(40_000));
        let psbt = b.finish().unwrap();
        (wallet, psbt)
    }

    #[test]
    fn unsigned_psbt_is_not_ready_to_broadcast() {
        let (_, psbt) = wallet_and_psbt();
        let r = inspect_signed_psbt(&psbt.to_string()).unwrap();
        assert!(!r.complete);
        assert_eq!(r.txid, None);
        assert!(r.detail.is_some());
        // The fee is known before signing: inputs carry their UTXOs.
        assert!(r.fee_sat.is_some_and(|f| f > 0));
    }

    #[test]
    fn estimated_vsize_bounds_the_signed_size() {
        let (wallet, mut psbt) = wallet_and_psbt();
        let desc = wallet.public_descriptor(KeychainKind::External).to_string();
        let est = estimate_signed_vsize(&psbt.to_string(), &desc).unwrap();
        let opts = SignOptions {
            try_finalize: false,
            ..Default::default()
        };
        wallet.sign(&mut psbt, opts).unwrap();
        let actual = inspect_signed_psbt(&psbt.to_string())
            .unwrap()
            .vsize
            .unwrap();
        // An upper bound, off by at most a byte per signature (ECDSA
        // signature length varies).
        assert!(est >= actual && est - actual <= 2, "{est} vs {actual}");
        assert_eq!(estimate_signed_vsize("nonsense", &desc), None);
    }

    #[test]
    fn signed_psbt_finalizes_without_broadcasting() {
        let (wallet, mut psbt) = wallet_and_psbt();
        let unsigned_txid = psbt.unsigned_tx.compute_txid();
        let opts = SignOptions {
            try_finalize: false,
            ..Default::default()
        };
        wallet.sign(&mut psbt, opts).unwrap();
        let signed = psbt.to_string();

        let r = inspect_signed_psbt(&signed).unwrap();
        assert!(r.complete, "{r:?}");
        // Segwit: signing doesn't change the txid.
        assert_eq!(r.txid, Some(unsigned_txid.to_string()));
        assert_eq!(r.fee_sat, Some(psbt.fee().unwrap().to_sat()));
        assert!(r.vsize.is_some_and(|v| v > 100));
        assert_eq!(r.detail, None);
        // The PSBT handed in is untouched (still not finalized), so it can
        // go to cmd_local_broadcast_psbt or another signer afterwards.
        let again = Psbt::from_str(&signed).unwrap();
        assert!(again.inputs[0].final_script_witness.is_none());
        assert!(finalize_psbt(&again).is_ok());
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(inspect_signed_psbt("not a psbt").is_err());
    }
}
