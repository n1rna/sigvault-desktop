use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::Manager;
use crate::window::emit_notification;

fn descriptor_to_multipath(desc: &str) -> String {
    desc.replace("/**", "/<0;1>/*")
}

fn descriptor_for_bitbox(desc: &str) -> String {
    descriptor_to_multipath(desc).replace("sortedmulti", "multi")
}
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
    pub ledger_hmacs: Option<std::collections::HashMap<String, [u8; 32]>>,
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
    locked_devices: Arc<Mutex<std::collections::HashMap<String, LockedDeviceHandle>>>,
    supported_devices: Arc<Mutex<std::collections::HashMap<String, Arc<dyn HWI + Send + Sync>>>>,
    ledger_hmacs: Arc<Mutex<std::collections::HashMap<String, [u8; 32]>>>,
}

impl HardwareWalletManager {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            locked_devices: Arc::new(Mutex::new(std::collections::HashMap::new())),
            supported_devices: Arc::new(Mutex::new(std::collections::HashMap::new())),
            ledger_hmacs: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub async fn get_ledger_hmacs_hex(&self) -> std::collections::HashMap<String, String> {
        self.ledger_hmacs
            .lock()
            .await
            .iter()
            .map(|(fp, bytes)| (fp.clone(), hex::encode(bytes)))
            .collect()
    }

    /// Discover all connected hardware wallets and return their states
    pub async fn discover_devices(
        &self,
        wallet_config: Option<&WalletConfig>,
        app_handle: Option<&tauri::AppHandle>,
    ) -> Result<Vec<DiscoveredDevice>, Box<dyn Error + Send + Sync>> {
        info!(
            "Starting hardware wallet discovery on network: {:?}",
            self.network
        );

        let window = app_handle.and_then(|h| h.get_webview_window("main"));
        let emit_progress = |_stage: &str, message: &str, _devices_found: usize| {
            if let Some(w) = &window {
                emit_notification(w, "Device Discovery", message, "info");
            }
        };

        let mut devices = Vec::new();
        let mut locked_devices = self.locked_devices.lock().await;
        let mut supported_devices = self.supported_devices.lock().await;

        // Clear previous state
        locked_devices.clear();
        supported_devices.clear();

        emit_progress("scanning_specter", "Scanning for Specter devices...", 0);

        // Try Specter Simulator (with timeout — uninitialized simulators
        // hang on fingerprint request indefinitely).
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            SpecterSimulator::try_connect(),
        ).await {
            Ok(result) => match result {
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
            Err(_) => debug!("Specter simulator connection timed out"),
        }

        emit_progress("scanning_ledger_simulator", "Scanning for Ledger simulator...", devices.len());

        // Try Ledger Simulator
        match LedgerSimulator::try_connect().await {
            Ok(device) => {
                let id = "ledger-simulator".to_string();
                match self
                    .handle_ledger_device(id, device, wallet_config, app_handle, &mut supported_devices)
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

        emit_progress("scanning_jade", "Scanning for Jade devices...", devices.len());

        // Try Jade QEMU emulator (TCP) with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            async_hwi::jade::JadeEmulator::try_connect(),
        ).await {
            Ok(Ok(jade)) => {
                let jade = jade.with_network(self.network);
                let id = "jade-emulator".to_string();
                // The emulator doesn't need PIN — treat as a supported device directly.
                let device: Arc<dyn HWI + Send + Sync> = Arc::new(jade);
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
                    Err(e) => debug!("Jade emulator error: {:?}", e),
                }
            }
            Ok(Err(_)) => debug!("Jade emulator not available"),
            Err(_) => debug!("Jade emulator connection timed out"),
        }

        // Enumerate physical Jade devices (serial ports)
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
        let device_count = api.device_list().count();
        debug!("HID API found {} devices total", device_count);
        for device_info in api.device_list() {
            debug!(
                "HID device: VID={:#06x} PID={:#06x} product={:?} usage_page={:#06x} iface={}",
                device_info.vendor_id(),
                device_info.product_id(),
                device_info.product_string(),
                device_info.usage_page(),
                device_info.interface_number(),
            );
            if async_hwi::bitbox::is_bitbox02(device_info) {
                let id = format!(
                    "bitbox-{:?}-{}-{}",
                    device_info.path(),
                    device_info.vendor_id(),
                    device_info.product_id()
                );
                debug!("BitBox02 detected at {:?}, attempting to open", device_info.path());

                match device_info.open_device(&api) {
                    Err(e) => {
                        warn!("Failed to open BitBox02 HID device at {:?}: {}", device_info.path(), e);
                    }
                    Ok(hid_device) => {
                    emit_progress(
                        "bitbox02_connecting",
                        "BitBox02 detected — enter your device password if prompted",
                        devices.len(),
                    );
                    // bitbox-api uses blocking hidapi I/O inside async fns.
                    // Run on a dedicated runtime to avoid deadlocking the
                    // main tokio runtime (which also runs the UHID bridge
                    // relay tasks for emulated devices).
                    let connect_result = tokio::task::spawn_blocking(move || {
                        let rt = tokio::runtime::Builder::new_multi_thread()
                            .worker_threads(2)
                            .enable_all()
                            .build()
                            .map_err(|e| HWIError::Device(format!("runtime: {e}")))?;
                        rt.block_on(PairingBitbox02WithLocalCache::<runtime::TokioRuntime>::connect(
                            hid_device, None,
                        ))
                    })
                    .await
                    .map_err(|e| HWIError::Device(format!("task: {e}")))?;
                    match connect_result {
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

                if device_info.serial_number().is_some() {
                    // The coldcard crate does blocking HID I/O. Run on a
                    // blocking thread with a 5s timeout — stale UHID devices
                    // (kept alive by browsers) hang on HID read forever.
                    let sn = device_info.serial_number().unwrap_or("").to_string();
                    let cc_result = tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        tokio::task::spawn_blocking(move || {
                            let cc_api = coldcard::api::Api::new()?;
                            cc_api.open(&sn, None)
                        }),
                    ).await;
                    let cc_result = match cc_result {
                        Ok(Ok(Ok(r))) => Some(r),
                        Ok(Ok(Err(e))) => {
                            debug!("Coldcard open failed (will try next): {:?}", e);
                            None
                        }
                        Ok(Err(e)) => {
                            debug!("Coldcard open task error: {:?}", e);
                            None
                        }
                        Err(_) => {
                            debug!("Coldcard open timed out (stale device?)");
                            None
                        }
                    };
                    if let Some((cc, _)) = cc_result {
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
                        .handle_ledger_device(id, device, wallet_config, app_handle, &mut supported_devices)
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
        if let Some(w) = app_handle.and_then(|h| h.get_webview_window("main")) {
            emit_notification(&w, "Device Discovery", "Discovery complete", "success");
        }
        Ok(devices)
    }

    /// Unlock a locked device (BitBox02 pairing confirmation, Jade PIN entry)
    pub async fn unlock_device(
        &self,
        device_id: &str,
        wallet_config: Option<&WalletConfig>,
        app_handle: Option<&tauri::AppHandle>,
    ) -> Result<DiscoveredDevice, Box<dyn Error + Send + Sync>> {
        info!("Unlocking device: {}", device_id);

        let window = app_handle.and_then(|h| h.get_webview_window("main"));
        let emit_unlock = |message: &str| {
            if let Some(w) = &window {
                emit_notification(w, "Device Unlock", message, "info");
            }
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
                if let Some(w) = app_handle.and_then(|h| h.get_webview_window("main")) {
                    emit_notification(&w, "Device Unlock", "BitBox02 pairing confirmed", "success");
                }

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
                        let bb_desc = descriptor_for_bitbox(desc);
                        {
                        bitbox = bitbox.with_policy(&bb_desc).unwrap();
                        info!(
                            "Checking BitBox02 policy registration with descriptor and network: {:?} and {:?}",
                            bb_desc, self.network
                        );

                        match bitbox.is_policy_registered(&bb_desc).await {
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
                                    match bitbox.register_wallet(&wallet_name, &bb_desc).await {
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
                        } // close else (non-multisig policy path)
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

                    let mut xpub = device.get_extended_pubkey(&path).await?;
                    xpub.network = self.network.into();
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
        app_handle: Option<&tauri::AppHandle>,
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
                        let fp_str = fingerprint.to_string();
                        let cached_hmac = self.ledger_hmacs.lock().await.get(&fp_str).copied();
                        let backend_hmac = config.ledger_hmacs.as_ref().and_then(|m| m.get(&fp_str).copied());
                        let hmac = if config.hmac.is_some() {
                            config.hmac
                        } else if let Some(cached) = cached_hmac {
                            info!("Using cached Ledger HMAC for fingerprint {}", fp_str);
                            Some(cached)
                        } else if let Some(from_backend) = backend_hmac {
                            info!("Using backend-persisted Ledger HMAC for fingerprint {}", fp_str);
                            self.ledger_hmacs.lock().await.insert(fp_str.clone(), from_backend);
                            Some(from_backend)
                        } else {
                            info!("Registering wallet policy on Ledger (confirm on device)...");
                            if let Some(w) = app_handle.and_then(|h| h.get_webview_window("main")) {
                                emit_notification(&w, "Device Discovery", "Ledger found — approve wallet policy on your device", "info");
                            }
                            match device.register_wallet(&config.name, desc).await {
                                Ok(h) => {
                                    info!("Ledger wallet registered, hmac: {:?}", h.map(|h| hex::encode(h)));
                                    if let Some(ref hmac_bytes) = h {
                                        self.ledger_hmacs.lock().await.insert(fp_str.clone(), *hmac_bytes);
                                    }
                                    if let Some(w) = app_handle.and_then(|h| h.get_webview_window("main")) {
                                        emit_notification(&w, "Device Discovery", "Ledger wallet policy registered", "success");
                                    }
                                    h
                                }
                                Err(e) => {
                                    error!("Ledger wallet registration failed: {:?}", e);
                                    None
                                }
                            }
                        };
                        device = match device.with_wallet(&config.name, desc, hmac) {
                            Ok(d) => {
                                registered = Some(hmac.is_some());
                                d
                            }
                            Err(e) => {
                                error!("Ledger with_wallet failed: {:?}", e);
                                return Ok(DiscoveredDevice {
                                    id,
                                    device_type: "Ledger".to_string(),
                                    model: "Ledger".to_string(),
                                    state: DeviceState::Unsupported {
                                        reason: UnsupportedReason::InitializationError(
                                            format!("Policy registration failed: {:?}", e),
                                        ),
                                        version: Some(version.to_string()),
                                    },
                                });
                            }
                        };
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
        app_handle: Option<&tauri::AppHandle>,
    ) -> Result<DiscoveredDevice, HWIError> {
        if let Some(w) = app_handle.and_then(|h| h.get_webview_window("main")) {
            emit_notification(&w, "Device Discovery", "Trezor found — enter PIN on your device if prompted", "info");
        }
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

/// Finalize all taproot inputs in a PSBT by constructing the final witness.
///
/// Handles both keypath spends (tap_key_sig present) and script-path spends
/// (tap_script_sigs present, including multi_a scripts).
///
/// After calling this function the PSBT is ready for `extract_tx`.
pub fn finalize_psbt(psbt: &mut Psbt) {
    for i in 0..psbt.inputs.len() {
        if psbt.inputs[i].final_script_witness.is_some() {
            continue;
        }

        if let Some(sig) = psbt.inputs[i].tap_key_sig {
            let mut witness = bitcoin::Witness::new();
            witness.push(sig.to_vec());
            psbt.inputs[i].final_script_witness = Some(witness);
            psbt.inputs[i].tap_key_sig = None;
            psbt.inputs[i].tap_internal_key = None;
            psbt.inputs[i].tap_merkle_root = None;
            psbt.inputs[i].tap_key_origins.clear();
            psbt.inputs[i].tap_scripts.clear();
            continue;
        }

        if psbt.inputs[i].tap_script_sigs.is_empty() {
            continue;
        }

        let tap_scripts: Vec<_> = psbt.inputs[i].tap_scripts.clone().into_iter().collect();
        let tap_script_sigs = psbt.inputs[i].tap_script_sigs.clone();
        let tap_key_origins = psbt.inputs[i].tap_key_origins.clone();

        for (control_block, (script, _leaf_version)) in &tap_scripts {
            let leaf_hash = bitcoin::taproot::TapLeafHash::from_script(
                script,
                bitcoin::taproot::LeafVersion::TapScript,
            );

            let sigs_for_leaf: Vec<_> = tap_script_sigs
                .iter()
                .filter(|((_, lh), _)| *lh == leaf_hash)
                .collect();

            if sigs_for_leaf.is_empty() {
                continue;
            }

            let mut witness = bitcoin::Witness::new();

            let is_checksigadd = script.as_bytes().contains(&0xba);
            if is_checksigadd {
                let keys_in_script: Vec<bitcoin::XOnlyPublicKey> = tap_key_origins
                    .iter()
                    .filter(|(_, (leaf_hashes, _))| leaf_hashes.contains(&leaf_hash))
                    .map(|(pk, _)| *pk)
                    .collect();

                for key in keys_in_script.iter().rev() {
                    if let Some((_, sig)) = sigs_for_leaf.iter().find(|((pk, _), _)| pk == key) {
                        witness.push(sig.to_vec());
                    } else {
                        witness.push(&[] as &[u8]);
                    }
                }
            } else {
                for ((_, _), sig) in &sigs_for_leaf {
                    witness.push(sig.to_vec());
                }
            }

            witness.push(script.as_bytes());
            witness.push(control_block.serialize());

            psbt.inputs[i].final_script_witness = Some(witness);
            psbt.inputs[i].tap_script_sigs.clear();
            psbt.inputs[i].tap_internal_key = None;
            psbt.inputs[i].tap_merkle_root = None;
            psbt.inputs[i].tap_key_origins.clear();
            psbt.inputs[i].tap_scripts.clear();
            break;
        }
    }
}
