use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tauri::Window;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum MessageType {
    AuthorizationSuccess,
    ConnectionOpened,
    ConnectionClosed,
    TextMessage,
    BackendCommand,
    WebsocketCommand,
    SetApplicationState, // Add other message types as needed
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WindowApplicationRoute {
    #[serde(rename = "Loading")]
    Loading,
    #[serde(rename = "MainPage")]
    MainPage,
    #[serde(rename = "MachineRegistration")]
    MachineRegistration,
    #[serde(rename = "RemoteSessions")]
    RemoteSessions,
}

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to emit event: {0}")]
    EmitError(#[from] tauri::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Window state error: {0}")]
    StateError(String),
}

pub type WindowResult<T> = Result<T, Box<dyn std::error::Error + Send>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowApplicationState {
    pub route: WindowApplicationRoute,
    pub socket_connected: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BackendEventMessage {
    pub message_type: MessageType,
    pub payload: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WindowEventMessage {
    pub success: bool,
    pub message: BackendEventMessage,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QueuedWindowMessage {
    pub id: String,
    pub message: WindowEventMessage,
}

pub struct MessageQueue {
    queue: VecDeque<QueuedWindowMessage>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn push(&mut self, message: WindowEventMessage) -> String {
        let id = Uuid::new_v4().to_string();
        let queued_message = QueuedWindowMessage {
            id: id.clone(),
            message,
        };
        self.queue.push_back(queued_message);
        id
    }

    pub fn pop(&mut self) -> Option<QueuedWindowMessage> {
        self.queue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

pub struct WindowState {
    pub message_queue: MessageQueue,
    pub is_processing: bool,
}

impl WindowState {
    pub fn new() -> Self {
        Self {
            message_queue: MessageQueue::new(),
            is_processing: false,
        }
    }
}

pub type SharedWindowState = Arc<Mutex<WindowState>>;

pub fn create_shared_window_state() -> SharedWindowState {
    Arc::new(Mutex::new(WindowState::new()))
}

pub async fn emit_window_message(
    window: &Window,
    message: WindowEventMessage,
    state: &SharedWindowState,
) -> WindowResult<()> {
    let mut window_state = state.lock().await;
    let _id = window_state.message_queue.push(message);

    if !window_state.is_processing {
        window_state.is_processing = true;
        drop(window_state); // Release the lock before calling process_next_message
        process_next_message(window, state).await?;
    }

    Ok(())
}

pub async fn process_next_message(window: &Window, state: &SharedWindowState) -> WindowResult<()> {
    let mut window_state = state.lock().await;
    if let Some(queued_message) = window_state.message_queue.pop() {
        debug!("Processing message: {:?}", queued_message);
        window.emit(
            "backend_connection",
            serde_json::json!({
                "id": queued_message.id,
                "message": queued_message.message,
            }),
        ).unwrap();
    } else {
        window_state.is_processing = false;
    }
    Ok(())
}

pub async fn set_window_application_state(
    window: &Window,
    state: &WindowApplicationState,
    window_state: &SharedWindowState,
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
    window: &Window,
    command: String,
    command_payload: serde_json::Value,
    window_state: &SharedWindowState,
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
