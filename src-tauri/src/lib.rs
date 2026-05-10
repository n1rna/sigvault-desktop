// Sigvault Desktop Application
// Clean architecture with modular design

mod api;
mod app_mode;
mod commands;
mod config;
mod env_config;
mod error;
pub mod hwi;
mod kdf;
mod local_wallet;
mod machine;
mod oauth;
mod state;
mod storage;
mod websocket;
mod window;

use log::info;
use tauri::Manager;

use commands::*;
use local_wallet::commands::{
    cmd_local_broadcast_psbt, cmd_local_build_psbt, cmd_local_create_liana,
    cmd_local_create_multisig, cmd_local_create_singlesig_hw, cmd_local_create_wallet,
    cmd_local_create_watch_only, cmd_local_delete_wallet, cmd_local_get_balance,
    cmd_local_get_history, cmd_local_get_receive_address, cmd_local_get_settings,
    cmd_local_get_wallet_details, cmd_local_list_spending_paths, cmd_local_list_wallets,
    cmd_local_lock_wallet, cmd_local_recover_from_mnemonic, cmd_local_set_settings,
    cmd_local_sign_psbt_hardware, cmd_local_sign_psbt_software, cmd_local_sync,
    cmd_local_unlock_wallet,
};
use state::ApplicationState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    info!("Starting Sigvault Desktop application");

    // rustls 0.23 (pulled in transitively by electrum-client via
    // wallet-runtime, plus reqwest, tokio-tungstenite, and friends) does
    // not auto-select a CryptoProvider when more than one could be
    // active in the dep graph. Without this, the first TLS handshake
    // (e.g. cmd_local_sync against ssl://ers.regtest.sigvault.org) hits
    // a panic deep inside spawn_blocking. Install ring up front; the
    // .ok() swallows the "already installed" case so we don't fight
    // any provider another lib registered first.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        // .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                // Set default log level to Info for all dependencies
                .level(log::LevelFilter::Info)
                // Enable Debug logs only for our application modules
                .level_for("sigvault_desktop", log::LevelFilter::Debug)
                .level_for("sigvault_desktop_lib", log::LevelFilter::Debug)
                // To see debug logs from specific dependencies, add them like:
                // .level_for("oauth2", log::LevelFilter::Debug)
                // .level_for("axum", log::LevelFilter::Debug)
                .build(),
        )
        .plugin(
            tauri_plugin_stronghold::Builder::new(|password| {
                kdf::derive_password_key(password.as_bytes()).to_vec()
            })
            .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(ApplicationState::new())
        .invoke_handler(tauri::generate_handler![
            cmd_initialize_app,
            cmd_set_app_mode,
            cmd_clear_app_mode,
            cmd_list_environments,
            cmd_set_environment,
            cmd_clear_environment,
            cmd_authenticate,
            // cmd_check_authentication,
            // cmd_authenticate_with_session,
            cmd_logout,
            cmd_navigate,
            cmd_close_splashscreen,
            cmd_get_current_user,
            cmd_get_remote_sessions,
            cmd_update_remote_sessions,
            cmd_start_session_websocket_connection,
            cmd_submit_user_input_session_websocket,
            cmd_exit_session,
            cmd_discover_hardware_wallets,
            cmd_unlock_device,
            cmd_get_device_xpub,
            cmd_submit_device_registration,
            cmd_sign_psbt,
            cmd_submit_transaction_signature,
            cmd_get_ledger_hmacs,
            // Local wallet commands (QBL-216 onward).
            cmd_local_list_wallets,
            cmd_local_create_wallet,
            cmd_local_recover_from_mnemonic,
            cmd_local_create_singlesig_hw,
            cmd_local_create_watch_only,
            cmd_local_create_multisig,
            cmd_local_create_liana,
            cmd_local_unlock_wallet,
            cmd_local_lock_wallet,
            cmd_local_delete_wallet,
            cmd_local_get_receive_address,
            cmd_local_get_balance,
            cmd_local_get_history,
            cmd_local_get_settings,
            cmd_local_set_settings,
            cmd_local_sync,
            cmd_local_get_wallet_details,
            cmd_local_list_spending_paths,
            cmd_local_build_psbt,
            cmd_local_sign_psbt_software,
            cmd_local_sign_psbt_hardware,
            cmd_local_broadcast_psbt,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Application exited");
}
