// Session-related commands

use log::{debug, error, warn};
use tauri::{State, WebviewWindow};

use crate::api::ApiClient;
use crate::error::AppErrorCode;
use crate::machine::get_machine_information;
use crate::state::ApplicationState;
use crate::websocket::WebsocketHandler;
use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

use super::types::CommandResult;

#[tauri::command]
pub async fn cmd_start_session_websocket_connection(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    session_id: String,
) -> Result<CommandResult, String> {
    app_state.require_cloud_mode().await?;
    let app_state_clone = app_state.clone();
    let ws_thread = app_state_clone.ws_thread.lock().await;

    // Get OAuth access token for WebSocket authentication
    let oauth_access_token = app_state_clone
        .auth_tokens
        .lock()
        .await
        .get_oauth_access_token()
        .ok_or("No OAuth access token available - user not authenticated")?;

    // Check if WebSocket is already running
    if let Some(join_handle) = ws_thread.as_ref() {
        if !join_handle.inner().is_finished() {
            update_state(
                &window.clone(),
                StateUpdateEvent::builder()
                    .socket_connected(true)
                    .session_id(session_id.clone())
                    .build(),
            )
            .await
            .map_err(|e| e.to_string())?;

            return Ok(CommandResult::error(
                "Websocket connection already started",
                AppErrorCode::WebsocketAlreadyActive,
            ));
        }
    }

    // Fetch WebSocket token from API
    let machine_info = get_machine_information();
    let env = app_state_clone.require_env().await?;
    let api_client = ApiClient::new(env.api_base_url.clone());
    let websocket_token = match api_client
        .fetch_websocket_token(oauth_access_token, session_id.clone(), machine_info)
        .await
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

    drop(ws_thread); // Release the lock
    let window_clone = window.clone();
    let session_id_clone = session_id.clone();
    let ws_handler_clone = app_state.ws_handler.clone();

    // Start WebSocket connection in a new async task
    let join_handler = tauri::async_runtime::spawn(async move {
        let mut ws_handler = WebsocketHandler::new(
            window_clone.clone(),
            env.websocket_base_url(),
            session_id_clone.clone(),
            websocket_token.token,
        );

        ws_handler_clone.lock().await.replace(ws_handler.clone());

        if let Err(e) = ws_handler.run().await {
            error!("WebSocket connection error: {e:?}");

            update_state(
                &window_clone.clone(),
                StateUpdateEvent::builder()
                    .socket_connected(false)
                    .session_id(session_id_clone.clone())
                    .build(),
            )
            .await
            .map_err(|update_err| {
                error!("Failed to update state after WebSocket error: {update_err:?}");
            })
            .ok();
        }
    });

    app_state.ws_thread.lock().await.replace(join_handler);

    update_state(
        &window.clone(),
        StateUpdateEvent::builder()
            .route(WindowApplicationRoute::SessionDetails)
            .build(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(CommandResult::success("Websocket connection started"))
}

#[tauri::command]
pub async fn cmd_exit_session(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    app_state.require_cloud_mode().await?;
    // First, close the WebSocket connection
    {
        let mut ws_handler = app_state.ws_handler.lock().await;
        if let Some(mut handler) = ws_handler.take() {
            if let Err(e) = handler.close().await {
                error!("Failed to close websocket connection: {e:?}");
            }
        }
    }

    // Then, abort the WebSocket thread and wait for it to finish
    {
        let mut ws_thread = app_state.ws_thread.lock().await;
        if let Some(thread) = ws_thread.take() {
            thread.abort();
            match tokio::time::timeout(std::time::Duration::from_secs(5), thread).await {
                Ok(_) => debug!("WebSocket thread cleaned up successfully"),
                Err(e) => warn!("WebSocket thread cleanup timed out: {e:?}"),
            }
        }
    }

    // Finally, update the window state

    update_state(
        &window.clone(),
        StateUpdateEvent::builder()
            .route(WindowApplicationRoute::RemoteSessions)
            .socket_connected(false)
            .build(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(CommandResult::success("Session exited successfully"))
}

#[tauri::command]
pub async fn cmd_submit_user_input_session_websocket(
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    _session_id: String,
    input: String,
) -> Result<CommandResult, String> {
    app_state.require_cloud_mode().await?;
    let ws_handler = app_state.ws_handler.lock().await;

    if let Some(handler) = &*ws_handler {
        let message = serde_json::json!({
            "type": "session",
            "action": "submit",
            "payload": input
        });

        if let Err(e) = handler.send_message(&message).await {
            error!("Failed to send message: {e:?}");
            return Ok(CommandResult::error(
                "Failed to send message",
                AppErrorCode::WebsocketConnection,
            ));
        }

        return Ok(CommandResult::success("Message sent successfully"));
    }

    Ok(CommandResult::error(
        "No active websocket connection",
        AppErrorCode::WebsocketConnection,
    ))
}
