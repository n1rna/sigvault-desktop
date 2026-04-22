// Environment selection commands.
//
// The desktop app supports multiple SigVault deployments. The user picks
// one before logging in; once authenticated, the choice is locked until
// they log out (or explicitly clear it from the login screen).

use log::{error, info, warn};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::env_config::{self, EnvConfig};
use crate::error::AppErrorCode;
use crate::state::ApplicationState;
use crate::storage::{EnvStorage, SecureStorage, StoredEnvData};
use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

use super::types::CommandResult;

#[derive(Serialize)]
pub struct EnvironmentsResponse {
    pub environments: Vec<EnvConfig>,
    pub selected_id: Option<String>,
}

/// List available environments. Tries the network first (with TTL cache)
/// and falls back to the on-disk cache. Also returns the currently
/// selected environment id, if any.
#[tauri::command]
pub async fn cmd_list_environments(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<EnvironmentsResponse, String> {
    let manifest = env_config::load(&app).await?;
    let selected_id = app_state
        .current_env
        .read()
        .await
        .as_ref()
        .map(|e| e.id.clone());

    Ok(EnvironmentsResponse {
        environments: manifest.environments,
        selected_id,
    })
}

/// Persist and apply the user's environment choice. Rejects if the user
/// is already authenticated — they must log out first.
#[tauri::command]
pub async fn cmd_set_environment(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    env_id: String,
) -> Result<CommandResult, String> {
    if app_state
        .auth_tokens
        .lock()
        .await
        .get_oauth_access_token()
        .is_some()
    {
        return Ok(CommandResult::error(
            "Cannot change environment while logged in",
            AppErrorCode::AuthorizationFailed,
        ));
    }

    let manifest = env_config::load(&app).await?;
    let env = manifest
        .environments
        .iter()
        .find(|e| e.id == env_id)
        .cloned()
        .ok_or_else(|| format!("Unknown environment: {env_id}"))?;

    if env.coming_soon {
        return Ok(CommandResult::error(
            "Environment is not yet available",
            AppErrorCode::AuthorizationFailed,
        ));
    }

    info!("Setting environment to {} ({})", env.id, env.network);

    // Defensive: any cached auth token belongs to a different backend
    // and is no longer valid here.
    let secure = SecureStorage::new(app.clone());
    let _ = secure.clear_auth_data().await;

    let env_storage = EnvStorage::new(app.clone());
    if let Err(e) = env_storage
        .store(&StoredEnvData {
            selected_env_id: Some(env.id.clone()),
        })
        .await
    {
        error!("Failed to persist env selection: {e}");
        return Err(format!("Failed to persist env selection: {e}"));
    }

    app_state.set_environment(env).await;

    if let Some(window) = app.get_webview_window("main") {
        update_state(
            &window,
            StateUpdateEvent::builder()
                .route(WindowApplicationRoute::Login)
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(CommandResult::success("Environment set"))
}

/// Clear the persisted environment selection and return to the picker.
/// Rejects while authenticated.
#[tauri::command]
pub async fn cmd_clear_environment(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    if app_state
        .auth_tokens
        .lock()
        .await
        .get_oauth_access_token()
        .is_some()
    {
        return Ok(CommandResult::error(
            "Cannot change environment while logged in",
            AppErrorCode::AuthorizationFailed,
        ));
    }

    let env_storage = EnvStorage::new(app.clone());
    if let Err(e) = env_storage.clear().await {
        warn!("Failed to clear env storage: {e}");
    }

    let secure = SecureStorage::new(app.clone());
    let _ = secure.clear_auth_data().await;

    app_state.clear_environment().await;

    if let Some(window) = app.get_webview_window("main") {
        update_state(
            &window,
            StateUpdateEvent::builder()
                .route(WindowApplicationRoute::SelectEnv)
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(CommandResult::success("Environment cleared"))
}
