// Window state and message management - New improved version

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

/// Emit an event to the frontend
pub async fn emit_event(
    window: &WebviewWindow,
    event: AppEvent,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    let event_payload = serde_json::to_value(&event)?;

    if window_state.message_queue.is_empty() && !window_state.is_processing {
        window_state.is_processing = true;
        emit_event_immediate(window, event_payload).await?;
        window_state.is_processing = false;
    } else {
        // Queue the event
        window_state.message_queue.push_raw(event_payload);
    }

    Ok(())
}

/// Emit an event immediately without queueing
async fn emit_event_immediate(
    window: &WebviewWindow,
    event_payload: serde_json::Value,
) -> WindowResult<()> {
    debug!("Emitting event: {:?}", event_payload);
    window
        .emit("app_event", event_payload)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
    Ok(())
}

/// Process the next queued message
pub async fn process_next_message(
    window: &WebviewWindow,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    if let Some(event_payload) = window_state.message_queue.pop_raw() {
        emit_event_immediate(window, event_payload).await?;
    }
    Ok(())
}

/// Update the application state
pub async fn update_state(
    window: &WebviewWindow,
    state_update: StateUpdateEvent,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    debug!("Updating application state: {:?}", state_update);
    emit_event(window, AppEvent::state_update(state_update), window_state).await
}

/// Send a command to the frontend
pub async fn send_command(
    window: &WebviewWindow,
    command: impl Into<String>,
    payload: serde_json::Value,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    emit_event(
        window,
        AppEvent::command(CommandEvent::new(command, payload)),
        window_state,
    )
    .await
}

/// Send a session event to the frontend
pub async fn send_session_event(
    window: &WebviewWindow,
    message_type: SessionMessageType,
    payload: serde_json::Value,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    emit_event(
        window,
        AppEvent::session(SessionEvent::new(message_type, payload)),
        window_state,
    )
    .await
}

/// Send a notification to the frontend
pub async fn send_notification(
    window: &WebviewWindow,
    notification: NotificationEvent,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    emit_event(
        window,
        AppEvent::notification(notification),
        window_state,
    )
    .await
}

// Legacy compatibility functions
#[allow(deprecated)]
#[deprecated(note = "Use update_state instead")]
pub async fn set_window_application_state(
    window: &WebviewWindow,
    state: &WindowApplicationState,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    update_state(
        window,
        StateUpdateEvent {
            route: state.route.clone(),
            socket_connected: Some(state.socket_connected),
            session_id: state.current_session_id.clone(),
            session_type: state.current_session_type.clone(),
        },
        window_state,
    )
    .await
}

#[allow(deprecated)]
#[deprecated(note = "Use send_command instead")]
pub async fn send_backend_command(
    window: &WebviewWindow,
    command: String,
    command_payload: serde_json::Value,
    window_state: &mut WindowState,
) -> WindowResult<()> {
    send_command(window, command, command_payload, window_state).await
}
