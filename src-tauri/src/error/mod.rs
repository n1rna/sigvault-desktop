// Centralized error handling for the application

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("WebSocket already active")]
    WebsocketAlreadyActive,

    #[error("Authorization failed")]
    AuthorizationFailed,

    #[error("Empty token")]
    EmptyToken,

    #[error("Machine authorization failed")]
    MachineAuthorizationFailed,

    #[error("Machine not registered")]
    MachineNotRegistered,

    #[error("WebSocket connection error: {0}")]
    WebsocketConnection(String),

    #[error("Failed to fetch remote sessions")]
    FetchRemoteSessionsFailed,

    #[error("Hardware wallet error: {0}")]
    HardwareWalletError(String),

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
    #[serde(rename = "error_empty_token")]
    EmptyToken,
    #[serde(rename = "error_machine_authorization_failed")]
    MachineAuthorizationFailed,
    #[serde(rename = "error_machine_not_registered")]
    MachineNotRegistered,
    #[serde(rename = "error_websocket_connection")]
    WebsocketConnection,
    #[serde(rename = "error_fetch_remote_sessions_failed")]
    FetchRemoteSessionsFailed,
    #[serde(rename = "error_hardware_wallet")]
    HardwareWalletError,
}

impl From<AppError> for AppErrorCode {
    fn from(error: AppError) -> Self {
        match error {
            AppError::WebsocketAlreadyActive => AppErrorCode::WebsocketAlreadyActive,
            AppError::AuthorizationFailed => AppErrorCode::AuthorizationFailed,
            AppError::EmptyToken => AppErrorCode::EmptyToken,
            AppError::MachineAuthorizationFailed => AppErrorCode::MachineAuthorizationFailed,
            AppError::MachineNotRegistered => AppErrorCode::MachineNotRegistered,
            AppError::WebsocketConnection(_) => AppErrorCode::WebsocketConnection,
            AppError::FetchRemoteSessionsFailed => AppErrorCode::FetchRemoteSessionsFailed,
            AppError::HardwareWalletError(_) => AppErrorCode::HardwareWalletError,
            _ => AppErrorCode::AuthorizationFailed,
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
