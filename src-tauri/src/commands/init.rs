// Application initialization commands

use log::{debug, error, info};
use tauri::{AppHandle, Manager, State};

use crate::state::ApplicationState;
use crate::storage::SecureStorage;
use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

use super::oauth::authenticate_user;
use super::types::CommandResult;

/// Initialize the application on startup
/// Checks authentication status and routes to appropriate page
#[tauri::command]
pub async fn cmd_initialize_app(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    debug!("Initializing application");

    // Get the main window
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;

    // Get window state

    // Initialize secure storage
    let storage = SecureStorage::new(app.clone());
    if let Err(e) = storage.initialize(b"sigvault_default_key").await {
        error!("Failed to initialize storage: {}", e);

        // Route to login on storage error
        let event = StateUpdateEvent::builder()
            .route(WindowApplicationRoute::Login)
            .build();

        let mut window_state = app_state.window_state.lock().await;
        update_state(&window, event, &mut window_state)
            .await
            .map_err(|e| format!("Failed to update state: {}", e))?;

        return Ok(CommandResult::success(
            "Initialization complete - login required",
        ));
    }

    // Release the window state lock before calling the shared function
    let mut authenticated = false;
    // Check if we have stored OAuth access token
    match storage.get_auth_data().await {
        Ok(auth_data) => {
            if let Some(access_token) = auth_data.oauth_access_token {
                info!("Found stored OAuth access token, verifying with user profile");

                // Release lock before calling authenticate_user
                // drop(window_state);
                // Use shared authentication logic (handles success and failure)
                match authenticate_user(app.clone(), app_state.inner().clone(), access_token).await
                {
                    Ok(_) => {
                        info!("User authenticated successfully, routing to main page");
                        authenticated = true;
                    }
                    Err(e) => {
                        error!("Authentication failed: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to retrieve auth data from storage: {}", e);
        }
    }

    if !authenticated {
        debug!("No OAuth access token found, routing to login");

        let event = StateUpdateEvent::builder()
            .route(WindowApplicationRoute::Login)
            .build();

        let mut window_state = app_state.window_state.lock().await;
        update_state(&window, event, &mut window_state)
            .await
            .map_err(|e| format!("Failed to update state: {}", e))?;
    }

    Ok(CommandResult::success(
        "Initialization complete - login required",
    ))
}
