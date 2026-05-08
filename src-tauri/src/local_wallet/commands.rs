//! Tauri command surface for local-mode wallets.
//!
//! Every `cmd_local_*` here gates on `require_local_mode` so the calls
//! cleanly fail when invoked from cloud mode. Implementations are kept
//! minimal — heavy lifting (wallet sync, PSBT pipeline, HW signing) lands
//! in QBL-218 / QBL-219 / QBL-220 and slots in via the manager + state
//! abstractions this module already exposes.

use std::str::FromStr;
use std::sync::Arc;

use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::{Address, Amount, FeeRate, Network, Psbt};
use bdk_wallet::chain::ChainPosition;
use bdk_wallet::miniscript::psbt::PsbtExt;
use bdk_wallet::KeychainKind;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use wallet_runtime::{
    add_xprv_signer, analyze_for_signing, list_spending_paths, sign_psbt, ElectrumClient,
    SignerKind, SpendingPath,
};

use super::manager::{
    derive_account_from_mnemonic, LianaKeyInput, LianaRecoveryPath, LianaSpendingPath,
    LocalWalletManager, ManagerError, WalletSummary,
};
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
    /// Liana, and watch-only have dedicated commands
    /// (`cmd_local_create_multisig`, `cmd_local_create_liana`,
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
    pub passphrase: String,
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
pub struct LianaKeyInputDto {
    pub fingerprint: String,
    pub xpub: String,
    pub derivation_path: String,
}

#[derive(Debug, Deserialize)]
pub struct LianaSpendingPathDto {
    pub keys: Vec<LianaKeyInputDto>,
    pub threshold: u32,
}

#[derive(Debug, Deserialize)]
pub struct LianaRecoveryPathDto {
    pub timelock_blocks: u16,
    pub path: LianaSpendingPathDto,
}

#[derive(Debug, Deserialize)]
pub struct CreateLianaWalletRequest {
    pub name: String,
    pub network: String,
    pub primary: LianaSpendingPathDto,
    pub recoveries: Vec<LianaRecoveryPathDto>,
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

#[tauri::command]
pub async fn cmd_local_list_wallets(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<Vec<WalletSummary>, String> {
    app_state.require_local_mode().await?;
    let mgr = manager_for(&app, &app_state)?;
    mgr.list_wallets().await.map_err(map_err)
}

#[tauri::command]
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
        "multisig" | "liana" | "watch_only" => Err(format!(
            "policy type '{}' uses a dedicated command — call cmd_local_create_{} instead",
            request.policy_type, request.policy_type
        )),
        other => Err(format!("unknown policy_type '{other}'")),
    }
}

#[tauri::command]
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
        request.passphrase.as_bytes(),
    )
    .await
    .map_err(map_err)
}

/// Create a singlesig wallet from a hardware device's xpub (QBL-220).
/// The frontend collects fingerprint + xpub + derivation path via the
/// existing cloud-mode HW commands (`cmd_discover_hardware_wallets`,
/// `cmd_unlock_device`, `cmd_get_device_xpub`) and then hands off here.
#[tauri::command]
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
#[tauri::command]
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

/// Create a Liana timelocked-policy wallet (QBL-225). Primary path is
/// spendable immediately; each recovery path becomes spendable after its
/// block-count timelock elapses. v1 collects all keys from hardware
/// devices or pasted xpubs (no hot keys for the primary path — see
/// QBL-235 for the related "unspendable primary" affordance).
#[tauri::command]
pub async fn cmd_local_create_liana(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    request: CreateLianaWalletRequest,
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;

    fn map_path(dto: LianaSpendingPathDto) -> LianaSpendingPath {
        LianaSpendingPath {
            keys: dto
                .keys
                .into_iter()
                .map(|k| LianaKeyInput {
                    fingerprint: k.fingerprint,
                    xpub: k.xpub,
                    derivation_path: k.derivation_path,
                })
                .collect(),
            threshold: dto.threshold,
        }
    }

    let primary = map_path(request.primary);
    let recoveries: Vec<LianaRecoveryPath> = request
        .recoveries
        .into_iter()
        .map(|r| LianaRecoveryPath {
            timelock_blocks: r.timelock_blocks,
            path: map_path(r.path),
        })
        .collect();

    mgr.create_liana(&request.name, network, primary, recoveries)
        .await
        .map_err(map_err)
}

/// Create a watch-only wallet from descriptor strings the user pasted
/// (QBL-226). Useful for monitoring an external wallet without holding
/// the keys — e.g. an existing Sparrow or Liana setup imported here so
/// you can see balance and history. Cannot sign.
#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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
#[tauri::command]
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
#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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
#[tauri::command]
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
    pub fingerprints: Vec<String>,
    pub has_hot_keys: bool,
}

#[tauri::command]
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
        external_descriptor: meta.external_descriptor,
        internal_descriptor: meta.internal_descriptor,
        fingerprints: meta.fingerprints,
        has_hot_keys: meta.has_hot_keys,
    })
}

// ---------- Spending-path enumeration (QBL-234) ----------
//
// Surfaces the available spending paths for a wallet so the SendWizard
// can present a path picker for multi-path descriptors (Liana primary
// vs recovery, taproot multi-leaf). Single-path wallets get one entry
// the wizard auto-selects.

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
/// Liana wallets surface one path per branch (primary + each recovery
/// in ascending-timelock order). The wallet must be unlocked — the
/// command needs BDK's external-policy id to populate `policy_path`
/// for multi-branch descriptors.
#[tauri::command]
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
    // entries for Liana branches (TxBuilder::policy_path keys on it).
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

    let paths = list_spending_paths(
        &meta.policy_type,
        &meta.external_descriptor,
        &meta.fingerprints,
        policy_id.as_deref(),
    )
    .map_err(|e| format!("enumerate spending paths: {e}"))?;

    Ok(paths.into_iter().map(SpendingPathDto::from).collect())
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
    /// descriptors (Liana primary vs recovery, taproot multi-leaf) so
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

#[tauri::command]
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
        // (Liana primary vs recovery, taproot multi-leaf). The same map
        // is applied to both keychains so change addresses go through
        // the matching path — for Liana this means change inherits the
        // primary's selection, which is what the user expects (you
        // wouldn't want recovery-only change).
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

#[tauri::command]
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
    let mnemonic_bytes = read_seed_file(&layout, request.passphrase.as_bytes())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "wallet has no on-device seed (watch-only or HW-only)".to_string())?;
    let mnemonic_str =
        std::str::from_utf8(&mnemonic_bytes).map_err(|e| format!("seed not utf-8: {}", e))?;
    let (account_xpriv, fingerprint) =
        derive_account_from_mnemonic(network, mnemonic_str).map_err(|e| e.to_string())?;

    let mut psbt =
        Psbt::from_str(&request.psbt_base64).map_err(|e| format!("parse psbt: {}", e))?;

    let handle = app_state
        .local_wallet_state
        .get(&id)
        .await
        .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()).to_string())?;
    let mut guard = handle.lock().await;

    let analysis = analyze_for_signing(&guard.wallet, &psbt, &fingerprint);
    // For singlesig segwit-v0 (the only QBL-219 case) all of these will
    // be SegwitV0 derivations on either the External or Internal chain.
    for (keychain, index) in &analysis.required_derivations {
        add_xprv_signer(
            &mut guard.wallet,
            &account_xpriv,
            *keychain,
            *index,
            SignerKind::SegwitV0,
        )
        .map_err(|e| format!("add_xprv_signer: {}", e))?;
    }

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
#[tauri::command]
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

#[tauri::command]
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

    let mut psbt =
        Psbt::from_str(&request.psbt_base64).map_err(|e| format!("parse psbt: {}", e))?;

    // Finalize via miniscript. wallet.sign(.., try_finalize: true) does
    // not handle multisig PSBTs correctly; PsbtExt::finalize_mut is the
    // canonical path used by walletrs's server too.
    let secp = Secp256k1::verification_only();
    if let Err(errors) = psbt.finalize_mut(&secp) {
        return Err(format!("finalize: {:?}", errors));
    }

    let tx = psbt
        .extract_tx()
        .map_err(|e| format!("extract_tx: {}", e))?;

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
