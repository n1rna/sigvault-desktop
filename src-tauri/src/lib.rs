// Sigvault Desktop Application
// Clean architecture with modular design

mod api;
mod commands;
mod config;
mod error;
pub mod hwi;
mod machine;
mod oauth;
mod state;
mod storage;
mod websocket;
mod window;

use log::info;
use tauri::Manager;

use commands::*;
use oauth::OAuthState;
use state::ApplicationState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    info!("Starting Sigvault Desktop application");

    // Initialize OAuth state (values burned in at build time from .env)
    let oauth_state = OAuthState::new(
        env!("OAUTH2_CLIENT_ID").to_string(),
        env!("OAUTH2_AUTH_URL").to_string(),
        env!("OAUTH2_TOKEN_URL").to_string(),
    )
    .expect("Failed to initialize OAuth state");

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
                // In production, this should use a proper key derivation function
                let mut key = vec![0u8; 32];
                let password_bytes = password.as_bytes();
                let len = password_bytes.len().min(32);
                key[..len].copy_from_slice(&password_bytes[..len]);
                key
            })
            .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(ApplicationState::new())
        .manage(oauth_state)
        .invoke_handler(tauri::generate_handler![
            cmd_initialize_app,
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
            cmd_get_ledger_hmacs
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
