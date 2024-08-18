use log::debug;
use serde::{Deserialize, Serialize};
use tauri::Window;

#[derive(Debug, Deserialize, Serialize)]
pub enum MessageType {
    AuthorizationSuccess,
    ConnectionOpened,
    ConnectionClosed,
    TextMessage,
    BackendCommand,
    WebsocketCommand,
    SetApplicationState
    // Add other message types as needed
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WindowApplicationRoute {
    #[serde(rename = "Loading")]
    Loading,
    #[serde(rename = "MainPage")]
    MainPage,
    #[serde(rename = "MachineRegistration")]
    MachineRegistration,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowApplicationState {
    pub route: WindowApplicationRoute,
    pub socket_connected: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WindowEventMessage {
    pub success: bool,
    pub message: BackendEventMessage,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BackendEventMessage {
    pub message_type: MessageType,
    pub payload: serde_json::Value,
}

pub fn emit_window_message(window: &Window, message: WindowEventMessage) {
    window
        .emit(
            "backend_connection",
            serde_json::to_string(&message).unwrap(),
        )
        .expect("failed to emit event");
}

pub fn set_window_application_state(window: &Window, state: &WindowApplicationState) {
    debug!("Setting window application state: {:?}", state);
    emit_window_message(
        &window,
        WindowEventMessage {
            success: true,
            message: BackendEventMessage {
                message_type: MessageType::SetApplicationState,
                payload: serde_json::to_value(state).unwrap(),
            },
            error: None,
        },
    );
}

pub fn send_backend_command(window: &Window, command: String, command_payload: serde_json::Value) {
    emit_window_message(
        &window,
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
    );
}
