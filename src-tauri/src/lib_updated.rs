// Sigvault Desktop Application
// Clean architecture with modular design

mod api;
mod commands;
mod error;
mod machine;
mod state;
mod storage;
mod websocket;
mod window;

use log::info;
use tauri::Manager;

use commands::*;
use state::ApplicationState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    info!("Starting Sigvault Desktop application");

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_stronghold::Builder::new(|password| {
            // In production, this should use a proper key derivation function
            let mut key = [0u8; 32];
            let password_bytes = password.as_bytes();
            let len = password_bytes.len().min(32);
            key[..len].copy_from_slice(&password_bytes[..len]);
            key
        }).build())
        .plugin(tauri_plugin_opener::init())
        .manage(ApplicationState::new())
        .invoke_handler(tauri::generate_handler![
            cmd_check_authentication,
            cmd_init_oauth,
            cmd_cancel_oauth,
            cmd_authenticate_with_session,
            cmd_logout,
            cmd_start_backend_authentication,
            cmd_register_new_machine,
            cmd_close_splashscreen,
            cmd_message_processed,
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
