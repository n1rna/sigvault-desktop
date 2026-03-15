use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

use async_hwi::{
    bitbox::{api::runtime, BitBox02, PairingBitbox02WithLocalCache},
    coldcard,
    jade::{self, Jade},
    ledger::{HidApi, Ledger, LedgerSimulator, TransportHID},
    specter::{Specter, SpecterSimulator},
    trezor::Trezor,
    Error as HWIError, Version, HWI,
};
use bitcoin::{
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    bip32::DerivationPath,
    Network, Psbt,
};
use std::error::Error;
use std::str::FromStr;

/// Helper wrapper to satisfy AsRef<HidApi> bounds for Coldcard
struct AsRefWrap<'a, T> {
    inner: &'a T,
}

impl<'a, T> AsRef<T> for AsRefWrap<'a, T> {
    fn as_ref(&self) -> &T {
        self.inner
    }
}

/// Reasons why a device might be unsupported
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum UnsupportedReason {
    /// Device firmware version is too old
    Version { minimal_supported_version: String },
    /// Device doesn't support a required method
    Method(String),
    /// Device is not part of the wallet (fingerprint doesn't match)
    NotPartOfWallet { fingerprint: String },
    /// Device is configured for a different network
    WrongNetwork,
    /// Bitcoin app is not open (Ledger)
    AppNotOpen,
    /// Unknown error during device initialization
    InitializationError(String),
}

/// State of a discovered hardware wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state")]
pub enum DeviceState {
    /// Device requires user interaction to unlock (BitBox02 pairing, Jade PIN)
    Locked {
        /// Pairing code to display (for BitBox02)
        pairing_code: Option<String>,
    },
    /// Device is ready to use
    Supported {
        fingerprint: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        /// Whether the wallet policy is registered on the device
        #[serde(skip_serializing_if = "Option::is_none")]
        registered: Option<bool>,
    },
    /// Device cannot be used
    Unsupported {
        reason: UnsupportedReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
}

/// A discovered hardware wallet device with its state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub id: String,
    pub device_type: String,
    pub model: String,
    pub state: DeviceState,
}

impl DiscoveredDevice {
    pub fn fingerprint(&self) -> Option<&str> {
        match &self.state {
            DeviceState::Supported { fingerprint, .. } => Some(fingerprint),
            _ => None,
        }
    }

    pub fn is_supported(&self) -> bool {
        matches!(self.state, DeviceState::Supported { .. })
    }

    pub fn is_locked(&self) -> bool {
        matches!(self.state, DeviceState::Locked { .. })
    }
}

/// Holds a locked device that needs user confirmation
pub enum LockedDeviceHandle {
    BitBox02(Box<PairingBitbox02WithLocalCache<runtime::TokioRuntime>>),
    Jade(Jade<jade::SerialTransport>),
}

impl std::fmt::Debug for LockedDeviceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockedDeviceHandle::BitBox02(_) => f.debug_struct("LockedBitBox02").finish(),
            LockedDeviceHandle::Jade(_) => f.debug_struct("LockedJade").finish(),
        }
    }
}

/// Wallet configuration for signing operations
pub struct WalletConfig {
    pub name: String,
    pub descriptor: Option<String>,
    pub hmac: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub xpub: String,
    pub fingerprint: String,
    pub derivation_path: String,
    pub device_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPsbt {
    pub psbt: String, // Base64 encoded signed PSBT
    pub fingerprint: String,
    pub derivation_path: String,
}

#[derive(Serialize, Clone)]
struct DiscoveryProgress {
    stage: String,
    message: String,
    devices_found: usize,
}

#[derive(Serialize, Clone)]
pub struct UnlockProgress {
    pub device_id: String,
    pub message: String,
}

/// Global state for managing locked devices awaiting confirmation
pub struct HardwareWalletManager {
    network: Network,
    /// Locked devices waiting for user confirmation, keyed by device ID
    locked_devices: Arc<Mutex<std::collections::HashMap<String, LockedDeviceHandle>>>,
    /// Supported devices ready for use, keyed by device ID
    supported_devices: Arc<Mutex<std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>>>,
}

impl HardwareWalletManager {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            locked_devices: Arc::new(Mutex::new(std::collections::HashMap::new())),
            supported_devices: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Discover all connected hardware wallets and return their states
    pub async fn discover_devices(
        &self,
        wallet_config: Option<&WalletConfig>,
        app_handle: &tauri::AppHandle,
    ) -> Result<Vec<DiscoveredDevice>, Box<dyn Error + Send + Sync>> {
        info!(
            "Starting hardware wallet discovery on network: {:?}",
            self.network
        );

        let emit_progress = |stage: &str, message: &str, devices_found: usize| {
            let _ = app_handle.emit(
                "hwi_discovery_progress",
                DiscoveryProgress {
                    stage: stage.to_string(),
                    message: message.to_string(),
                    devices_found,
                },
            );
        };

        let mut devices = Vec::new();
        let mut locked_devices = self.locked_devices.lock().await;
        let mut supported_devices = self.supported_devices.lock().await;

        // Clear previous state
        locked_devices.clear();
        supported_devices.clear();

        emit_progress("scanning_specter", "Scanning for Specter devices...", 0);

        // Try Specter Simulator
        match SpecterSimulator::try_connect().await {
            Ok(device) => {
                let id = "specter-simulator".to_string();
                let device = Arc::new(device);
                match self
                    .handle_supported_device(
                        id.clone(),
                        device.clone(),
                        wallet_config,
                        &mut supported_devices,
                    )
                    .await
                {
                    Ok(discovered) => devices.push(discovered),
                    Err(e) => debug!("Specter simulator error: {}", e),
                }
            }
            Err(HWIError::DeviceNotFound) => {}
            Err(e) => debug!("Specter simulator connection error: {}", e),
        }

        emit_progress("scanning_ledger_simulator", "Scanning for Ledger simulator...", devices.len());

        // Try Ledger Simulator
        match LedgerSimulator::try_connect().await {
            Ok(device) => {
                let id = "ledger-simulator".to_string();
                match self
                    .handle_ledger_device(id, device, wallet_config, &mut supported_devices)
                    .await
                {
                    Ok(discovered) => devices.push(discovered),
                    Err(e) => debug!("Ledger simulator error: {}", e),
                }
            }
            Err(HWIError::DeviceNotFound) => {}
            Err(e) => debug!("Ledger simulator connection error: {}", e),
        }

        emit_progress("scanning_specter_usb", "Scanning for Specter devices...", devices.len());

        // Enumerate Specter devices
        match Specter::enumerate().await {
            Ok(specter_devices) => {
                for (idx, device) in specter_devices.into_iter().enumerate() {
                    let id = format!("specter-{}", idx);
                    let device = Arc::new(device);
                    match self
                        .handle_supported_device(
                            id.clone(),
                            device.clone(),
                            wallet_config,
                            &mut supported_devices,
                        )
                        .await
                    {
                        Ok(discovered) => devices.push(discovered),
                        Err(e) => debug!("Specter device {} error: {}", idx, e),
                    }
                }
            }
            Err(e) => warn!("Error enumerating Specter devices: {}", e),
        }

        emit_progress("scanning_jade", "Scanning serial ports for Jade devices...", devices.len());

        // Enumerate Jade devices
        match jade::SerialTransport::enumerate_potential_ports() {
            Ok(ports) => {
                for port in ports {
                    let id = format!("jade-{}", port);
                    match jade::SerialTransport::new(port) {
                        Ok(transport) => {
                            let jade = Jade::new(transport).with_network(self.network);
                            match self
                                .handle_jade_device(
                                    id,
                                    jade,
                                    wallet_config,
                                    &mut locked_devices,
                                    &mut supported_devices,
                                )
                                .await
                            {
                                Ok(discovered) => devices.push(discovered),
                                Err(e) => warn!("Jade device error: {:?}", e),
                            }
                        }
                        Err(e) => warn!("Jade transport error: {:?}", e),
                    }
                }
            }
            Err(e) => warn!("Error enumerating Jade ports: {}", e),
        }

        emit_progress("scanning_usb", "Scanning USB devices...", devices.len());

        // Initialize HID API for USB devices
        let api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                error!("Failed to initialize HID API: {}", e);
                return Ok(devices);
            }
        };

        // Enumerate BitBox02 devices
        for device_info in api.device_list() {
            if async_hwi::bitbox::is_bitbox02(device_info) {
                let id = format!(
                    "bitbox-{:?}-{}-{}",
                    device_info.path(),
                    device_info.vendor_id(),
                    device_info.product_id()
                );

                if let Ok(hid_device) = device_info.open_device(&api) {
                    match PairingBitbox02WithLocalCache::<runtime::TokioRuntime>::connect(
                        hid_device, None,
                    )
                    .await
                    {
                        Ok(pairing_device) => {
                            let pairing_code =
                                pairing_device.pairing_code().map(|s| s.replace('\n', " "));

                            // Store the locked device for later unlocking
                            locked_devices.insert(
                                id.clone(),
                                LockedDeviceHandle::BitBox02(Box::new(pairing_device)),
                            );

                            devices.push(DiscoveredDevice {
                                id,
                                device_type: "BitBox02".to_string(),
                                model: "BitBox02".to_string(),
                                state: DeviceState::Locked { pairing_code },
                            });

                            emit_progress(
                                "scanning_bitbox02",
                                "Found BitBox02 device (needs pairing)",
                                devices.len(),
                            );
                        }
                        Err(e) => warn!("BitBox02 pairing connection error: {:?}", e),
                    }
                }
            }

            // Enumerate Coldcard devices
            if device_info.vendor_id() == coldcard::api::COINKITE_VID
                && device_info.product_id() == coldcard::api::CKCC_PID
            {
                let id = format!(
                    "coldcard-{:?}-{}-{}",
                    device_info.path(),
                    device_info.vendor_id(),
                    device_info.product_id()
                );

                if let Some(sn) = device_info.serial_number() {
                    if let Ok((cc, _)) =
                        coldcard::api::Coldcard::open(AsRefWrap { inner: &api }, sn, None)
                    {
                        let mut hw = coldcard::Coldcard::from(cc);
                        if let Some(config) = wallet_config {
                            hw = hw.with_wallet_name(config.name.clone());
                        }

                        let device: Arc<dyn HWI + Send + Sync> = Arc::new(hw);
                        match self
                            .handle_coldcard_device(
                                id,
                                device,
                                wallet_config,
                                &mut supported_devices,
                            )
                            .await
                        {
                            Ok(discovered) => devices.push(discovered),
                            Err(e) => warn!("Coldcard error: {:?}", e),
                        }
                    }
                }
            }
        }

        emit_progress("scanning_ledger", "Scanning for Ledger devices...", devices.len());

        // Enumerate Ledger devices
        for detected in Ledger::<TransportHID>::enumerate(&api) {
            let id = format!(
                "ledger-{:?}-{}-{}",
                detected.path(),
                detected.vendor_id(),
                detected.product_id()
            );

            match Ledger::<TransportHID>::connect(&api, detected) {
                Ok(device) => {
                    match self
                        .handle_ledger_device(id, device, wallet_config, &mut supported_devices)
                        .await
                    {
                        Ok(discovered) => devices.push(discovered),
                        Err(e) => warn!("Ledger error: {:?}", e),
                    }
                }
                Err(HWIError::DeviceNotFound) => {}
                Err(e) => debug!("Ledger connection error: {}", e),
            }
        }

        emit_progress("scanning_trezor", "Scanning for Trezor devices...", devices.len());

        // Enumerate Trezor devices
        for available in async_hwi::trezor::api::find_devices(false) {
            let id = format!("trezor-{}", available.model);
            match available.connect() {
                Ok(mut client) => {
                    if let Err(e) = client.init_device(None) {
                        warn!("Trezor init error: {:?}", e);
                        continue;
                    }
                    let trezor_network = match self.network {
                        Network::Regtest | Network::Signet => Network::Testnet,
                        other => other,
                    };
                    let trezor = Trezor::new(client, trezor_network);
                    match self
                        .handle_trezor_device(id, trezor, &mut supported_devices, app_handle)
                        .await
                    {
                        Ok(discovered) => devices.push(discovered),
                        Err(e) => warn!("Trezor error: {:?}", e),
                    }
                }
                Err(e) => warn!("Trezor connection error: {:?}", e),
            }
        }

        info!("Discovered {} hardware wallet(s)", devices.len());
        emit_progress("complete", "Discovery complete", devices.len());
        Ok(devices)
    }

    /// Unlock a locked device (BitBox02 pairing confirmation, Jade PIN entry)
    pub async fn unlock_device(
        &self,
        device_id: &str,
        wallet_config: Option<&WalletConfig>,
        app_handle: &tauri::AppHandle,
    ) -> Result<DiscoveredDevice, Box<dyn Error + Send + Sync>> {
        info!("Unlocking device: {}", device_id);

        let emit_unlock = |message: &str| {
            let _ = app_handle.emit(
                "hwi_unlock_progress",
                UnlockProgress {
                    device_id: device_id.to_string(),
                    message: message.to_string(),
                },
            );
        };

        let mut locked_devices = self.locked_devices.lock().await;
        let mut supported_devices = self.supported_devices.lock().await;

        let locked_device = locked_devices
            .remove(device_id)
            .ok_or_else(|| format!("Device {} not found in locked devices", device_id))?;

        match locked_device {
            LockedDeviceHandle::BitBox02(pairing_bb) => {
                info!("Waiting for BitBox02 confirmation...");
                emit_unlock("Confirm pairing code on your BitBox02 device");
                let (paired_device, _) = pairing_bb.wait_confirm().await?;

                let mut bitbox = BitBox02::from(paired_device).with_network(self.network);
                let fingerprint = bitbox.get_master_fingerprint().await?;
                let version = bitbox.get_version().await.ok();
                info!(
                    "BitBox02 paired: fingerprint={}, version={:?}",
                    fingerprint, version
                );

                let mut registered = None;
                if let Some(config) = wallet_config {
                    if let Some(ref desc) = config.descriptor {
                        bitbox = bitbox.with_policy(desc).unwrap();
                        info!(
                            "Checking BitBox02 policy registration with descriptor and network: {:?} and {:?}",
                            desc, self.network
                        );

                        match bitbox.is_policy_registered(desc).await {
                            Ok(is_registered) => {
                                info!("BitBox02 policy registration status: {}", is_registered);
                                registered = Some(is_registered);

                                // Always re-register to ensure fresh state
                                info!("Force re-registering BitBox02 policy...");
                                if !is_registered {
                                    info!("BitBox02 policy not registered, registering...");
                                    let wallet_name = format!(
                                        "sigvault-{}",
                                        hex::encode(&fingerprint.to_bytes()[..4]).to_string()
                                            + "-"
                                            + &rand::random::<u32>().to_string()[..4]
                                    );
                                    match bitbox.register_wallet(&wallet_name, desc).await {
                                        Ok(_) => {
                                            info!("BitBox02 policy registered successfully");
                                            registered = Some(true);
                                        }
                                        Err(e) => {
                                            error!(
                                                "Failed to register policy on BitBox02: {:?}",
                                                e
                                            );
                                            error!("Device fingerprint was: {}. Check that descriptor keys match this device.", fingerprint);
                                            return Err(e.into());
                                        }
                                    }
                                } else {
                                    info!(
                                        "BitBox02 policy already registered, skipping registration"
                                    );
                                }
                            }
                            Err(e) => {
                                error!("Failed to check policy registration on BitBox02: {:?}", e);
                                error!("Device fingerprint was: {}. Check that descriptor keys match this device.", fingerprint);
                                return Err(e.into());
                            }
                        }

                        debug!("BitBox02 policy registration status: {:?}", registered);
                    } else {
                        debug!("No descriptor provided for BitBox02, skipping policy check");
                    }
                }

                let device: Arc<dyn HWI + Send + Sync> = Arc::new(bitbox);
                supported_devices.insert(device_id.to_string(), device);

                Ok(DiscoveredDevice {
                    id: device_id.to_string(),
                    device_type: "BitBox02".to_string(),
                    model: "BitBox02".to_string(),
                    state: DeviceState::Supported {
                        fingerprint: fingerprint.to_string(),
                        version: version.map(|v| v.to_string()),
                        registered,
                    },
                })
            }
            LockedDeviceHandle::Jade(jade) => {
                info!("Authenticating Jade device...");
                emit_unlock("Enter your PIN on the Jade device");
                jade.auth()
                    .await
                    .map_err(|e| format!("Jade auth error: {:?}", e))?;

                let fingerprint = jade.get_master_fingerprint().await?;
                let version = jade.get_version().await.ok();

                let mut registered = None;
                if let Some(config) = wallet_config {
                    if let Some(ref desc) = config.descriptor {
                        let jade_with_wallet = jade.with_wallet(config.name.clone());
                        registered = Some(
                            jade_with_wallet
                                .is_wallet_registered(&config.name, desc)
                                .await?,
                        );

                        let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade_with_wallet);
                        supported_devices.insert(device_id.to_string(), device);
                    } else {
                        let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade);
                        supported_devices.insert(device_id.to_string(), device);
                    }
                } else {
                    let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade);
                    supported_devices.insert(device_id.to_string(), device);
                }

                Ok(DiscoveredDevice {
                    id: device_id.to_string(),
                    device_type: "Jade".to_string(),
                    model: "Jade".to_string(),
                    state: DeviceState::Supported {
                        fingerprint: fingerprint.to_string(),
                        version: version.map(|v| v.to_string()),
                        registered,
                    },
                })
            }
        }
    }

    /// Get a supported device by ID for signing operations
    pub async fn get_device(&self, device_id: &str) -> Option<Arc<dyn HWI + Send + Sync>> {
        let devices = self.supported_devices.lock().await;
        devices.get(device_id).cloned()
    }

    /// Check if a device is locked and waiting for confirmation
    pub async fn is_device_locked(&self, device_id: &str) -> bool {
        let devices = self.locked_devices.lock().await;
        devices.contains_key(device_id)
    }

    /// Sign a PSBT using a managed device looked up by device ID
    pub async fn sign_psbt(
        &self,
        device_id: &str,
        psbt_base64: &str,
    ) -> Result<SignedPsbt, Box<dyn Error + Send + Sync>> {
        info!("Signing PSBT with device {}", device_id);

        let device = {
            let devices = self.supported_devices.lock().await;
            devices
                .get(device_id)
                .cloned()
                .ok_or_else(|| format!("Device {} not found. Run discovery first.", device_id))?
        };

        let psbt_bytes = BASE64.decode(psbt_base64)?;
        let mut psbt = Psbt::deserialize(&psbt_bytes)?;

        let fingerprint = device.get_master_fingerprint().await?;

        device.sign_tx(&mut psbt).await?;

        let signed_psbt_bytes = psbt.serialize();
        let signed_psbt_base64 = BASE64.encode(&signed_psbt_bytes);

        info!("Successfully signed PSBT with device {}", device_id);

        Ok(SignedPsbt {
            psbt: signed_psbt_base64,
            fingerprint: fingerprint.to_string(),
            derivation_path: String::new(),
        })
    }

    /// Get device info (xpub) by matching fingerprint across all supported devices
    pub async fn get_device_info(
        &self,
        fingerprint: &str,
        derivation_path: &str,
    ) -> Result<DeviceInfo, Box<dyn Error + Send + Sync>> {
        info!(
            "Extracting device info for fingerprint: {}, path: {}",
            fingerprint, derivation_path
        );

        let path = DerivationPath::from_str(derivation_path)?;

        // Clone device Arcs out of the lock quickly
        let device_list: Vec<Arc<dyn HWI + Send + Sync>> = {
            let devices = self.supported_devices.lock().await;
            devices.values().cloned().collect()
        };

        for device in device_list {
            match device.get_master_fingerprint().await {
                Ok(fp) if fp.to_string() == fingerprint => {
                    debug!("Found matching device with fingerprint: {}", fingerprint);

                    let xpub = device.get_extended_pubkey(&path).await?;
                    let device_type = format!("{:?}", device.device_kind());

                    info!("Successfully extracted device info");
                    return Ok(DeviceInfo {
                        xpub: xpub.to_string(),
                        fingerprint: fp.to_string(),
                        derivation_path: derivation_path.to_string(),
                        device_type,
                    });
                }
                Ok(_) => continue,
                Err(e) => {
                    error!("Failed to get fingerprint: {}", e);
                    continue;
                }
            }
        }

        Err(format!("Device with fingerprint {} not found", fingerprint).into())
    }

    // Helper methods for handling specific device types

    async fn handle_supported_device(
        &self,
        id: String,
        device: Arc<dyn HWI + Send + Sync>,
        _wallet_config: Option<&WalletConfig>,
        supported_devices: &mut std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>,
    ) -> Result<DiscoveredDevice, HWIError> {
        let kind = device.device_kind();
        let fingerprint = device.get_master_fingerprint().await?;
        let version = device.get_version().await.ok();

        supported_devices.insert(id.clone(), device);

        Ok(DiscoveredDevice {
            id,
            device_type: format!("{:?}", kind),
            model: format!("{:?}", kind),
            state: DeviceState::Supported {
                fingerprint: fingerprint.to_string(),
                version: version.map(|v| v.to_string()),
                registered: None,
            },
        })
    }

    async fn handle_jade_device(
        &self,
        id: String,
        jade: Jade<jade::SerialTransport>,
        wallet_config: Option<&WalletConfig>,
        locked_devices: &mut std::collections::HashMap<String, LockedDeviceHandle>,
        supported_devices: &mut std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>,
    ) -> Result<DiscoveredDevice, HWIError> {
        let info = jade.get_info().await?;
        let version = async_hwi::parse_version(&info.jade_version).ok();

        // Use the device's own network for auth so it works regardless of app network
        let device_network = match info.jade_networks {
            jade::api::JadeNetworks::Main => Network::Bitcoin,
            jade::api::JadeNetworks::Test | jade::api::JadeNetworks::All => self.network,
        };
        let jade = jade.with_network(device_network);

        // Check if device needs unlocking
        match info.jade_state {
            jade::api::JadeState::Locked
            | jade::api::JadeState::Temp
            | jade::api::JadeState::Uninit
            | jade::api::JadeState::Unsaved => {
                locked_devices.insert(id.clone(), LockedDeviceHandle::Jade(jade));
                Ok(DiscoveredDevice {
                    id,
                    device_type: "Jade".to_string(),
                    model: "Jade".to_string(),
                    state: DeviceState::Locked { pairing_code: None },
                })
            }
            jade::api::JadeState::Ready => {
                let fingerprint = jade.get_master_fingerprint().await?;

                let mut registered = None;
                if let Some(config) = wallet_config {
                    if let Some(ref desc) = config.descriptor {
                        let jade_with_wallet = jade.with_wallet(config.name.clone());
                        registered = Some(
                            jade_with_wallet
                                .is_wallet_registered(&config.name, desc)
                                .await?,
                        );
                        let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade_with_wallet);
                        supported_devices.insert(id.clone(), device);
                    } else {
                        let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade);
                        supported_devices.insert(id.clone(), device);
                    }
                } else {
                    let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade);
                    supported_devices.insert(id.clone(), device);
                }

                Ok(DiscoveredDevice {
                    id,
                    device_type: "Jade".to_string(),
                    model: "Jade".to_string(),
                    state: DeviceState::Supported {
                        fingerprint: fingerprint.to_string(),
                        version: version.map(|v| v.to_string()),
                        registered,
                    },
                })
            }
        }
    }

    async fn handle_ledger_device<T: async_hwi::ledger::Transport + Sync + Send + 'static>(
        &self,
        id: String,
        mut device: Ledger<T>,
        wallet_config: Option<&WalletConfig>,
        supported_devices: &mut std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>,
    ) -> Result<DiscoveredDevice, HWIError> {
        match (
            device.get_master_fingerprint().await,
            device.get_version().await,
        ) {
            (Ok(fingerprint), Ok(version)) => {
                // Check minimum version (2.1.0)
                if version.major < 2 || (version.major == 2 && version.minor < 1) {
                    return Ok(DiscoveredDevice {
                        id,
                        device_type: "Ledger".to_string(),
                        model: "Ledger".to_string(),
                        state: DeviceState::Unsupported {
                            reason: UnsupportedReason::Version {
                                minimal_supported_version: "2.1.0".to_string(),
                            },
                            version: Some(version.to_string()),
                        },
                    });
                }

                let mut registered = None;
                if let Some(config) = wallet_config {
                    if let Some(ref desc) = config.descriptor {
                        device = device.with_wallet(&config.name, desc, config.hmac)?;
                        registered = Some(true);
                    }
                }

                let device: Arc<dyn HWI + Send + Sync> = Arc::new(device);
                supported_devices.insert(id.clone(), device);

                Ok(DiscoveredDevice {
                    id,
                    device_type: "Ledger".to_string(),
                    model: "Ledger".to_string(),
                    state: DeviceState::Supported {
                        fingerprint: fingerprint.to_string(),
                        version: Some(version.to_string()),
                        registered,
                    },
                })
            }
            (_, _) => Ok(DiscoveredDevice {
                id,
                device_type: "Ledger".to_string(),
                model: "Ledger".to_string(),
                state: DeviceState::Unsupported {
                    reason: UnsupportedReason::AppNotOpen,
                    version: None,
                },
            }),
        }
    }

    async fn handle_trezor_device(
        &self,
        id: String,
        trezor: Trezor,
        supported_devices: &mut std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>,
        app_handle: &tauri::AppHandle,
    ) -> Result<DiscoveredDevice, HWIError> {
        let _ = app_handle.emit(
            "hwi_discovery_progress",
            DiscoveryProgress {
                stage: "trezor_unlock".to_string(),
                message: "Trezor found — enter PIN on your device if prompted".to_string(),
                devices_found: 0,
            },
        );
        match (
            trezor.get_master_fingerprint().await,
            trezor.get_version().await,
        ) {
            (Ok(fingerprint), Ok(version)) => {
                let device: Arc<dyn HWI + Send + Sync> = Arc::new(trezor);
                supported_devices.insert(id.clone(), device);

                Ok(DiscoveredDevice {
                    id,
                    device_type: "Trezor".to_string(),
                    model: "Trezor".to_string(),
                    state: DeviceState::Supported {
                        fingerprint: fingerprint.to_string(),
                        version: Some(version.to_string()),
                        registered: None,
                    },
                })
            }
            (Err(e), _) => {
                let msg = format!("{}", e);
                if msg.contains("PIN required") || msg.contains("locked") {
                    Ok(DiscoveredDevice {
                        id,
                        device_type: "Trezor".to_string(),
                        model: "Trezor".to_string(),
                        state: DeviceState::Unsupported {
                            reason: UnsupportedReason::InitializationError(
                                "Device is locked (enter PIN on device)".to_string(),
                            ),
                            version: None,
                        },
                    })
                } else {
                    Ok(DiscoveredDevice {
                        id,
                        device_type: "Trezor".to_string(),
                        model: "Trezor".to_string(),
                        state: DeviceState::Unsupported {
                            reason: UnsupportedReason::InitializationError(msg),
                            version: None,
                        },
                    })
                }
            }
            (_, Err(e)) => Ok(DiscoveredDevice {
                id,
                device_type: "Trezor".to_string(),
                model: "Trezor".to_string(),
                state: DeviceState::Unsupported {
                    reason: UnsupportedReason::InitializationError(format!("{}", e)),
                    version: None,
                },
            }),
        }
    }

    async fn handle_coldcard_device(
        &self,
        id: String,
        device: Arc<dyn HWI + Send + Sync>,
        _wallet_config: Option<&WalletConfig>,
        supported_devices: &mut std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>,
    ) -> Result<DiscoveredDevice, HWIError> {
        match (
            device.get_master_fingerprint().await,
            device.get_version().await,
        ) {
            (Ok(fingerprint), Ok(version)) => {
                // Check minimum version (6.2.1)
                let min_version = Version {
                    major: 6,
                    minor: 2,
                    patch: 1,
                    prerelease: None,
                };

                if version >= min_version {
                    supported_devices.insert(id.clone(), device);

                    Ok(DiscoveredDevice {
                        id,
                        device_type: "Coldcard".to_string(),
                        model: "Coldcard".to_string(),
                        state: DeviceState::Supported {
                            fingerprint: fingerprint.to_string(),
                            version: Some(version.to_string()),
                            registered: None,
                        },
                    })
                } else {
                    Ok(DiscoveredDevice {
                        id,
                        device_type: "Coldcard".to_string(),
                        model: "Coldcard".to_string(),
                        state: DeviceState::Unsupported {
                            reason: UnsupportedReason::Version {
                                minimal_supported_version: "6.2.1 (Edge firmware)".to_string(),
                            },
                            version: Some(version.to_string()),
                        },
                    })
                }
            }
            (_, _) => {
                error!("Failed to connect to Coldcard");
                Ok(DiscoveredDevice {
                    id,
                    device_type: "Coldcard".to_string(),
                    model: "Coldcard".to_string(),
                    state: DeviceState::Unsupported {
                        reason: UnsupportedReason::InitializationError(
                            "Failed to get device info".to_string(),
                        ),
                        version: None,
                    },
                })
            }
        }
    }
}
