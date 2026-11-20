//! Slot-based wallet building blocks.
//!
//! The create and recover wizards model every wallet as a policy with
//! key *slots*. Each slot is filled from a hardware device, a pasted
//! xpub, or a private key held on this machine (freshly generated, a
//! typed BIP39 seed, an xprv, or an encrypted key file). This module
//! holds the pieces those flows share:
//!
//! - deriving a slot's public key from its secret (`resolve_secret`),
//! - analysing a pasted descriptor (or wallet backup file) into its
//!   policy, spending paths and slots (`analyze_descriptor`),
//! - the encrypted single-key backup file (`.sigvault-key`) and the
//!   secret-free wallet backup file.
//!
//! Wallet persistence lives in `manager::create_from_slots`.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use bdk_wallet::bitcoin::bip32::{DerivationPath, Fingerprint, Xpriv, Xpub};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::{Network, NetworkKind};
use bdk_wallet::keys::bip39::{Language, Mnemonic};
use bdk_wallet::miniscript::descriptor::{
    Descriptor, DescriptorPublicKey, ShInner, SinglePubKey, WshInner,
};
use bdk_wallet::miniscript::{ForEachKey, Terminal};
use serde::{Deserialize, Serialize};
use wallet_runtime::{list_spending_paths, PolicyDescriptor};
use zeroize::Zeroizing;

use super::manager::{derive_master_xpriv, ManagerError};
use super::storage::{decrypt_seed, encrypt_seed, EncryptedSeed, HotSecret};

pub const KEY_BACKUP_FORMAT: &str = "sigvault-key";
pub const WALLET_BACKUP_FORMAT: &str = "sigvault-wallet-backup";
const BACKUP_VERSION: u32 = 1;
const MIN_PASSWORD_LEN: usize = 8;

fn err(msg: impl Into<String>) -> ManagerError {
    ManagerError::Runtime(msg.into())
}

// ---------- paths + fingerprints ----------

/// Parse a BIP32 path with or without the leading `m/`.
pub fn parse_path(path: &str) -> Result<DerivationPath, ManagerError> {
    let trimmed = path.trim();
    let normalized = trimmed
        .strip_prefix("m/")
        .or_else(|| trimmed.strip_prefix('m'))
        .unwrap_or(trimmed);
    if normalized.is_empty() {
        return Ok(DerivationPath::master());
    }
    DerivationPath::from_str(normalized)
        .map_err(|e| err(format!("invalid derivation path '{path}': {e}")))
}

/// Render a path as `m/48'/1'/0'/2'` (`m` for the master).
pub fn fmt_path(path: &DerivationPath) -> String {
    let body = path.to_string();
    let body = body
        .strip_prefix("m/")
        .or_else(|| body.strip_prefix('m'))
        .unwrap_or(&body)
        .to_string();
    if body.is_empty() {
        "m".to_string()
    } else {
        format!("m/{body}")
    }
}

/// `[fp/48'/1'/0'/2']xpub…` — the origin-tagged key expression used in
/// descriptors (without a trailing derivation step).
pub fn key_expr(fingerprint: &str, path: &DerivationPath, xpub: &str) -> String {
    let p = fmt_path(path);
    let body = p.strip_prefix("m/").unwrap_or("");
    if body.is_empty() {
        format!("[{fingerprint}]{xpub}")
    } else {
        format!("[{fingerprint}/{body}]{xpub}")
    }
}

pub fn parse_fingerprint(fp: &str) -> Result<Fingerprint, ManagerError> {
    Fingerprint::from_str(fp.trim()).map_err(|_| err(format!("invalid fingerprint '{fp}'")))
}

fn network_label(kind: NetworkKind) -> &'static str {
    match kind {
        NetworkKind::Main => "mainnet",
        NetworkKind::Test => "test networks",
    }
}

fn check_network_kind(
    found: NetworkKind,
    network: Network,
    what: &str,
) -> Result<(), ManagerError> {
    let expected = NetworkKind::from(network);
    if found != expected {
        return Err(err(format!(
            "{what} is for {} but the wallet network is {network}",
            network_label(found)
        )));
    }
    Ok(())
}

pub fn parse_xpub(xpub: &str, network: Network) -> Result<Xpub, ManagerError> {
    let x = Xpub::from_str(xpub.trim()).map_err(|e| err(format!("invalid xpub: {e}")))?;
    check_network_kind(x.network, network, "the xpub")?;
    Ok(x)
}

/// Two xpubs are the same key when their public key and chain code
/// agree; the version bytes (xpub/tpub) and depth metadata don't matter.
pub fn same_xpub(a: &Xpub, b: &Xpub) -> bool {
    a.public_key == b.public_key && a.chain_code == b.chain_code
}

// ---------- secrets ----------

/// A slot key derived from its private material.
pub struct ResolvedKey {
    pub account_xpriv: Xpriv,
    pub fingerprint: Fingerprint,
    pub derivation_path: DerivationPath,
    pub xpub: Xpub,
}

/// Wire form of a derived slot key, returned to the wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedKey {
    pub fingerprint: String,
    pub derivation_path: String,
    pub xpub: String,
    pub key_expr: String,
}

impl From<&ResolvedKey> for DerivedKey {
    fn from(k: &ResolvedKey) -> Self {
        let fingerprint = k.fingerprint.to_string();
        let xpub = k.xpub.to_string();
        DerivedKey {
            key_expr: key_expr(&fingerprint, &k.derivation_path, &xpub),
            fingerprint,
            derivation_path: fmt_path(&k.derivation_path),
            xpub,
        }
    }
}

/// Split `[fp/path]tprv…/0/*` into its origin and bare xprv.
fn split_xprv_input(raw: &str) -> Result<(Option<(String, String)>, String), ManagerError> {
    let mut s = raw.trim();
    let mut origin = None;
    if let Some(rest) = s.strip_prefix('[') {
        let close = rest
            .find(']')
            .ok_or_else(|| err("unterminated key origin '[' in xprv"))?;
        let inner = &rest[..close];
        let (fp, path) = inner.split_once('/').unwrap_or((inner, ""));
        origin = Some((fp.to_string(), path.to_string()));
        s = &rest[close + 1..];
    }
    let bare = s
        .trim_end_matches("/<0;1>/*")
        .trim_end_matches("/0/*")
        .trim_end_matches("/1/*")
        .trim()
        .to_string();
    Ok((origin, bare))
}

/// Derive the slot key a secret produces.
///
/// - Mnemonic and master xprv (depth 0) derive down `derivation_path`.
/// - Account-level xprv is used as-is; its origin comes from a pasted
///   `[fp/path]` prefix or the `origin_*` arguments.
pub fn resolve_secret(
    network: Network,
    secret: &HotSecret,
    derivation_path: Option<&str>,
    origin_fingerprint: Option<&str>,
    origin_path: Option<&str>,
) -> Result<ResolvedKey, ManagerError> {
    let secp = Secp256k1::new();
    let require_path = || {
        derivation_path
            .filter(|p| !p.trim().is_empty())
            .ok_or_else(|| err("a derivation path is required for this key"))
            .and_then(parse_path)
    };
    match secret {
        HotSecret::Mnemonic {
            mnemonic,
            bip39_passphrase,
        } => {
            let master = derive_master_xpriv(network, mnemonic.trim(), bip39_passphrase)?;
            let path = require_path()?;
            let account = master
                .derive_priv(&secp, &path)
                .map_err(|e| err(format!("derive at {}: {e}", fmt_path(&path))))?;
            Ok(ResolvedKey {
                xpub: Xpub::from_priv(&secp, &account),
                account_xpriv: account,
                fingerprint: master.fingerprint(&secp),
                derivation_path: path,
            })
        }
        HotSecret::Xprv { xprv } => {
            let (pasted_origin, bare) = split_xprv_input(xprv)?;
            let bare = Zeroizing::new(bare);
            let key = Xpriv::from_str(&bare).map_err(|e| err(format!("invalid xprv: {e}")))?;
            check_network_kind(key.network, network, "the xprv")?;
            if key.depth == 0 {
                let path = require_path()?;
                let account = key
                    .derive_priv(&secp, &path)
                    .map_err(|e| err(format!("derive at {}: {e}", fmt_path(&path))))?;
                return Ok(ResolvedKey {
                    xpub: Xpub::from_priv(&secp, &account),
                    account_xpriv: account,
                    fingerprint: key.fingerprint(&secp),
                    derivation_path: path,
                });
            }
            let (fp, path) = match (pasted_origin, origin_fingerprint, origin_path) {
                (Some((fp, path)), _, _) => (fp, path),
                (None, Some(fp), Some(path)) if !fp.trim().is_empty() => {
                    (fp.to_string(), path.to_string())
                }
                _ => {
                    return Err(err(
                        "account-level xprv needs its origin [fingerprint/path]",
                    ))
                }
            };
            let fingerprint = parse_fingerprint(&fp)?;
            let path = parse_path(&path)?;
            if path.len() != key.depth as usize {
                return Err(err(format!(
                    "xprv depth {} does not match origin path {}",
                    key.depth,
                    fmt_path(&path)
                )));
            }
            Ok(ResolvedKey {
                xpub: Xpub::from_priv(&secp, &key),
                account_xpriv: key,
                fingerprint,
                derivation_path: path,
            })
        }
    }
}

/// Generate a fresh BIP39 mnemonic. Not persisted anywhere.
pub fn generate_mnemonic(word_count: u32) -> Result<Vec<String>, ManagerError> {
    let entropy_len = match word_count {
        12 => 16,
        24 => 32,
        other => return Err(err(format!("word_count must be 12 or 24 (got {other})"))),
    };
    let mut entropy = Zeroizing::new(vec![0u8; entropy_len]);
    getrandom::getrandom(&mut entropy).map_err(|e| err(format!("rng: {e}")))?;
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map_err(|e| err(format!("mnemonic: {e}")))?;
    Ok(mnemonic.words().map(str::to_string).collect())
}

// ---------- key backup file ----------

#[derive(Debug, Serialize, Deserialize)]
struct KeyBackupFile {
    format: String,
    version: u32,
    fingerprint: String,
    derivation_path: String,
    xpub: String,
    envelope: EncryptedSeed,
}

/// Decrypted contents of a `.sigvault-key` file.
#[derive(Debug, Serialize, Deserialize)]
pub struct KeyBackupContents {
    pub secret: HotSecret,
    pub fingerprint: String,
    pub derivation_path: String,
    pub xpub: String,
}

pub fn encrypt_key_backup(
    secret: &HotSecret,
    password: &str,
    fingerprint: &str,
    derivation_path: &str,
    xpub: &str,
) -> Result<String, ManagerError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(err(format!(
            "backup password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    let plaintext = Zeroizing::new(serde_json::to_vec(secret)?);
    let envelope = encrypt_seed(&plaintext, password.as_bytes())?;
    let file = KeyBackupFile {
        format: KEY_BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        fingerprint: fingerprint.to_lowercase(),
        derivation_path: fmt_path(&parse_path(derivation_path)?),
        xpub: xpub.to_string(),
        envelope,
    };
    Ok(serde_json::to_string_pretty(&file)?)
}

pub fn decrypt_key_backup(
    contents: &str,
    password: &str,
) -> Result<KeyBackupContents, ManagerError> {
    let file: KeyBackupFile = serde_json::from_str(contents.trim())
        .map_err(|e| err(format!("not a sigvault key file: {e}")))?;
    if file.format != KEY_BACKUP_FORMAT {
        return Err(err(format!(
            "not a sigvault key file (format '{}')",
            file.format
        )));
    }
    if file.version != BACKUP_VERSION {
        return Err(err(format!(
            "unsupported key file version {}",
            file.version
        )));
    }
    let plaintext = Zeroizing::new(decrypt_seed(&file.envelope, password.as_bytes())?);
    let secret: HotSecret =
        serde_json::from_slice(&plaintext).map_err(|e| err(format!("key file payload: {e}")))?;
    Ok(KeyBackupContents {
        secret,
        fingerprint: file.fingerprint,
        derivation_path: file.derivation_path,
        xpub: file.xpub,
    })
}

// ---------- wallet backup file ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBackupSlot {
    pub label: String,
    pub fingerprint: Option<String>,
    pub derivation_path: Option<String>,
    pub xpub: String,
    pub source: String,
}

/// Secret-free wallet backup: everything needed to rebuild the wallet's
/// addresses, plus the labels the user gave each key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBackup {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub network: String,
    pub policy_type: String,
    pub external_descriptor: String,
    pub internal_descriptor: String,
    #[serde(default)]
    pub slots: Vec<WalletBackupSlot>,
    pub created_at: i64,
}

impl WalletBackup {
    pub fn new(
        name: String,
        network: String,
        policy_type: String,
        external_descriptor: String,
        internal_descriptor: String,
        slots: Vec<WalletBackupSlot>,
        created_at: i64,
    ) -> Self {
        WalletBackup {
            format: WALLET_BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION,
            name,
            network,
            policy_type,
            external_descriptor,
            internal_descriptor,
            slots,
            created_at,
        }
    }
}

// ---------- descriptor analysis ----------

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzedPath {
    pub id: String,
    pub label: String,
    /// `0` when the threshold could not be determined (custom scripts).
    pub threshold: u32,
    pub timelock_blocks: Option<u32>,
    pub slot_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzedSlot {
    pub id: String,
    pub label: Option<String>,
    pub fingerprint: Option<String>,
    pub derivation_path: Option<String>,
    pub xpub: String,
    pub key_expr: String,
}

#[derive(Debug, Clone, Serialize)]
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

fn strip_checksum(s: &str) -> &str {
    s.split('#').next().unwrap_or(s).trim()
}

fn parse_public_descriptor(s: &str) -> Result<Descriptor<DescriptorPublicKey>, ManagerError> {
    const PRIVATE: &str = "descriptor contains private keys; paste the public descriptor";
    // Checked up front: miniscript rejects some private forms (multipath
    // xprvs) with an unrelated parse error before reporting the keymap.
    if s.contains("xprv") || s.contains("tprv") {
        return Err(err(PRIVATE));
    }
    let secp = Secp256k1::new();
    let (desc, keymap) = Descriptor::<DescriptorPublicKey>::parse_descriptor(&secp, s.trim())
        .map_err(|e| err(format!("invalid descriptor: {e}")))?;
    if !keymap.is_empty() {
        return Err(err(PRIVATE));
    }
    Ok(desc)
}

/// Turn the user's input into an external / internal descriptor pair.
fn split_input(
    network: Network,
    text: &str,
    warnings: &mut Vec<String>,
) -> Result<(String, String, Option<WalletBackup>), ManagerError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(err("paste a descriptor or open a wallet backup file"));
    }
    if trimmed.starts_with('{') {
        let backup: WalletBackup = serde_json::from_str(trimmed)
            .map_err(|e| err(format!("not a sigvault wallet backup: {e}")))?;
        if backup.format != WALLET_BACKUP_FORMAT {
            return Err(err(format!(
                "not a sigvault wallet backup (format '{}')",
                backup.format
            )));
        }
        if backup.network != network.to_string() {
            warnings.push(format!(
                "Backup was made for {}, recovering on {network}.",
                backup.network
            ));
        }
        return Ok((
            strip_checksum(&backup.external_descriptor).to_string(),
            strip_checksum(&backup.internal_descriptor).to_string(),
            Some(backup),
        ));
    }

    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    match lines.as_slice() {
        [single] => {
            let desc = parse_public_descriptor(single)?;
            if desc.is_multipath() {
                let singles = desc
                    .into_single_descriptors()
                    .map_err(|e| err(format!("multipath descriptor: {e}")))?;
                if singles.len() != 2 {
                    return Err(err(format!(
                        "multipath descriptor must have exactly 2 paths (receive and change), found {}",
                        singles.len()
                    )));
                }
                return Ok((
                    strip_checksum(&singles[0].to_string()).to_string(),
                    strip_checksum(&singles[1].to_string()).to_string(),
                    None,
                ));
            }
            let ext = strip_checksum(single).to_string();
            if !ext.contains("/0/*") {
                return Err(err(
                    "descriptor has no receive/change derivation (expected /0/* or /<0;1>/*)",
                ));
            }
            warnings.push(
                "Only a receive descriptor was given; the change descriptor was derived by using /1/* for every key."
                    .to_string(),
            );
            let int = ext.replace("/0/*", "/1/*");
            Ok((ext, int, None))
        }
        [ext, int] => Ok((
            strip_checksum(ext).to_string(),
            strip_checksum(int).to_string(),
            None,
        )),
        _ => Err(err(
            "expected one multipath descriptor or two descriptors (receive, then change)",
        )),
    }
}

/// A slot's descriptor origin, used to map policy paths back to slots.
type KeyOrigin = (Option<Fingerprint>, Option<DerivationPath>);

struct KeyInfo {
    fingerprint: Option<Fingerprint>,
    origin_path: Option<DerivationPath>,
    xpub: String,
    network: Option<NetworkKind>,
}

fn key_info(k: &DescriptorPublicKey) -> KeyInfo {
    match k {
        DescriptorPublicKey::XPub(x) => KeyInfo {
            fingerprint: x.origin.as_ref().map(|(fp, _)| *fp),
            origin_path: x.origin.as_ref().map(|(_, p)| p.clone()),
            xpub: x.xkey.to_string(),
            network: Some(x.xkey.network),
        },
        DescriptorPublicKey::MultiXPub(x) => KeyInfo {
            fingerprint: x.origin.as_ref().map(|(fp, _)| *fp),
            origin_path: x.origin.as_ref().map(|(_, p)| p.clone()),
            xpub: x.xkey.to_string(),
            network: Some(x.xkey.network),
        },
        DescriptorPublicKey::Single(s) => KeyInfo {
            fingerprint: s.origin.as_ref().map(|(fp, _)| *fp),
            origin_path: s.origin.as_ref().map(|(_, p)| p.clone()),
            xpub: match &s.key {
                SinglePubKey::FullKey(pk) => pk.to_string(),
                SinglePubKey::XOnly(pk) => pk.to_string(),
            },
            network: None,
        },
    }
}

fn collect_slots(
    desc: &Descriptor<DescriptorPublicKey>,
    skip_xpub: Option<&str>,
    network: Network,
) -> Result<(Vec<AnalyzedSlot>, Vec<KeyOrigin>), ManagerError> {
    let mut keys: Vec<DescriptorPublicKey> = Vec::new();
    desc.for_each_key(|k| {
        keys.push(k.clone());
        true
    });
    let mut seen: HashSet<(Option<Fingerprint>, String)> = HashSet::new();
    let mut slots = Vec::new();
    let mut origins = Vec::new();
    for k in keys {
        let info = key_info(&k);
        if Some(info.xpub.as_str()) == skip_xpub {
            continue;
        }
        if let Some(kind) = info.network {
            check_network_kind(kind, network, "the descriptor's keys")?;
        }
        if !seen.insert((info.fingerprint, info.xpub.clone())) {
            continue;
        }
        let fingerprint = info.fingerprint.map(|f| f.to_string());
        let derivation_path = info.origin_path.as_ref().map(fmt_path);
        let expr = match (&fingerprint, &info.origin_path) {
            (Some(fp), Some(path)) => key_expr(fp, path, &info.xpub),
            _ => info.xpub.clone(),
        };
        slots.push(AnalyzedSlot {
            id: format!("k{}", slots.len()),
            label: None,
            fingerprint,
            derivation_path,
            xpub: info.xpub,
            key_expr: expr,
        });
        origins.push((info.fingerprint, info.origin_path));
    }
    Ok((slots, origins))
}

/// Multipath form of a receive/change pair, when the pair follows the
/// `/0/*` ↔ `/1/*` convention. Liana's parser needs this shape.
fn multipath_of(ext: &str, int: &str) -> Option<String> {
    if !ext.contains("/0/*") || ext.replace("/0/*", "/1/*") != int {
        return None;
    }
    Some(ext.replace("/0/*", "/<0;1>/*"))
}

fn load_check(ext: &str, int: &str, network: Network) -> Result<(), ManagerError> {
    bdk_wallet::Wallet::create(ext.to_string(), int.to_string())
        .network(network)
        .create_wallet_no_persist()
        .map(|_| ())
        .map_err(|e| err(format!("descriptor pair does not load: {e}")))
}

/// Analyse a pasted descriptor (one multipath, a receive/change pair, a
/// receive-only `/0/*`) or a sigvault wallet backup JSON.
pub fn analyze_descriptor(
    network: Network,
    text: &str,
) -> Result<DescriptorAnalysis, ManagerError> {
    let mut warnings = Vec::new();
    let (ext, int, backup) = split_input(network, text, &mut warnings)?;
    let ext_desc = parse_public_descriptor(&ext)?;
    let int_desc = parse_public_descriptor(&int)?;
    if ext_desc.is_multipath() || int_desc.is_multipath() {
        return Err(err(
            "pass either one multipath descriptor or two single-path descriptors",
        ));
    }
    load_check(&ext, &int, network)?;

    let multipath = multipath_of(&ext, &int);
    let policy = multipath
        .as_deref()
        .and_then(|m| PolicyDescriptor::from_str(m).ok());

    // The recovery-only internal key is a provably unspendable sentinel,
    // not a slot anyone fills.
    let skip_internal = match (&policy, &ext_desc) {
        (Some(PolicyDescriptor::RecoveryOnly(_)), Descriptor::Tr(tr)) => {
            Some(key_info(tr.internal_key()).xpub)
        }
        _ => None,
    };
    let (mut slots, origins) = collect_slots(&ext_desc, skip_internal.as_deref(), network)?;
    let all_ids: Vec<String> = slots.iter().map(|s| s.id.clone()).collect();

    let ids_for = |fps: &HashMap<Fingerprint, HashSet<DerivationPath>>| -> Vec<String> {
        slots
            .iter()
            .zip(origins.iter())
            .filter(|(_, (fp, path))| match (fp, path) {
                // Liana reports origin path + the multipath step
                // (`…/0`, `…/1`), so match on the origin prefix.
                (Some(fp), Some(path)) => fps.get(fp).is_some_and(|paths| {
                    paths.iter().any(|p| p.as_ref().starts_with(path.as_ref()))
                }),
                (Some(fp), None) => fps.contains_key(fp),
                _ => false,
            })
            .map(|(s, _)| s.id.clone())
            .collect()
    };
    let ids_for_fps = |fps: &[String]| -> Vec<String> {
        slots
            .iter()
            .filter(|s| s.fingerprint.as_ref().is_some_and(|f| fps.contains(f)))
            .map(|s| s.id.clone())
            .collect()
    };

    let script = match &ext_desc {
        Descriptor::Wpkh(_) => "wpkh",
        Descriptor::Sh(sh) if matches!(sh.as_inner(), ShInner::Wpkh(_)) => "sh_wpkh",
        Descriptor::Wsh(_) => "wsh",
        Descriptor::Tr(_) => "tr",
        _ => "other",
    }
    .to_string();

    let single_path = |label: &str, threshold: u32| AnalyzedPath {
        id: "default".to_string(),
        label: label.to_string(),
        threshold,
        timelock_blocks: None,
        slot_ids: all_ids.clone(),
    };

    let (kind, paths) = match (&policy, &ext_desc) {
        (Some(PolicyDescriptor::Liana(desc)), _) => {
            let spending =
                list_spending_paths("timelocked", multipath.as_deref().unwrap_or(""), &[], None)
                    .map_err(|e| err(format!("spending paths: {e}")))?;
            let pol = desc.policy();
            let mut origin_sets = vec![pol.primary_path().thresh_origins().1];
            for info in pol.recovery_paths().values() {
                origin_sets.push(info.thresh_origins().1);
            }
            let paths = spending
                .into_iter()
                .zip(origin_sets.iter())
                .map(|(p, origins)| AnalyzedPath {
                    slot_ids: ids_for(origins),
                    id: p.id,
                    label: p.label,
                    threshold: p.threshold,
                    timelock_blocks: p.timelock_blocks,
                })
                .collect();
            ("timelocked", paths)
        }
        (Some(PolicyDescriptor::RecoveryOnly(_)), _) => {
            let spending =
                list_spending_paths("timelocked", multipath.as_deref().unwrap_or(""), &[], None)
                    .map_err(|e| err(format!("spending paths: {e}")))?;
            let paths = spending
                .into_iter()
                .map(|p| AnalyzedPath {
                    slot_ids: ids_for_fps(&p.fingerprints),
                    id: p.id,
                    label: p.label,
                    threshold: p.threshold,
                    timelock_blocks: p.timelock_blocks,
                })
                .collect();
            ("recovery_only", paths)
        }
        (None, Descriptor::Wpkh(_)) => ("singlesig", vec![single_path("Single key", 1)]),
        (None, Descriptor::Sh(sh)) if matches!(sh.as_inner(), ShInner::Wpkh(_)) => {
            ("singlesig", vec![single_path("Single key", 1)])
        }
        (None, Descriptor::Tr(tr)) if tr.tap_tree().is_none() => {
            ("singlesig", vec![single_path("Single key", 1)])
        }
        (None, Descriptor::Wsh(wsh)) => {
            let k = match wsh.as_inner() {
                WshInner::SortedMulti(smv) => Some(smv.k()),
                WshInner::Ms(ms) => match &ms.node {
                    Terminal::Multi(thresh) => Some(thresh.k()),
                    _ => None,
                },
            };
            match k {
                Some(k) => {
                    let label = format!("{}-of-{}", k, slots.len());
                    ("multisig", vec![single_path(&label, k as u32)])
                }
                None => ("custom", vec![single_path("Custom script", 0)]),
            }
        }
        _ => ("custom", vec![single_path("Custom script", 0)]),
    };

    if kind == "custom" {
        warnings.push(
            "This descriptor isn't a standard single-sig, multisig or timelocked policy. It can be imported, but spending paths can't be shown."
                .to_string(),
        );
    }

    let name_hint = backup.as_ref().map(|b| b.name.clone());
    if let Some(backup) = &backup {
        for slot in slots.iter_mut() {
            slot.label = backup
                .slots
                .iter()
                .find(|b| b.xpub == slot.xpub)
                .map(|b| b.label.clone());
        }
    }

    Ok(DescriptorAnalysis {
        external_descriptor: ext,
        internal_descriptor: int,
        kind: kind.to_string(),
        script,
        name_hint,
        paths,
        slots,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABANDON: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn mnemonic(m: &str) -> HotSecret {
        HotSecret::Mnemonic {
            mnemonic: m.to_string(),
            bip39_passphrase: String::new(),
        }
    }

    #[test]
    fn derives_known_master_fingerprint() {
        let k = resolve_secret(
            Network::Regtest,
            &mnemonic(ABANDON),
            Some("m/84'/1'/0'"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(k.fingerprint.to_string(), "73c5da0a");
        let d = DerivedKey::from(&k);
        assert_eq!(d.derivation_path, "m/84'/1'/0'");
        assert!(d.key_expr.starts_with("[73c5da0a/84'/1'/0']tpub"));
    }

    #[test]
    fn account_xprv_needs_origin_and_master_xprv_derives() {
        let secp = Secp256k1::new();
        let master = derive_master_xpriv(Network::Regtest, ABANDON, "").unwrap();
        let from_master = resolve_secret(
            Network::Regtest,
            &HotSecret::Xprv {
                xprv: master.to_string(),
            },
            Some("m/48'/1'/0'/2'"),
            None,
            None,
        )
        .unwrap();
        let from_seed = resolve_secret(
            Network::Regtest,
            &mnemonic(ABANDON),
            Some("48'/1'/0'/2'"),
            None,
            None,
        )
        .unwrap();
        assert!(same_xpub(&from_master.xpub, &from_seed.xpub));

        let account = from_seed.account_xpriv.to_string();
        let missing = resolve_secret(
            Network::Regtest,
            &HotSecret::Xprv {
                xprv: account.clone(),
            },
            None,
            None,
            None,
        );
        assert!(missing.is_err());
        let with_origin = resolve_secret(
            Network::Regtest,
            &HotSecret::Xprv {
                xprv: format!("[73c5da0a/48'/1'/0'/2']{account}/<0;1>/*"),
            },
            None,
            None,
            None,
        )
        .unwrap();
        assert!(same_xpub(&with_origin.xpub, &from_seed.xpub));
        assert_eq!(with_origin.fingerprint, master.fingerprint(&secp));
    }

    #[test]
    fn rejects_mainnet_xprv_on_regtest() {
        let master = derive_master_xpriv(Network::Bitcoin, ABANDON, "").unwrap();
        let res = resolve_secret(
            Network::Regtest,
            &HotSecret::Xprv {
                xprv: master.to_string(),
            },
            Some("m/84'/1'/0'"),
            None,
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn generate_mnemonic_lengths() {
        assert_eq!(generate_mnemonic(12).unwrap().len(), 12);
        assert_eq!(generate_mnemonic(24).unwrap().len(), 24);
        assert!(generate_mnemonic(18).is_err());
    }

    #[test]
    fn key_backup_round_trip_and_wrong_password() {
        let file = encrypt_key_backup(
            &mnemonic(ABANDON),
            "long enough",
            "73C5DA0A",
            "48'/1'/0'/2'",
            "tpubX",
        )
        .unwrap();
        assert!(!file.contains("abandon"), "secret must not appear in clear");
        let back = decrypt_key_backup(&file, "long enough").unwrap();
        assert_eq!(back.fingerprint, "73c5da0a");
        assert_eq!(back.derivation_path, "m/48'/1'/0'/2'");
        match &back.secret {
            HotSecret::Mnemonic { mnemonic, .. } => assert_eq!(mnemonic, ABANDON),
            other => panic!("expected mnemonic, got {other:?}"),
        }
        assert!(decrypt_key_backup(&file, "wrong password").is_err());
        assert!(encrypt_key_backup(&mnemonic(ABANDON), "short", "73c5da0a", "m", "x").is_err());
    }

    fn key(m: &str, path: &str) -> String {
        DerivedKey::from(
            &resolve_secret(Network::Regtest, &mnemonic(m), Some(path), None, None).unwrap(),
        )
        .key_expr
    }

    const SECOND: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";
    const THIRD: &str =
        "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

    #[test]
    fn analyzes_wpkh_pair() {
        let k = key(ABANDON, "m/84'/1'/0'");
        let text = format!("wpkh({k}/0/*)\nwpkh({k}/1/*)");
        let a = analyze_descriptor(Network::Regtest, &text).unwrap();
        assert_eq!(a.kind, "singlesig");
        assert_eq!(a.script, "wpkh");
        assert_eq!(a.slots.len(), 1);
        assert_eq!(a.slots[0].fingerprint.as_deref(), Some("73c5da0a"));
        assert_eq!(a.paths[0].slot_ids, vec!["k0".to_string()]);
    }

    #[test]
    fn analyzes_multipath_sortedmulti() {
        let keys = [ABANDON, SECOND, THIRD]
            .iter()
            .map(|m| format!("{}/<0;1>/*", key(m, "m/48'/1'/0'/2'")))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("wsh(sortedmulti(2,{keys}))");
        let a = analyze_descriptor(Network::Regtest, &text).unwrap();
        assert_eq!(a.kind, "multisig");
        assert_eq!(a.paths[0].threshold, 2);
        assert_eq!(a.paths[0].label, "2-of-3");
        assert_eq!(a.slots.len(), 3);
        assert!(a.external_descriptor.contains("/0/*"));
        assert!(a.internal_descriptor.contains("/1/*"));
    }

    #[test]
    fn receive_only_descriptor_derives_change_with_warning() {
        let k = key(ABANDON, "m/84'/1'/0'");
        let a = analyze_descriptor(Network::Regtest, &format!("wpkh({k}/0/*)")).unwrap();
        assert_eq!(a.internal_descriptor, format!("wpkh({k}/1/*)"));
        assert_eq!(a.warnings.len(), 1);
    }

    #[test]
    fn rejects_private_descriptor() {
        let master = derive_master_xpriv(Network::Regtest, ABANDON, "").unwrap();
        let res = analyze_descriptor(
            Network::Regtest,
            &format!("wpkh({master}/84'/1'/0'/<0;1>/*)"),
        );
        let msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("private keys"), "{msg}");
    }

    #[test]
    fn rejects_network_mismatch() {
        let k = DerivedKey::from(
            &resolve_secret(
                Network::Bitcoin,
                &mnemonic(ABANDON),
                Some("m/84'/0'/0'"),
                None,
                None,
            )
            .unwrap(),
        )
        .key_expr;
        assert!(analyze_descriptor(Network::Regtest, &format!("wpkh({k}/<0;1>/*)")).is_err());
    }

    #[test]
    fn analyzes_backup_json() {
        let k = key(ABANDON, "m/84'/1'/0'");
        let backup = WalletBackup::new(
            "Savings".to_string(),
            "regtest".to_string(),
            "singlesig".to_string(),
            format!("wpkh({k}/0/*)"),
            format!("wpkh({k}/1/*)"),
            vec![WalletBackupSlot {
                label: "Laptop key".to_string(),
                fingerprint: Some("73c5da0a".to_string()),
                derivation_path: Some("m/84'/1'/0'".to_string()),
                xpub: k.split(']').nth(1).unwrap().to_string(),
                source: "generated".to_string(),
            }],
            0,
        );
        let text = serde_json::to_string(&backup).unwrap();
        let a = analyze_descriptor(Network::Regtest, &text).unwrap();
        assert_eq!(a.name_hint.as_deref(), Some("Savings"));
        assert_eq!(a.slots[0].label.as_deref(), Some("Laptop key"));
    }
}
