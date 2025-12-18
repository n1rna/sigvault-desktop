// WebSocket message types

use serde::{Deserialize, Serialize};

use crate::window::SessionMessageType;

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeSessionMessage {
    pub r#type: String,
    pub action: String,
    pub payload: String,
}

impl InitializeSessionMessage {
    pub fn new() -> Self {
        Self {
            r#type: "session".to_string(),
            action: "initialize".to_string(),
            payload: "{}".to_string(),
        }
    }
}

impl Default for InitializeSessionMessage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowMessagePayload {
    pub r#type: String,
    pub step: u32,
    pub requirements: serde_json::Value,
    pub finished: bool,
    pub success: bool,
    pub message: Option<String>,
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct WorkflowMessagePayload(DeviceCreationPayloadData);

#[derive(Debug, Serialize, Deserialize)]
pub struct GenericWebsocketMessage {
    pub message_type: SessionMessageType,
    pub payload: Option<WorkflowMessagePayload>,
    pub success: bool,
    pub message: Option<String>,
}
