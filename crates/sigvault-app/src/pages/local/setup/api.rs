//! `setup/api.ts` — typed wrappers around the slot-based local-wallet
//! commands, plus the hardware-wallet discovery commands the hardware
//! slot source needs. Shapes mirror `local_wallet::commands`; the core's
//! request structs are built through serde so the wire names stay the
//! single source of truth.

use std::future::Future;

use gpui::App;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sigvault_core::commands::types::CommandResult;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_get_device_xpub, cmd_unlock_device,
};
use sigvault_core::local_wallet::commands::{
    cmd_local_analyze_descriptor, cmd_local_create_from_slots, cmd_local_create_watch_only,
    cmd_local_decrypt_key_backup, cmd_local_derive_key, cmd_local_encrypt_key_backup,
    cmd_local_export_wallet_backup, cmd_local_generate_mnemonic, cmd_local_unlock_wallet,
    GenerateMnemonicRequest,
};
use sigvault_core::State;

use crate::runtime::backend;

/// Private key material for one slot. Mirrors `HotSecret`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HotSecret {
    Mnemonic {
        mnemonic: String,
        #[serde(default)]
        bip39_passphrase: String,
    },
    Xprv {
        xprv: String,
    },
}

impl From<&sigvault_core::local_wallet::storage::HotSecret> for HotSecret {
    fn from(s: &sigvault_core::local_wallet::storage::HotSecret) -> Self {
        use sigvault_core::local_wallet::storage::HotSecret as Core;
        match s {
            Core::Mnemonic {
                mnemonic,
                bip39_passphrase,
            } => HotSecret::Mnemonic {
                mnemonic: mnemonic.clone(),
                bip39_passphrase: bip39_passphrase.clone(),
            },
            Core::Xprv { xprv } => HotSecret::Xprv { xprv: xprv.clone() },
        }
    }
}

/// The public half of a slot — what ends up in the descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotKey {
    pub fingerprint: String,
    /// `m/...` form.
    pub derivation_path: String,
    pub xpub: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedKey {
    pub fingerprint: String,
    pub derivation_path: String,
    pub xpub: String,
    /// `[fp/path]xpub`, ready to paste into a descriptor.
    pub key_expr: String,
}

impl DerivedKey {
    pub fn slot_key(&self) -> SlotKey {
        SlotKey {
            fingerprint: self.fingerprint.clone(),
            derivation_path: self.derivation_path.clone(),
            xpub: self.xpub.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotSource {
    Generated,
    Seed,
    Xprv,
    Hardware,
    Xpub,
    KeyFile,
}

impl SlotSource {
    /// Parse a key-library `source` string (`crate::keychain::SOURCES`).
    pub fn from_key_source(s: &str) -> Option<Self> {
        serde_json::from_value(Value::String(s.to_string())).ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzedSlot {
    pub id: String,
    pub label: Option<String>,
    pub fingerprint: Option<String>,
    pub derivation_path: Option<String>,
    pub xpub: String,
    pub key_expr: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzedPath {
    pub id: String,
    pub label: String,
    /// 0 when the threshold could not be determined (custom descriptors).
    pub threshold: u32,
    pub timelock_blocks: Option<u32>,
    pub slot_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorAnalysis {
    pub external_descriptor: String,
    pub internal_descriptor: String,
    /// `singlesig` | `multisig` | `timelocked` | `recovery_only` | `custom`
    pub kind: String,
    /// `wpkh` | `sh_wpkh` | `wsh` | `tr` | `other`
    pub script: String,
    pub name_hint: Option<String>,
    pub paths: Vec<AnalyzedPath>,
    pub slots: Vec<AnalyzedSlot>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
    Taproot,
    Segwit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePath {
    pub threshold: u32,
    pub slots: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRecovery {
    pub timelock_blocks: u32,
    pub threshold: u32,
    pub slots: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WirePolicy {
    Singlesig,
    Multisig {
        threshold: u32,
    },
    Timelocked {
        script: ScriptKind,
        primary: Option<WirePath>,
        recoveries: Vec<WireRecovery>,
    },
    Descriptor {
        external_descriptor: String,
        internal_descriptor: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireSlot {
    pub label: String,
    pub source: SlotSource,
    pub key: SlotKey,
    pub secret: Option<HotSecret>,
}

/// Decrypted `.sigvault-key` file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenedKeyFile {
    pub secret: HotSecret,
    pub fingerprint: String,
    pub derivation_path: String,
    pub xpub: String,
}

/// `deriveKey` input.
#[derive(Clone, Debug, Serialize)]
pub struct DeriveInput {
    pub network: String,
    pub secret: HotSecret,
    pub derivation_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_path: Option<String>,
}

/// Build a core request (most only derive `Deserialize`) from JSON.
fn req<T: DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Re-shape a core response into this module's mirror type.
fn reshape<S: Serialize, T: DeserializeOwned>(v: S) -> Result<T, String> {
    serde_json::to_value(v)
        .and_then(serde_json::from_value)
        .map_err(|e| e.to_string())
}

type Fut<T> = std::pin::Pin<Box<dyn Future<Output = Result<T, String>> + 'static>>;

pub fn generate_mnemonic(cx: &App, word_count: u32) -> Fut<Vec<String>> {
    Box::pin(backend(cx).call(move |_app, st| async move {
        cmd_local_generate_mnemonic(State::new(&st), GenerateMnemonicRequest { word_count }).await
    }))
}

pub fn derive_key(cx: &App, input: DeriveInput) -> Fut<DerivedKey> {
    let v = serde_json::to_value(&input).unwrap_or(Value::Null);
    Box::pin(backend(cx).call(move |_app, st| async move {
        let out = cmd_local_derive_key(State::new(&st), req(v)?).await?;
        reshape(out)
    }))
}

pub fn analyze_descriptor(cx: &App, network: String, text: String) -> Fut<DescriptorAnalysis> {
    Box::pin(backend(cx).call(move |_app, st| async move {
        let r = req(json!({ "network": network, "text": text }))?;
        let out = cmd_local_analyze_descriptor(State::new(&st), r).await?;
        reshape(out)
    }))
}

pub struct CreateFromSlots {
    pub name: String,
    pub network: String,
    pub policy: WirePolicy,
    pub slots: Vec<WireSlot>,
    pub encrypt_passphrase: Option<String>,
}

pub fn create_from_slots(cx: &App, r: CreateFromSlots) -> Fut<String> {
    let v = json!({
        "name": r.name,
        "network": r.network,
        "policy": r.policy,
        "slots": r.slots,
        "encrypt_passphrase": r.encrypt_passphrase,
    });
    Box::pin(backend(cx).call(move |app, st| async move {
        let id = cmd_local_create_from_slots(app, State::new(&st), req(v)?).await?;
        Ok(id.as_str().to_string())
    }))
}

pub struct CreateWatchOnly {
    pub name: String,
    pub network: String,
    pub external_descriptor: String,
    pub internal_descriptor: String,
    pub fingerprints: Vec<String>,
}

pub fn create_watch_only(cx: &App, r: CreateWatchOnly) -> Fut<String> {
    let v = json!({
        "name": r.name,
        "network": r.network,
        "external_descriptor": r.external_descriptor,
        "internal_descriptor": r.internal_descriptor,
        "fingerprints": r.fingerprints,
        "spendable": false,
    });
    Box::pin(backend(cx).call(move |app, st| async move {
        let id = cmd_local_create_watch_only(app, State::new(&st), req(v)?).await?;
        Ok(id.as_str().to_string())
    }))
}

pub fn encrypt_key_backup(
    cx: &App,
    secret: HotSecret,
    password: String,
    key: SlotKey,
) -> Fut<String> {
    let v = json!({
        "secret": secret,
        "password": password,
        "fingerprint": key.fingerprint,
        "derivation_path": key.derivation_path,
        "xpub": key.xpub,
    });
    Box::pin(backend(cx).call(move |_app, st| async move {
        cmd_local_encrypt_key_backup(State::new(&st), req(v)?).await
    }))
}

pub fn decrypt_key_backup(cx: &App, contents: String, password: String) -> Fut<OpenedKeyFile> {
    let v = json!({ "contents": contents, "password": password });
    Box::pin(backend(cx).call(move |_app, st| async move {
        let out = cmd_local_decrypt_key_backup(State::new(&st), req(v)?).await?;
        reshape(out)
    }))
}

pub fn export_wallet_backup(cx: &App, wallet_id: String) -> Fut<String> {
    let v = json!({ "wallet_id": wallet_id });
    Box::pin(backend(cx).call(move |app, st| async move {
        cmd_local_export_wallet_backup(app, State::new(&st), req(v)?).await
    }))
}

pub fn unlock_wallet(cx: &App, wallet_id: String, passphrase: String) -> Fut<()> {
    let v = json!({ "wallet_id": wallet_id, "passphrase": passphrase });
    Box::pin(backend(cx).call(move |app, st| async move {
        cmd_local_unlock_wallet(app, State::new(&st), req(v)?).await
    }))
}

// ── hardware wallets (`components/DeviceDiscovery.tsx`) ──

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum UnsupportedReason {
    Version { minimal_supported_version: String },
    Method(String),
    NotPartOfWallet { fingerprint: String },
    WrongNetwork,
    AppNotOpen,
    InitializationError(String),
}

impl UnsupportedReason {
    pub fn text(&self) -> String {
        match self {
            UnsupportedReason::Version {
                minimal_supported_version,
            } => format!("Firmware version too old. Minimum required: {minimal_supported_version}"),
            UnsupportedReason::Method(m) => format!("Device doesn't support required feature: {m}"),
            UnsupportedReason::NotPartOfWallet { fingerprint } => {
                format!("Device fingerprint {fingerprint} is not part of this wallet")
            }
            UnsupportedReason::WrongNetwork => {
                "Device is configured for a different network".into()
            }
            UnsupportedReason::AppNotOpen => "Bitcoin app not open on device".into(),
            UnsupportedReason::InitializationError(e) => format!("Initialization error: {e}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "state")]
pub enum DeviceState {
    Locked {
        #[serde(default)]
        pairing_code: Option<String>,
    },
    Supported {
        fingerprint: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        registered: Option<bool>,
    },
    Unsupported {
        reason: UnsupportedReason,
    },
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceInfo {
    pub xpub: String,
    pub fingerprint: String,
    pub derivation_path: String,
    pub device_type: String,
}

/// `CommandResult<T>` → `Result<T>`, using `fallback` when the command
/// failed without a message.
fn command_data<T: DeserializeOwned>(r: CommandResult, fallback: &str) -> Result<T, String> {
    if !r.success {
        return Err(if r.message.is_empty() {
            fallback.to_string()
        } else {
            r.message
        });
    }
    match r.data {
        Some(d) => serde_json::from_value(d).map_err(|e| e.to_string()),
        None => Err(if r.message.is_empty() {
            fallback.to_string()
        } else {
            r.message
        }),
    }
}

pub fn discover_devices(cx: &App) -> Fut<Vec<DiscoveredDevice>> {
    Box::pin(backend(cx).call(move |app, st| async move {
        let window = app.get_webview_window("main").ok_or("no window")?;
        let r = cmd_discover_hardware_wallets(app, window, State::new(&st), None).await?;
        command_data(r, "Failed to discover devices")
    }))
}

pub fn unlock_device(cx: &App, device_id: String) -> Fut<DiscoveredDevice> {
    Box::pin(backend(cx).call(move |app, st| async move {
        let window = app.get_webview_window("main").ok_or("no window")?;
        let r = cmd_unlock_device(app, window, State::new(&st), device_id, None).await?;
        command_data(r, "Failed to unlock device")
    }))
}

pub fn get_device_xpub(cx: &App, fingerprint: String, derivation_path: String) -> Fut<DeviceInfo> {
    Box::pin(backend(cx).call(move |app, st| async move {
        let window = app.get_webview_window("main").ok_or("no window")?;
        let r = cmd_get_device_xpub(window, State::new(&st), fingerprint, derivation_path).await?;
        command_data(r, "Failed to extract device information")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigvault_core::local_wallet::commands::{
        CreateFromSlotsRequest, CreateWatchOnlyWalletRequest, DecryptKeyBackupRequest,
        DeriveKeyRequest, EncryptKeyBackupRequest, UnlockWalletRequest,
    };
    use sigvault_core::local_wallet::slots as core_slots;

    fn key() -> SlotKey {
        SlotKey {
            fingerprint: "aabbccdd".into(),
            derivation_path: "m/48'/1'/0'/2'".into(),
            xpub: "tpub".into(),
        }
    }

    #[test]
    fn requests_match_core_wire_shapes() {
        let secret = HotSecret::Mnemonic {
            mnemonic: "x".into(),
            bip39_passphrase: String::new(),
        };
        let v = json!({
            "name": "n",
            "network": "regtest",
            "policy": WirePolicy::Timelocked {
                script: ScriptKind::Taproot,
                primary: Some(WirePath { threshold: 1, slots: vec![0] }),
                recoveries: vec![WireRecovery { timelock_blocks: 4320, threshold: 1, slots: vec![1] }],
            },
            "slots": vec![
                WireSlot { label: "a".into(), source: SlotSource::KeyFile, key: key(), secret: Some(secret.clone()) },
                WireSlot { label: "b".into(), source: SlotSource::Hardware, key: key(), secret: None },
            ],
            "encrypt_passphrase": "pass1234",
        });
        req::<CreateFromSlotsRequest>(v).unwrap();
        req::<CreateFromSlotsRequest>(json!({
            "name": "n", "network": "regtest",
            "policy": WirePolicy::Descriptor { external_descriptor: "a".into(), internal_descriptor: "b".into() },
            "slots": [], "encrypt_passphrase": null,
        }))
        .unwrap();
        req::<DeriveKeyRequest>(
            serde_json::to_value(DeriveInput {
                network: "regtest".into(),
                secret: HotSecret::Xprv {
                    xprv: "tprv".into(),
                },
                derivation_path: "m/84'/1'/0'".into(),
                origin_fingerprint: None,
                origin_path: None,
            })
            .unwrap(),
        )
        .unwrap();
        req::<EncryptKeyBackupRequest>(json!({
            "secret": secret, "password": "p", "fingerprint": "f", "derivation_path": "m", "xpub": "x",
        }))
        .unwrap();
        req::<DecryptKeyBackupRequest>(json!({ "contents": "c", "password": "p" })).unwrap();
        req::<UnlockWalletRequest>(json!({ "wallet_id": "w", "passphrase": "" })).unwrap();
        req::<CreateWatchOnlyWalletRequest>(json!({
            "name": "n", "network": "regtest", "external_descriptor": "a",
            "internal_descriptor": "b", "fingerprints": [], "spendable": false,
        }))
        .unwrap();
    }

    #[test]
    fn every_library_source_maps_to_a_slot_source() {
        for (s, _) in crate::keychain::SOURCES {
            assert!(SlotSource::from_key_source(s).is_some(), "{s}");
        }
        assert_eq!(
            SlotSource::from_key_source("key_file"),
            Some(SlotSource::KeyFile)
        );
        assert_eq!(SlotSource::from_key_source("nope"), None);
    }

    #[test]
    fn core_responses_reshape() {
        let a = core_slots::DescriptorAnalysis {
            external_descriptor: "e".into(),
            internal_descriptor: "i".into(),
            kind: "multisig".into(),
            script: "wsh".into(),
            name_hint: None,
            paths: vec![core_slots::AnalyzedPath {
                id: "primary".into(),
                label: "2-of-2".into(),
                threshold: 2,
                timelock_blocks: None,
                slot_ids: vec!["s0".into()],
            }],
            slots: vec![core_slots::AnalyzedSlot {
                id: "s0".into(),
                label: None,
                fingerprint: Some("aabbccdd".into()),
                derivation_path: Some("m/48'/1'/0'/2'".into()),
                xpub: "tpub".into(),
                key_expr: "[aabbccdd/48'/1'/0'/2']tpub".into(),
            }],
            warnings: vec![],
        };
        let mine: DescriptorAnalysis = reshape(a).unwrap();
        assert_eq!(mine.paths[0].threshold, 2);
        let d: DerivedKey = reshape(core_slots::DerivedKey {
            fingerprint: "f".into(),
            derivation_path: "m".into(),
            xpub: "x".into(),
            key_expr: "k".into(),
        })
        .unwrap();
        assert_eq!(d.key_expr, "k");
    }
}
