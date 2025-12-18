// Sigvault Desktop Application
// Clean architecture with modular design

mod api;
mod commands;
mod error;
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

    // Initialize OAuth state
    // TODO: Get these from environment variables or configuration
    let oauth_state = OAuthState::new(
        std::env::var("OAUTH2_CLIENT_ID").unwrap_or_else(|_| "346819126007796376".to_string()),
        std::env::var("OAUTH2_AUTH_URL").unwrap_or_else(|_| {
            "https://sigvault-jsyfl0.us1.zitadel.cloud/oauth/v2/authorize".to_string()
        }),
        std::env::var("OAUTH2_TOKEN_URL").unwrap_or_else(|_| {
            "https://sigvault-jsyfl0.us1.zitadel.cloud/oauth/v2/token".to_string()
        }),
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
            cmd_exit_session
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
