// Window state and message management

pub mod events;
pub mod types;

pub use events::*;
pub use types::*;

use log::debug;
use tauri::{Emitter, WebviewWindow};

type WindowResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send>>;

/// Update application state and emit event to frontend
pub async fn update_state(window: &WebviewWindow, event: StateUpdateEvent) -> WindowResult<()> {
    debug!("Updating application state: {:?}", event);

    // Emit state update event
    window
        .emit("app_event", AppEvent::state_update(event))
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

    Ok(())
}

pub async fn update_session_state(window: &WebviewWindow, event: SessionEvent) -> WindowResult<()> {
    debug!("Updating active session state: {:?}", event);

    // Emit state update event
    window
        .emit("app_event", AppEvent::session(event))
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

    Ok(())
}
