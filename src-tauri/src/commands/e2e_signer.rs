//! Debug-only software signer for the e2e desktop signing-ceremony test.
//!
//! Mirrors the `SIGVAULT_E2E_AUTH_URL_FILE` oauth hook in `oauth.rs`: when
//! `SIGVAULT_E2E_SIGNER_MNEMONIC` is set in a debug build, the hardware-wallet
//! commands in `hwi.rs` short-circuit to this module instead of touching USB.
//!
//! It presents a single synthetic "device" whose fingerprint is the master
//! fingerprint derived from the mnemonic (so it matches the
//! `signature_slots` fingerprint of a device the test registered with the
//! same mnemonic), and signs PSBTs in-process via rust-bitcoin's
//! `Psbt::sign`. The ceremony PSBT arrives over the WebSocket carrying its
//! own `bip32_derivation`, so signing needs only the master xprv — no BDK
//! wallet or on-disk descriptor.
//!
//! Compiled only under `debug_assertions`; the interception sites in
//! `hwi.rs` are `#[cfg(debug_assertions)]` too, so this is wholly absent from
//! release builds.
#![cfg(debug_assertions)]

use std::str::FromStr;

use bdk_wallet::bitcoin::bip32::{Xpriv, Xpub};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::{Network, Psbt};
use serde_json::Value;

use crate::error::AppErrorCode;
use crate::hwi::{DeviceInfo, DeviceState, DiscoveredDevice, SignedPsbt};
use crate::local_wallet::manager::{derive_account_at_path, derive_master_xpriv};

use super::types::CommandResult;

const MNEMONIC_ENV: &str = "SIGVAULT_E2E_SIGNER_MNEMONIC";
const PATH_ENV: &str = "SIGVAULT_E2E_SIGNER_PATH";
const PASSPHRASE_ENV: &str = "SIGVAULT_E2E_SIGNER_PASSPHRASE";
const DEFAULT_PATH: &str = "m/48'/0'/0'/2'";
const DEVICE_ID: &str = "e2e-software-signer";
const DEVICE_TYPE: &str = "e2e-software-signer";

// The network only affects xprv/xpub serialization — never the signatures or
// the master fingerprint — so we fix it here rather than coupling to app
// state (the hardware subsystem may never be initialized in e2e mode). The
// fingerprint produced is identical across networks.
const NETWORK: Network = Network::Bitcoin;

/// True when the debug-only e2e signer should intercept hardware commands.
pub fn active() -> bool {
    std::env::var(MNEMONIC_ENV)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

fn mnemonic() -> Result<String, String> {
    std::env::var(MNEMONIC_ENV).map_err(|_| format!("{MNEMONIC_ENV} not set"))
}

fn signer_path() -> String {
    std::env::var(PATH_ENV).unwrap_or_else(|_| DEFAULT_PATH.to_string())
}

fn passphrase() -> String {
    std::env::var(PASSPHRASE_ENV).unwrap_or_default()
}

fn master() -> Result<Xpriv, String> {
    derive_master_xpriv(NETWORK, &mnemonic()?, &passphrase()).map_err(|e| e.to_string())
}

fn fingerprint() -> Result<String, String> {
    let secp = Secp256k1::new();
    Ok(master()?.fingerprint(&secp).to_string())
}

fn synthetic_device() -> Result<DiscoveredDevice, String> {
    Ok(DiscoveredDevice {
        id: DEVICE_ID.to_string(),
        device_type: DEVICE_TYPE.to_string(),
        model: "E2E Software Signer".to_string(),
        state: DeviceState::Supported {
            fingerprint: fingerprint()?,
            version: None,
            // Report the wallet policy as already registered so the UI
            // doesn't attempt an on-device policy registration.
            registered: Some(true),
        },
    })
}

fn ok(message: &str, data: Value) -> CommandResult {
    CommandResult {
        success: true,
        message: message.to_string(),
        error: None,
        data: Some(data),
    }
}

fn fail(message: String) -> CommandResult {
    CommandResult::error(message, AppErrorCode::HardwareWalletError)
}

/// `cmd_discover_hardware_wallets` → one synthetic supported device.
pub fn discover_result() -> CommandResult {
    match synthetic_device()
        .and_then(|d| serde_json::to_value(vec![d]).map_err(|e| format!("serialize devices: {e}")))
    {
        Ok(data) => ok("Found 1 hardware wallet(s)", data),
        Err(e) => fail(e),
    }
}

/// `cmd_unlock_device` → the same synthetic device (already unlocked).
pub fn unlock_result() -> CommandResult {
    match synthetic_device()
        .and_then(|d| serde_json::to_value(d).map_err(|e| format!("serialize device: {e}")))
    {
        Ok(data) => ok("Device unlocked successfully", data),
        Err(e) => fail(e),
    }
}

/// `cmd_get_device_xpub` → account xpub + master fingerprint at the path.
pub fn device_xpub_result() -> CommandResult {
    let info = (|| -> Result<DeviceInfo, String> {
        let path = signer_path();
        let account = derive_account_at_path(NETWORK, &mnemonic()?, &passphrase(), &path)
            .map_err(|e| e.to_string())?;
        let secp = Secp256k1::new();
        Ok(DeviceInfo {
            xpub: Xpub::from_priv(&secp, &account).to_string(),
            fingerprint: fingerprint()?,
            derivation_path: path,
            device_type: DEVICE_TYPE.to_string(),
        })
    })();
    match info
        .and_then(|i| serde_json::to_value(i).map_err(|e| format!("serialize device info: {e}")))
    {
        Ok(data) => ok("Device info extracted successfully", data),
        Err(e) => fail(e),
    }
}

/// `cmd_sign_psbt` → sign the PSBT in-process with the master xprv.
pub fn sign_result(psbt_b64: &str) -> CommandResult {
    let signed = (|| -> Result<SignedPsbt, String> {
        let master = master()?;
        let secp = Secp256k1::new();
        let mut psbt = Psbt::from_str(psbt_b64).map_err(|e| format!("parse psbt: {e}"))?;
        // Signs every input whose `bip32_derivation` references our master
        // fingerprint, mutating the PSBT in place. The result lists inputs
        // we could not sign (those we don't own) — irrelevant here, so the
        // outcome is dropped. If nothing matched, the PSBT goes back
        // unsigned and the server rejects it, which is the correct signal.
        let _ = psbt.sign(&master, &secp);
        Ok(SignedPsbt {
            psbt: psbt.to_string(),
            fingerprint: fingerprint()?,
            derivation_path: signer_path(),
        })
    })();
    match signed
        .and_then(|s| serde_json::to_value(s).map_err(|e| format!("serialize signed psbt: {e}")))
    {
        Ok(data) => ok("PSBT signed successfully", data),
        Err(e) => fail(e),
    }
}
