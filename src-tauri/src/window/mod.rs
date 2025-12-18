// Window state and message management

pub mod events;
pub mod message_queue;
pub mod state;
pub mod types;

pub use events::*;
pub use state::{create_shared_window_state, SharedWindowState, WindowState};
pub use types::*;

use log::debug;
use serde_json;
use tauri::{Emitter, WebviewWindow};

type WindowResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send>>;

pub async fn emit_window_message(
    window: &WebviewWindow,
    message: WindowEventMessage,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    if window_state.message_queue.is_empty() {
        window_state.message_queue.push(message);
        process_next_message(window, window_state).await?;
    } else {
        window_state.message_queue.push(message);
    }

    Ok(())
}

pub async fn process_next_message(
    window: &WebviewWindow,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    if let Some(queued_message) = window_state.message_queue.pop() {
        debug!("Processing message: {:?}", queued_message);
        window
            .emit(
                "backend_connection",
                serde_json::json!({
                    "id": queued_message.id,
                    "message": queued_message.message,
                }),
            )
            .unwrap();
    }
    Ok(())
}

pub async fn set_window_application_state(
    window: &WebviewWindow,
    state: &WindowApplicationState,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    debug!("Setting window application state: {:?}", state);
    emit_window_message(
        window,
        WindowEventMessage {
            success: true,
            message: BackendEventMessage {
                message_type: MessageType::SetApplicationState,
                payload: serde_json::to_value(state).unwrap(),
            },
            error: None,
        },
        window_state,
    )
    .await
}

pub async fn send_backend_command(
    window: &WebviewWindow,
    command: String,
    command_payload: serde_json::Value,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    emit_window_message(
        window,
        WindowEventMessage {
            success: true,
            message: BackendEventMessage {
                message_type: MessageType::BackendCommand,
                payload: serde_json::json!({
                    "command": command,
                    "payload": command_payload,
                }),
            },
            error: None,
        },
        window_state,
    )
    .await
}

/// Update application state and emit event to frontend
pub async fn update_state(
    window: &WebviewWindow,
    event: StateUpdateEvent,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    debug!("Updating application state: {:?}", event);

    // Update window state route if provided
    if let Some(route) = &event.route {
        window_state.set_route(route.clone());
    }

    // Emit state update event
    window
        .emit("app_event", AppEvent::state_update(event))
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

    Ok(())
}

pub async fn update_session_state(
    window: &WebviewWindow,
    event: SessionEvent,
) -> WindowResult<()> {
    debug!("Updating active session state: {:?}", event);

    // Emit state update event
    window
        .emit("app_event", AppEvent::session(event))
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

    Ok(())
}
