//! Tauri command surface for local-mode wallets.
//!
//! Every `cmd_local_*` here gates on `require_local_mode` so the calls
//! cleanly fail when invoked from cloud mode. Implementations are kept
//! minimal — heavy lifting (wallet sync, PSBT pipeline, HW signing) lands
//! in QBL-218 / QBL-219 / QBL-220 and slots in via the manager + state
//! abstractions this module already exposes.

use std::str::FromStr;

use bdk_wallet::bitcoin::Network;
use bdk_wallet::KeychainKind;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::manager::{LocalWalletManager, ManagerError, WalletSummary};
use super::settings::{LocalSettings, SettingsStore};
use super::storage::WalletId;
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

#[derive(Debug, Serialize, Default)]
pub struct LocalBalance {
    pub confirmed_sat: u64,
    pub unconfirmed_sat: u64,
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
) -> Result<WalletId, String> {
    app_state.require_local_mode().await?;
    let network = parse_network(&request.network)?;
    let mgr = manager_for(&app, &app_state)?;

    match request.policy_type.as_str() {
        "singlesig_hot" => {
            let pass = request
                .passphrase
                .ok_or_else(|| "passphrase required for hot wallets".to_string())?;
            mgr.create_singlesig_hot(&request.name, network, pass.as_bytes())
                .await
                .map_err(map_err)
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
        .map_err(map_err)
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

/// Stub — proper balance lookup needs an Electrum sync (QBL-218). The
/// returned zeros are a placeholder so the dashboard can render before
/// the sync engine lands.
#[tauri::command]
pub async fn cmd_local_get_balance(
    _app: AppHandle,
    app_state: State<'_, ApplicationState>,
    _wallet_id: String,
) -> Result<LocalBalance, String> {
    app_state.require_local_mode().await?;
    Ok(LocalBalance::default())
}

/// Stub — transaction history requires a synced wallet (QBL-218). Empty
/// list for the v1 scaffold; replaced once sync is wired.
#[tauri::command]
pub async fn cmd_local_get_history(
    _app: AppHandle,
    app_state: State<'_, ApplicationState>,
    _wallet_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    app_state.require_local_mode().await?;
    Ok(Vec::new())
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
