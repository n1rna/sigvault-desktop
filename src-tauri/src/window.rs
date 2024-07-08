use tauri::Window;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WindowEventMessage {
    pub success: bool,
    pub message: WebsocketMessage,
    pub error: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
pub struct WebsocketMessage {
    pub message_type: String,
    pub payload: serde_json::Value,
}

pub fn emit_window_message(window: &Window, message: WindowEventMessage) {
    window
        .emit(
            "websocket_connection",
            serde_json::to_string(&message).unwrap(),
        )
        .expect("failed to emit event");
}
