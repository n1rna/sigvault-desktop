//! Tauri command surface for local-mode wallets.
//!
//! Every `cmd_local_*` here gates on `require_local_mode` so the calls
//! cleanly fail when invoked from cloud mode. Implementations are kept
//! minimal — heavy lifting (wallet sync, PSBT pipeline, HW signing) lands
//! in QBL-218 / QBL-219 / QBL-220 and slots in via the manager + state
//! abstractions this module already exposes.

use std::str::FromStr;
use std::sync::Arc;

use bdk_wallet::bitcoin::Network;
use bdk_wallet::chain::ChainPosition;
use bdk_wallet::KeychainKind;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::manager::{LocalWalletManager, ManagerError, WalletSummary};
use super::settings::{LocalSettings, SettingsStore};
use super::storage::WalletId;
use super::sync::{run_sync, ClosureSink, ProgressSink, SyncProgress, SyncSummary};
use crate::state::ApplicationState;

// ---------- request / response payloads ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWalletRequest {
    pub name: String,
    pub network: String,
    /// `singlesig_hot` is the only fully-implemented variant in QBL-216.
    /// `multisig`, `liana`, `watch_only` return `UnsupportedPolicy`
    /// until QBL-224 / QBL-225 / QBL-226 ship the corresponding flows.
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
            "policy type '{}' not yet supported in v1 (lands in a later milestone ticket)",
            request.policy_type
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
        unconfirmed_sat: (balance.trusted_pending
            + balance.untrusted_pending
            + balance.immature)
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
            let fee_sat = wallet
                .calculate_fee(tx)
                .map(|f| f.to_sat())
                .unwrap_or(0);
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
