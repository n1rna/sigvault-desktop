// Centralized error handling for the application

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("WebSocket connection error: {0}")]
    WebsocketConnection(String),

    #[error("API request failed: {0}")]
    ApiError(#[from] reqwest::Error),

    #[error("Window error: {0}")]
    WindowError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AppErrorCode {
    #[serde(rename = "error_websocket_already_active")]
    WebsocketAlreadyActive,
    #[serde(rename = "error_authorization_failed")]
    AuthorizationFailed,
    #[serde(rename = "error_websocket_connection")]
    WebsocketConnection,
    #[serde(rename = "error_fetch_remote_sessions_failed")]
    FetchRemoteSessionsFailed,
    #[serde(rename = "error_hardware_wallet")]
    HardwareWalletError,
}

pub type Result<T> = std::result::Result<T, AppError>;
