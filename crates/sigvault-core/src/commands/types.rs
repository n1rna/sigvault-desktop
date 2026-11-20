// Command result types

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppErrorCode;

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    pub error: Option<AppErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl CommandResult {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            error: None,
            data: None,
        }
    }

    pub fn error(message: impl Into<String>, error_code: AppErrorCode) -> Self {
        Self {
            success: false,
            message: message.into(),
            error: Some(error_code),
            data: None,
        }
    }
}
