use bitcoin::Network;
use log::{debug, error, info};
use serde_json;
use tauri::{State, WebviewWindow};

use crate::error::AppErrorCode;
use crate::hwi::{DeviceInfo, HardwareWalletDevice, HwiService};
use crate::state::ApplicationState;

use super::types::CommandResult;

#[tauri::command]
pub async fn cmd_discover_hardware_wallets(
    _window: WebviewWindow,
    _app_state: State<'_, ApplicationState>,
    network: Option<String>,
) -> Result<CommandResult, String> {
    info!("Starting hardware wallet discovery");

    let btc_network = match network.as_deref() {
        Some("mainnet") => Network::Bitcoin,
        Some("testnet") => Network::Testnet,
        Some("regtest") => Network::Regtest,
        Some("signet") => Network::Signet,
        _ => {
            debug!("No network specified, defaulting to testnet");
            Network::Testnet
        }
    };

    match HwiService::discover_devices(btc_network).await {
        Ok(devices) => {
            info!("Successfully discovered {} device(s)", devices.len());
            let devices_json = serde_json::to_value(&devices)
                .map_err(|e| format!("Failed to serialize devices: {}", e))?;

            Ok(CommandResult {
                success: true,
                message: format!("Found {} hardware wallet(s)", devices.len()),
                data: Some(devices_json),
                error: None,
            })
        }
        Err(e) => {
            error!("Hardware wallet discovery failed: {}", e);
            Ok(CommandResult::error(
                &format!("Failed to discover devices: {}", e),
                AppErrorCode::HardwareWalletError,
            ))
        }
    }
}

#[tauri::command]
pub async fn cmd_get_device_xpub(
    _window: WebviewWindow,
    _app_state: State<'_, ApplicationState>,
    fingerprint: String,
    derivation_path: String,
    network: Option<String>,
) -> Result<CommandResult, String> {
    info!(
        "Extracting xpub for device {} at path {}",
        fingerprint, derivation_path
    );

    let btc_network = match network.as_deref() {
        Some("mainnet") => Network::Bitcoin,
        Some("testnet") => Network::Testnet,
        Some("regtest") => Network::Regtest,
        Some("signet") => Network::Signet,
        _ => {
            debug!("No network specified, defaulting to testnet");
            Network::Testnet
        }
    };

    match HwiService::get_device_info(btc_network, fingerprint.clone(), derivation_path).await {
        Ok(device_info) => {
            info!("Successfully extracted device info for {}", fingerprint);
            let device_info_json = serde_json::to_value(&device_info)
                .map_err(|e| format!("Failed to serialize device info: {}", e))?;

            Ok(CommandResult {
                success: true,
                message: "Device info extracted successfully".to_string(),
                data: Some(device_info_json),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to extract device info: {}", e);
            Ok(CommandResult::error(
                &format!("Failed to extract device info: {}", e),
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
    info!("Submitting device registration for session {}", session_id);

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
            error!("Failed to send device registration: {:?}", e);
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
