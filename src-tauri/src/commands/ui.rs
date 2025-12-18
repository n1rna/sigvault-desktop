// UI-related commands

use log::{debug, error, info, warn};
use serde::Serialize;
use tauri::{Manager, State, WebviewWindow};

use crate::state::ApplicationState;
use crate::window;

#[derive(Serialize)]
pub struct UserInfo {
    pub id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
}

#[tauri::command]
pub async fn cmd_close_splashscreen(window: WebviewWindow) {
    info!("Attempting to close splashscreen");
    match window.get_webview_window("splashscreen") {
        Some(splashscreen) => {
            debug!("Splashscreen window found, closing");
            if let Err(e) = splashscreen.close() {
                error!("Failed to close splashscreen: {:?}", e);
            } else {
                info!("Splashscreen closed successfully");
            }
        }
        None => {
            warn!("No splashscreen window found");
        }
    }

    info!("Attempting to show main window");
    match window.get_webview_window("main") {
        Some(main_window) => {
            debug!("Main window found, showing");
            if let Err(e) = main_window.show() {
                error!("Failed to show main window: {:?}", e);
            } else {
                info!("Main window shown successfully");
            }
        }
        None => {
            error!("No window labeled 'main' found");
        }
    }
}

#[tauri::command]
pub async fn cmd_message_processed(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    message_id: String,
) -> Result<(), String> {
    debug!("Message processed: {}", message_id);
    let mut window_state = app_state.window_state.lock().await;

    // Process the next message if there is one
    if !window_state.message_queue.is_empty() {
        window::process_next_message(&window, &mut window_state)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        window_state.is_processing = false;
    }

    Ok(())
}

#[tauri::command]
pub async fn cmd_get_current_user(
    app_state: State<'_, ApplicationState>,
) -> Result<UserInfo, String> {
    let user_data = app_state.user_data.lock().await;

    Ok(UserInfo {
        id: user_data.id.clone(),
        email: user_data.email.clone(),
        name: user_data.name.clone(),
        username: user_data.username.clone(),
    })
}

#[tauri::command]
pub async fn cmd_get_remote_sessions(
    app_state: State<'_, ApplicationState>,
) -> Result<Vec<crate::api::types::RemoteSession>, String> {
    let remote_sessions = app_state.remote_sessions.lock().await;
    Ok(remote_sessions.clone())
}

#[tauri::command]
pub async fn cmd_navigate(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    route: String,
) -> Result<(), String> {
    use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

    let mut window_state = app_state.window_state.lock().await;

    // Parse the route string to WindowApplicationRoute
    let app_route = match route.as_str() {
        "Loading" => WindowApplicationRoute::Loading,
        "Login" => WindowApplicationRoute::Login,
        "MainPage" => WindowApplicationRoute::MainPage,
        "MachineRegistration" => WindowApplicationRoute::MachineRegistration,
        "RemoteSessions" => WindowApplicationRoute::RemoteSessions,
        "SessionDetails" => WindowApplicationRoute::SessionDetails,
        _ => return Err(format!("Invalid route: {}", route)),
    };

    update_state(
        &window,
        StateUpdateEvent::builder().route(app_route).build(),
        &mut window_state,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}
