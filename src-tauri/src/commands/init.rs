// Application initialization commands

use log::{debug, error, info, warn};
use std::error::Error;
use tauri::{AppHandle, Manager, State};

use crate::env_config;
use crate::state::ApplicationState;
use crate::storage::{EnvStorage, SecureStorage};
use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

use super::oauth::authenticate_user;
use super::types::CommandResult;

/// Initialize the application on startup. Loads the environment manifest,
/// hydrates the previously-selected environment (if any), and routes to
/// the right page:
///   - no env selected               → /select-env
///   - env selected, no valid token  → /login
///   - env selected, valid token     → /dashboard
#[tauri::command]
pub async fn cmd_initialize_app(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    debug!("Initializing application");

    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;

    // Initialize secure storage (no-op today, kept for symmetry with future
    // password-protected stores).
    let storage = SecureStorage::new(app.clone());
    if let Err(e) = storage.initialize(b"sigvault_default_key").await {
        error!("Failed to initialize storage: {e}");
        update_state(
            &window,
            StateUpdateEvent::builder()
                .route(WindowApplicationRoute::SelectEnv)
                .build(),
        )
        .await
        .map_err(|e: Box<dyn Error + Send + 'static>| format!("Failed to update state: {e}"))?;
        return Ok(CommandResult::success(
            "Initialization complete - storage error",
        ));
    }

    // Load the environments manifest. If even the cache is unavailable we
    // still send the user to the picker; the picker will surface the error
    // and offer a retry.
    let manifest = env_config::load(&app).await.ok();

    // Hydrate previously-selected environment.
    let env_storage = EnvStorage::new(app.clone());
    let stored_env = env_storage.load().await.unwrap_or_default();

    let mut selected_env = None;
    if let (Some(manifest), Some(env_id)) = (&manifest, &stored_env.selected_env_id) {
        match manifest.environments.iter().find(|e| &e.id == env_id) {
            Some(env) => selected_env = Some(env.clone()),
            None => {
                warn!("Stored env '{env_id}' no longer in manifest, clearing");
                let _ = env_storage.clear().await;
            }
        }
    }

    let Some(env) = selected_env else {
        update_state(
            &window,
            StateUpdateEvent::builder()
                .route(WindowApplicationRoute::SelectEnv)
                .build(),
        )
        .await
        .map_err(|e| format!("Failed to update state: {e}"))?;
        return Ok(CommandResult::success("Initialization complete - select env"));
    };

    app_state.set_environment(env).await;

    // Try existing token against the now-known environment.
    let mut authenticated = false;
    match storage.get_auth_data().await {
        Ok(auth_data) => {
            if let Some(access_token) = auth_data.oauth_access_token {
                info!("Found stored OAuth access token, verifying with user profile");
                match authenticate_user(app.clone(), app_state.inner().clone(), access_token).await
                {
                    Ok(_) => {
                        info!("User authenticated, routing to main page");
                        authenticated = true;
                    }
                    Err(e) => {
                        error!("Authentication failed: {e}");
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to retrieve auth data from storage: {e}");
        }
    }

    if !authenticated {
        debug!("No valid OAuth access token, routing to login");
        update_state(
            &window,
            StateUpdateEvent::builder()
                .route(WindowApplicationRoute::Login)
                .build(),
        )
        .await
        .map_err(|e| format!("Failed to update state: {e}"))?;
    }

    Ok(CommandResult::success("Initialization complete"))
}
