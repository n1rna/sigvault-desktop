use log::{error, info};
use serde::Deserialize;
use serde_json;
use tauri::{State, AppHandle, WebviewWindow};

use crate::error::AppErrorCode;
use crate::hwi::{DeviceInfo, WalletConfig};
use crate::state::ApplicationState;

use super::types::CommandResult;

/// Optional wallet configuration for device discovery/signing
#[derive(Debug, Clone, Deserialize)]
pub struct WalletConfigInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub descriptor: Option<String>,
    #[serde(default)]
    pub hmac: Option<String>,
    #[serde(default)]
    pub ledger_hmacs: Option<std::collections::HashMap<String, String>>,
}

impl WalletConfigInput {
    fn decode_hmac_hex(hex_str: &str) -> Result<[u8; 32], String> {
        let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid HMAC hex: {e}"))?;
        if bytes.len() != 32 {
            return Err(format!("HMAC must be 32 bytes, got {}", bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }

    pub fn to_wallet_config(&self) -> Result<WalletConfig, String> {
        let hmac = match &self.hmac {
            Some(hex_str) => Some(Self::decode_hmac_hex(hex_str)?),
            None => None,
        };

        let ledger_hmacs = match &self.ledger_hmacs {
            Some(map) => {
                let mut decoded = std::collections::HashMap::new();
                for (fp, hex_str) in map {
                    decoded.insert(fp.clone(), Self::decode_hmac_hex(hex_str)?);
                }
                Some(decoded)
            }
            None => None,
        };

        Ok(WalletConfig {
            name: self
                .name
                .clone()
                .unwrap_or_else(|| "Unnamed Wallet".to_string()),
            descriptor: self.descriptor.clone(),
            hmac,
            ledger_hmacs,
        })
    }
}

/// Discover all connected hardware wallets with their states
#[tauri::command]
pub async fn cmd_discover_hardware_wallets(
    app: AppHandle,
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    wallet_config: Option<WalletConfigInput>,
) -> Result<CommandResult, String> {
    info!("Starting hardware wallet discovery");

    let config = match wallet_config {
        Some(input) => Some(input.to_wallet_config()?),
        None => None,
    };

    let hw_manager = app_state.require_hw_manager().await?;
    match hw_manager.discover_devices(config.as_ref(), Some(&app)).await {
        Ok(devices) => {
            info!("Successfully discovered {} device(s)", devices.len());
            let devices_json = serde_json::to_value(&devices)
                .map_err(|e| format!("Failed to serialize devices: {e}"))?;
            Ok(CommandResult {
                success: true,
                message: format!("Found {} hardware wallet(s)", devices.len()),
                data: Some(devices_json),
                error: None,
            })
        }
        Err(e) => {
            error!("Hardware wallet discovery failed: {e}");
            Ok(CommandResult::error(
                format!("Failed to discover devices: {e}"),
                AppErrorCode::HardwareWalletError,
            ))
        }
    }
}

/// Unlock a locked device (BitBox02 pairing, Jade PIN)
#[tauri::command]
pub async fn cmd_unlock_device(
    app: AppHandle,
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    device_id: String,
    wallet_config: Option<WalletConfigInput>,
) -> Result<CommandResult, String> {
    info!("Unlocking device: {device_id}");

    let config = match wallet_config {
        Some(input) => Some(input.to_wallet_config()?),
        None => None,
    };

    let hw_manager = app_state.require_hw_manager().await?;
    match hw_manager
        .unlock_device(&device_id, config.as_ref(), Some(&app))
        .await
    {
        Ok(device) => {
            info!("Successfully unlocked device: {device_id}");
            let device_json = serde_json::to_value(&device)
                .map_err(|e| format!("Failed to serialize device: {e}"))?;
            Ok(CommandResult {
                success: true,
                message: "Device unlocked successfully".to_string(),
                data: Some(device_json),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to unlock device: {e}");
            Ok(CommandResult::error(
                format!("Failed to unlock device: {e}"),
                AppErrorCode::HardwareWalletError,
            ))
        }
    }
}

#[tauri::command]
pub async fn cmd_get_device_xpub(
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    fingerprint: String,
    derivation_path: String,
) -> Result<CommandResult, String> {
    info!(
        "Extracting xpub for device {fingerprint} at path {derivation_path}"
    );

    let hw_manager = app_state.require_hw_manager().await?;
    match hw_manager
        .get_device_info(&fingerprint, &derivation_path)
        .await
    {
        Ok(device_info) => {
            info!("Successfully extracted device info for {fingerprint}");
            let device_info_json = serde_json::to_value(&device_info)
                .map_err(|e| format!("Failed to serialize device info: {e}"))?;

            Ok(CommandResult {
                success: true,
                message: "Device info extracted successfully".to_string(),
                data: Some(device_info_json),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to extract device info: {e}");
            Ok(CommandResult::error(
                format!("Failed to extract device info: {e}"),
                AppErrorCode::HardwareWalletError,
            ))
        }
    }
}

#[tauri::command]
pub async fn cmd_submit_device_registration(
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    session_id: String,
    device_info: DeviceInfo,
) -> Result<CommandResult, String> {
    info!("Submitting device registration for session {session_id}");

    let ws_handler = app_state.ws_handler.lock().await;

    if let Some(handler) = &*ws_handler {
        let payload = serde_json::json!({
            "xpub": device_info.xpub,
            "fingerprint": device_info.fingerprint,
            "derivation_path": device_info.derivation_path,
            "device_type": device_info.device_type,
        });

        let message = serde_json::json!({
            "type": "session",
            "action": "submit",
            "payload": payload.to_string()
        });

        if let Err(e) = handler.send_message(&message).await {
            error!("Failed to send device registration: {e:?}");
            return Ok(CommandResult::error(
                "Failed to send device registration",
                AppErrorCode::WebsocketConnection,
            ));
        }

        info!("Device registration submitted successfully");
        return Ok(CommandResult::success(
            "Device registration submitted successfully",
        ));
    }

    Ok(CommandResult::error(
        "No active websocket connection",
        AppErrorCode::WebsocketConnection,
    ))
}

/// Sign a PSBT with a hardware wallet
#[tauri::command]
pub async fn cmd_sign_psbt(
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    device_id: String,
    _fingerprint: String,
    psbt: String, // Base64 encoded
    _wallet_config: Option<WalletConfigInput>,
) -> Result<CommandResult, String> {
    info!("Signing PSBT with device {device_id}");

    let hw_manager = app_state.require_hw_manager().await?;
    match hw_manager.sign_psbt(&device_id, &psbt).await {
        Ok(signed_psbt) => {
            info!("Successfully signed PSBT with device {device_id}");
            let signed_psbt_json = serde_json::to_value(&signed_psbt)
                .map_err(|e| format!("Failed to serialize signed PSBT: {e}"))?;
            Ok(CommandResult {
                success: true,
                message: "PSBT signed successfully".to_string(),
                data: Some(signed_psbt_json),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to sign PSBT: {e}");
            Ok(CommandResult::error(
                format!("Failed to sign PSBT: {e}"),
                AppErrorCode::HardwareWalletError,
            ))
        }
    }
}

#[tauri::command]
pub async fn cmd_submit_transaction_signature(
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    session_id: String,
    signed_psbt: String,
    txid: String,
    device_fingerprint: String,
    device_derivation_path: String,
    ledger_hmacs: Option<std::collections::HashMap<String, String>>,
) -> Result<CommandResult, String> {
    info!(
        "Submitting transaction signature for session {session_id}"
    );

    let ws_handler = app_state.ws_handler.lock().await;

    if let Some(handler) = &*ws_handler {
        let mut payload = serde_json::json!({
            "signed_psbt": signed_psbt,
            "txid": txid,
            "device_info": {
                "fingerprint": device_fingerprint,
                "derivation_path": device_derivation_path,
            }
        });
        if let Some(hmacs) = &ledger_hmacs {
            if !hmacs.is_empty() {
                payload["ledger_hmacs"] = serde_json::json!(hmacs);
            }
        }

        let message = serde_json::json!({
            "type": "session",
            "action": "submit",
            "payload": payload.to_string()
        });

        if let Err(e) = handler.send_message(&message).await {
            error!("Failed to send transaction signature: {e:?}");
            return Ok(CommandResult::error(
                "Failed to send transaction signature",
                AppErrorCode::WebsocketConnection,
            ));
        }

        info!("Transaction signature submitted successfully");
        return Ok(CommandResult::success(
            "Transaction signature submitted successfully",
        ));
    }

    Ok(CommandResult::error(
        "No active websocket connection",
        AppErrorCode::WebsocketConnection,
    ))
}

#[tauri::command]
pub async fn cmd_get_ledger_hmacs(
    _window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    let hw_manager = app_state.require_hw_manager().await?;
    let hmacs = hw_manager.get_ledger_hmacs_hex().await;
    let hmacs_json = serde_json::to_value(&hmacs)
        .map_err(|e| format!("Failed to serialize HMACs: {e}"))?;
    Ok(CommandResult {
        success: true,
        message: format!("Found {} Ledger HMAC(s)", hmacs.len()),
        data: Some(hmacs_json),
        error: None,
    })
}
