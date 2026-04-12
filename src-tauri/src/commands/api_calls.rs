// Authentication-related commands

use log::{error, info};
use tauri::{State, WebviewWindow};

use crate::api::ApiClient;
use crate::error::AppErrorCode;
use crate::state::ApplicationState;
use crate::window::{update_state, StateUpdateEvent};

use super::types::CommandResult;

#[tauri::command]
pub async fn cmd_update_remote_sessions(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    let api_client = ApiClient::new();

    // Get OAuth access token from app state
    let oauth_access_token = app_state
        .auth_tokens
        .lock()
        .await
        .get_oauth_access_token()
        .ok_or("No OAuth access token available - user not authenticated")?;

    // Fetch remote sessions
    let remote_sessions_response = match api_client.fetch_remote_sessions(oauth_access_token).await
    {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to fetch remote sessions: {e:?}");
            return Ok(CommandResult::error(
                "Failed to fetch remote sessions",
                AppErrorCode::FetchRemoteSessionsFailed,
            ));
        }
    };

    // Update remote sessions in app state
    {
        let mut remote_sessions = app_state.remote_sessions.lock().await;
        *remote_sessions = remote_sessions_response.sessions;
        info!(
            "Updated remote sessions in app state: {} sessions",
            remote_sessions.len()
        );
    }

    // Navigate to remote sessions page
    update_state(
        &window,
        StateUpdateEvent::builder()
            // .route(WindowApplicationRoute::RemoteSessions)
            .socket_connected(false)
            .build(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(CommandResult::success(
        "Remote sessions updated successfully",
    ))
}
