use bitcoin::{bip32::DerivationPath, Network};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::list;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareWalletDevice {
    pub id: String,
    pub device_type: String,
    pub model: String,
    pub fingerprint: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub xpub: String,
    pub fingerprint: String,
    pub derivation_path: String,
    pub device_type: String,
}

pub struct HwiService;

impl HwiService {
    pub async fn discover_devices(network: Network) -> Result<Vec<HardwareWalletDevice>, String> {
        info!("Starting hardware wallet discovery on network: {:?}", network);

        let mut devices = list(network, None)
            .await
            .map_err(|e| format!("Failed to enumerate devices: {}", e))?;

        let mut discovered = Vec::new();

        for (idx, device) in devices.iter_mut().enumerate() {
            let fingerprint_result = device.get_master_fingerprint().await;
            let device_type = device.device_kind();

            match fingerprint_result {
                Ok(fingerprint) => {
                    let model = format!("{:?}", device_type);

                    let hw_device = HardwareWalletDevice {
                        id: format!("{}_{}", fingerprint, idx),
                        device_type: model.clone(),
                        model,
                        fingerprint: fingerprint.to_string(),
                        connected: true,
                    };

                    debug!("Discovered device: {:?}", hw_device);
                    discovered.push(hw_device);
                }
                Err(e) => {
                    error!("Failed to get fingerprint for device: {}", e);
                    continue;
                }
            }
        }

        info!("Discovered {} hardware wallet(s)", discovered.len());
        Ok(discovered)
    }

    pub async fn get_device_info(
        network: Network,
        fingerprint: String,
        derivation_path: String,
    ) -> Result<DeviceInfo, String> {
        info!(
            "Extracting device info for fingerprint: {}, path: {}",
            fingerprint, derivation_path
        );

        let devices = list(network, None)
            .await
            .map_err(|e| format!("Failed to enumerate devices: {}", e))?;

        let path = DerivationPath::from_str(&derivation_path)
            .map_err(|e| format!("Invalid derivation path: {}", e))?;

        for device in devices {
            match device.get_master_fingerprint().await {
                Ok(fp) if fp.to_string() == fingerprint => {
                    debug!("Found matching device with fingerprint: {}", fingerprint);

                    let xpub = device
                        .get_extended_pubkey(&path)
                        .await
                        .map_err(|e| format!("Failed to get xpub: {}", e))?;

                    let device_type = format!("{:?}", device.device_kind());

                    let device_info = DeviceInfo {
                        xpub: xpub.to_string(),
                        fingerprint: fp.to_string(),
                        derivation_path: derivation_path.clone(),
                        device_type,
                    };

                    info!("Successfully extracted device info");
                    return Ok(device_info);
                }
                Ok(_) => continue,
                Err(e) => {
                    error!("Failed to get fingerprint: {}", e);
                    continue;
                }
            }
        }

        Err(format!(
            "Device with fingerprint {} not found",
            fingerprint
        ))
    }
}
