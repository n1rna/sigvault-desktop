// Window types and enums

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum MessageType {
    ConnectionOpened,
    ConnectionClosed,
    TextMessage,
    SessionMessage,
    BackendCommand,
    SetApplicationState,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum SessionMessageType {
    WorkflowMessage,
    SystemMessage,
    AuthenticationMessage,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WindowApplicationRoute {
    #[serde(rename = "Loading")]
    Loading,
    #[serde(rename = "Login")]
    Login,
    #[serde(rename = "MainPage")]
    MainPage,
    #[serde(rename = "MachineRegistration")]
    MachineRegistration,
    #[serde(rename = "RemoteSessions")]
    RemoteSessions,
    #[serde(rename = "SessionDetails")]
    SessionDetails,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WindowApplicationState {
    pub route: Option<WindowApplicationRoute>,
    pub socket_connected: bool,
    pub current_session_id: Option<String>,
    pub current_session_type: Option<String>,
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
