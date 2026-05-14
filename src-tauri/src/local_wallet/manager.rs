//! High-level local-wallet operations.
//!
//! This is the surface the Tauri command layer (and the integration
//! tests) drive. It binds together storage (QBL-215), the BDK persister
//! (QBL-216), wallet-runtime, and policy-core key generation. The manager
//! owns the local-root path and the in-memory unlocked-state map; it
//! does NOT own a Tauri AppHandle — the command layer reaches into the
//! AppHandle for the data dir and passes a `LocalWalletManager` already
//! configured for that dir.
//!
//! Wallet-creation surface (one method per flavour):
//! - `create_singlesig_hot` — hot mnemonic + passphrase (QBL-216)
//! - `create_singlesig_hardware` — HW xpub (QBL-220)
//! - `create_multisig` — N HW or pasted-xpub cosigners (QBL-224)
//! - `create_watch_only` — descriptor paste / xpub import (QBL-226)
//! - `create_liana` — Liana timelocked-policy, HW + pasted xpub (QBL-225)
//!
//! Sync / PSBT / HW signing are wired in QBL-218–QBL-220.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bdk_wallet::bitcoin::bip32::Xpriv;
use bdk_wallet::bitcoin::Network;
use bdk_wallet::keys::bip39::{Language, Mnemonic};
use bdk_wallet::miniscript::descriptor::DescriptorPublicKey;
use bdk_wallet::KeychainKind;
use policy_core::{
    build_descriptor, unspendable_primary_xpub, KeyUtils, PolicyPath as CorePolicyPath,
    RecoveryPath as CoreRecoveryPath, WalletShape,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wallet_runtime::{
    create_wallet as wr_create_wallet, load_wallet as wr_load_wallet, WalletDescriptors,
};
use zeroize::Zeroizing;

use super::persister::{LocalBdkPersister, LocalPersisterError};
use super::state::{SharedLocalWalletState, UnlockedHandle};
use super::storage::{
    read_seed_file, write_seed_file, SeedPayload, SeedStoreError, WalletDirLayout, WalletId,
};

const POLICY_TYPE_SINGLESIG: &str = "singlesig";
const POLICY_TYPE_SINGLESIG_HW: &str = "singlesig_hardware";
const POLICY_TYPE_WATCH_ONLY: &str = "watch_only";
const POLICY_TYPE_MULTISIG: &str = "multisig";
const POLICY_TYPE_DESCRIPTOR: &str = "descriptor";
const POLICY_TYPE_LIANA: &str = "liana";

/// A cosigner contribution to a multisig wallet (QBL-224). `key` is the
/// descriptor key expression up through the xpub — `[fp/path]xpub` for
/// HW-collected or origin-tagged paste inputs, or bare `xpub` for paste
/// inputs without origin info. The descriptor builder appends `/0/*` or
/// `/1/*` for the external / internal keychains.
#[derive(Debug, Clone)]
pub struct MultisigCosigner {
    pub key: String,
    pub fingerprint: Option<String>,
}

/// One key contribution to a Liana spending path (QBL-225). `xpub` is the
/// public key at `derivation_path`; `fingerprint` is the master key
/// fingerprint. Same shape as the inputs we collect for HW singlesig +
/// multisig — the wizard reuses the existing `cmd_get_device_xpub` flow,
/// then hands the result here.
#[derive(Debug, Clone)]
pub struct LianaKeyInput {
    pub fingerprint: String,
    pub xpub: String,
    pub derivation_path: String,
}

/// One spending path inside a Liana wallet. `threshold == 1` and
/// `keys.len() == 1` is the singlesig case; larger thresholds are
/// M-of-N inside the path.
#[derive(Debug, Clone)]
pub struct LianaSpendingPath {
    pub keys: Vec<LianaKeyInput>,
    pub threshold: u32,
}

/// One recovery branch with its block-count timelock.
#[derive(Debug, Clone)]
pub struct LianaRecoveryPath {
    pub timelock_blocks: u16,
    pub path: LianaSpendingPath,
}

/// How the primary spending path is satisfied. `Keys` is the standard
/// case (one or more user-controlled HW keys + threshold). `Unspendable`
/// (QBL-235) tags the primary as recovery-only — we substitute a
/// deterministic NUMS-derived xpub so only the timelocked recovery
/// path(s) can ever spend. Mirrors Liana Desktop's "no primary key"
/// affordance for cold-storage / inheritance setups.
#[derive(Debug, Clone)]
pub enum LianaPrimary {
    Keys(LianaSpendingPath),
    Unspendable,
}

/// What's persisted in `metadata.json` for a local wallet. Everything
/// here is recoverable from disk without a passphrase — names, public
/// descriptor strings, fingerprints. Nothing here is sensitive enough to
/// require encryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalWalletMetadata {
    pub id: WalletId,
    pub name: String,
    pub network: String,
    pub policy_type: String,
    pub external_descriptor: String,
    pub internal_descriptor: String,
    pub fingerprints: Vec<String>,
    pub has_hot_keys: bool,
    pub created_at: i64,
    /// Liana wallets only — `true` when the primary spending path was
    /// created with QBL-235's "unspendable primary" option, meaning
    /// only the timelocked recovery path(s) can ever sign. The
    /// dashboard surfaces a "Recovery-only" badge based on this flag.
    /// `#[serde(default)]` keeps wallets created before QBL-235
    /// readable (they all default to `false` = spendable primary).
    #[serde(default)]
    pub recovery_only: bool,
    /// QBL-230 — derivation path the stored mnemonic was sliced at to
    /// produce the wallet's hot xprv. Empty / `None` falls back to the
    /// network's default singlesig path (`m/84'/coin'/0'`), which is
    /// what every wallet created before QBL-230 used. Cosigner-recovered
    /// wallets store the matching key's origin path from the supplied
    /// descriptor here so signing derives the right xprv.
    #[serde(default)]
    pub derivation_path: Option<String>,
}

/// Lightweight view returned by `list_wallets` for the wallet list UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSummary {
    pub id: WalletId,
    pub name: String,
    pub network: String,
    pub policy_type: String,
    pub fingerprints: Vec<String>,
    pub has_hot_keys: bool,
    pub created_at: i64,
    pub locked: bool,
    #[serde(default)]
    pub recovery_only: bool,
}

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("seed store: {0}")]
    Seed(#[from] SeedStoreError),
    #[error("persister: {0}")]
    Persister(#[from] LocalPersisterError),
    #[error("wallet runtime: {0}")]
    Runtime(String),
    #[error("wallet not found: {0}")]
    NotFound(String),
    #[error("wallet '{0}' is already unlocked")]
    AlreadyUnlocked(String),
    #[error("wallet '{0}' is not unlocked")]
    NotUnlocked(String),
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("unsupported network: {0}")]
    UnsupportedNetwork(String),
    #[error("unsupported policy type: {0}")]
    UnsupportedPolicy(String),
}

pub struct LocalWalletManager {
    local_root: PathBuf,
    state: SharedLocalWalletState,
}

impl LocalWalletManager {
    pub fn new(app_data_dir: PathBuf, state: SharedLocalWalletState) -> Self {
        Self {
            local_root: app_data_dir.join("local"),
            state,
        }
    }

    /// Used by tests + commands that already have the shared state arc.
    pub fn state(&self) -> &SharedLocalWalletState {
        &self.state
    }

    pub fn local_root(&self) -> &Path {
        &self.local_root
    }

    fn layout(&self, id: &WalletId) -> WalletDirLayout {
        WalletDirLayout::for_wallet(self.local_root.clone(), id)
    }

    fn write_metadata(
        &self,
        layout: &WalletDirLayout,
        meta: &LocalWalletMetadata,
    ) -> Result<(), ManagerError> {
        layout.ensure_dir()?;
        let json = serde_json::to_vec_pretty(meta)?;
        fs::write(layout.metadata_path(), json)?;
        Ok(())
    }

    pub fn read_metadata(&self, id: &WalletId) -> Result<LocalWalletMetadata, ManagerError> {
        let layout = self.layout(id);
        if !layout.metadata_path().exists() {
            return Err(ManagerError::NotFound(id.to_string()));
        }
        let bytes = fs::read(layout.metadata_path())?;
        let meta: LocalWalletMetadata = serde_json::from_slice(&bytes)?;
        Ok(meta)
    }

    /// Walk the local root and surface one summary per wallet directory
    /// that has a readable metadata.json. Directories with corrupt or
    /// missing metadata are skipped silently (they show up next time the
    /// user inspects them via the dashboard error UI).
    pub async fn list_wallets(&self) -> Result<Vec<WalletSummary>, ManagerError> {
        if !self.local_root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.local_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id_str = entry.file_name().to_string_lossy().into_owned();
            let id_meta_path = entry.path().join("metadata.json");
            if !id_meta_path.exists() {
                continue;
            }
            let bytes = match fs::read(&id_meta_path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let meta: LocalWalletMetadata = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Per-wallet `is_unlocked` lookup; the state's outer mutex
            // is held only briefly each call, so a slow op on one
            // wallet doesn't stall list_wallets.
            let locked = !self.state.is_unlocked(&meta.id).await;
            // Defensive: id in metadata should match dir name; if not,
            // trust the metadata (the dir might have been moved by hand).
            let _ = id_str;
            out.push(WalletSummary {
                id: meta.id,
                name: meta.name,
                network: meta.network,
                policy_type: meta.policy_type,
                fingerprints: meta.fingerprints,
                has_hot_keys: meta.has_hot_keys,
                created_at: meta.created_at,
                locked,
                recovery_only: meta.recovery_only,
            });
        }
        Ok(out)
    }

    /// Generate a fresh BIP39 24-word mnemonic, derive a singlesig segwit-v0
    /// xprv, build the wpkh descriptor pair, encrypt the mnemonic under
    /// the passphrase, and persist. Returns the new wallet's id.
    pub async fn create_singlesig_hot(
        &self,
        name: &str,
        network: Network,
        passphrase: &[u8],
    ) -> Result<(WalletId, Vec<String>), ManagerError> {
        ensure_supported_network(network)?;

        let keyset = KeyUtils::generate_complete_key_set(network);
        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        // The mnemonic is the durable secret. Persist as a SeedPayload
        // — the create flow never asks the user for a BIP39 passphrase,
        // so it's always empty here; recovery is where users opt in.
        let words = keyset.words.clone();
        let mnemonic_str = words.join(" ");
        let payload = SeedPayload {
            mnemonic: mnemonic_str.clone(),
            bip39_passphrase: String::new(),
        };
        write_seed_file(&layout, &payload, passphrase)?;

        let descriptors = WalletDescriptors::new(
            keyset.external_descriptor.clone(),
            keyset.internal_descriptor.clone(),
        );

        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: POLICY_TYPE_SINGLESIG.to_string(),
            external_descriptor: descriptors.external,
            internal_descriptor: descriptors.internal,
            fingerprints: vec![keyset.fingerprint.clone()],
            has_hot_keys: true,
            created_at: now_unix_seconds(),
            recovery_only: false,
            derivation_path: None,
        };
        self.write_metadata(&layout, &meta)?;

        Ok((id, words))
    }

    /// Recover a singlesig segwit-v0 wallet from a BIP39 mnemonic the
    /// user typed in. `bip39_passphrase` is the optional 25th-word
    /// passphrase — empty for the common case of a wallet that was
    /// created without one. Multisig / Liana cosigner recovery goes
    /// through `recover_descriptor_hot_cosigner` instead.
    pub async fn recover_singlesig_hot(
        &self,
        name: &str,
        network: Network,
        mnemonic_str: &str,
        bip39_passphrase: &str,
        encrypt_passphrase: &[u8],
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;

        let account_xpriv = derive_account_at_path(
            network,
            mnemonic_str,
            bip39_passphrase,
            &KeyUtils::get_primary_derivation_path(network).to_string(),
        )?;
        let (external_descriptor, internal_descriptor, _xpub, fingerprint) =
            KeyUtils::get_account_extended_descriptor(account_xpriv);

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        let payload = SeedPayload {
            mnemonic: mnemonic_str.to_string(),
            bip39_passphrase: bip39_passphrase.to_string(),
        };
        write_seed_file(&layout, &payload, encrypt_passphrase)?;

        let descriptors =
            WalletDescriptors::new(external_descriptor.clone(), internal_descriptor.clone());

        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: POLICY_TYPE_SINGLESIG.to_string(),
            external_descriptor,
            internal_descriptor,
            fingerprints: vec![fingerprint],
            has_hot_keys: true,
            created_at: now_unix_seconds(),
            recovery_only: false,
            derivation_path: None,
        };
        self.write_metadata(&layout, &meta)?;

        Ok(id)
    }

    /// Recover a multisig or Liana wallet where one of the cosigner
    /// slots is a hot key (QBL-230). The user supplies their mnemonic +
    /// the wallet's descriptor pair + the matching slot's master
    /// fingerprint. We derive the master xprv from the mnemonic,
    /// confirm the fingerprint actually appears in the descriptor's
    /// origin blocks, and persist the wallet with `has_hot_keys=true`
    /// and `derivation_path` set to whatever the descriptor's origin
    /// block carries for that fingerprint.
    ///
    /// At sign time, `cmd_local_sign_psbt_software` reads
    /// `derivation_path` from metadata and derives the account xprv
    /// at that exact path (e.g. `m/48'/1'/0'/2'` for a BIP48 multisig
    /// slot) rather than the singlesig default.
    ///
    /// `policy_type` must be `"multisig"` or `"liana"` — the descriptor
    /// shape determines which, but the caller already knows from the
    /// recovery wizard step.
    pub async fn recover_descriptor_hot_cosigner(
        &self,
        name: &str,
        network: Network,
        mnemonic_str: &str,
        bip39_passphrase: &str,
        encrypt_passphrase: &[u8],
        external_descriptor: &str,
        internal_descriptor: &str,
        policy_type: &str,
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;
        if policy_type != POLICY_TYPE_MULTISIG && policy_type != POLICY_TYPE_LIANA {
            return Err(ManagerError::Runtime(format!(
                "cosigner recovery requires policy_type 'multisig' or 'liana' (got '{policy_type}')"
            )));
        }

        // Derive the master fingerprint from the mnemonic so we can
        // look up which slot the seed fills in the descriptor.
        let master_xpriv = derive_master_xpriv(network, mnemonic_str, bip39_passphrase)?;
        let secp = bdk_wallet::bitcoin::secp256k1::Secp256k1::new();
        let my_fp = master_xpriv.fingerprint(&secp).to_string();

        // Look up the user's derivation path inside the descriptor's
        // origin blocks. Fingerprint mismatch means the supplied
        // mnemonic doesn't correspond to any slot — surface a clear
        // error so the user knows which seed they need.
        let path =
            extract_origin_path_for_fingerprint(external_descriptor, &my_fp).ok_or_else(|| {
                ManagerError::Runtime(format!(
                    "the supplied mnemonic's master fingerprint ({my_fp}) is not in the descriptor"
                ))
            })?;

        // Sanity-check by actually deriving at that path. If the
        // descriptor's path is malformed, we want to surface the
        // failure now rather than at sign time.
        let _account_xpriv =
            derive_account_at_path(network, mnemonic_str, bip39_passphrase, &path)?;

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        let payload = SeedPayload {
            mnemonic: mnemonic_str.to_string(),
            bip39_passphrase: bip39_passphrase.to_string(),
        };
        write_seed_file(&layout, &payload, encrypt_passphrase)?;

        let descriptors = WalletDescriptors::new(
            external_descriptor.to_string(),
            internal_descriptor.to_string(),
        );
        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        // Surface every fingerprint we can parse out of the descriptor
        // for the wallet-list metadata column. The user's fingerprint
        // is guaranteed to be in there since the lookup succeeded above.
        let mut fingerprints = scan_descriptor_fingerprints(external_descriptor);
        if !fingerprints.iter().any(|f| f.eq_ignore_ascii_case(&my_fp)) {
            fingerprints.push(my_fp.clone());
        }

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: policy_type.to_string(),
            external_descriptor: external_descriptor.to_string(),
            internal_descriptor: internal_descriptor.to_string(),
            fingerprints,
            has_hot_keys: true,
            created_at: now_unix_seconds(),
            recovery_only: false,
            derivation_path: Some(path),
        };
        self.write_metadata(&layout, &meta)?;

        Ok(id)
    }

    /// Create a singlesig segwit-v0 wallet from a hardware device's
    /// xpub. Watch-only as far as on-disk material goes — there is no
    /// `seed.enc` because the device holds the private keys. Sign-time
    /// flows (QBL-219 → QBL-229) re-route through the HW manager
    /// instead of `cmd_local_sign_psbt_software`.
    ///
    /// `derivation_path` is the BIP32 path that produced `xpub` on the
    /// device (typically `m/84'/coin'/0'`). It's embedded in the
    /// descriptor's key origin metadata so PSBTs can be analysed for
    /// what the device needs to sign.
    pub async fn create_singlesig_hardware(
        &self,
        name: &str,
        network: Network,
        fingerprint: &str,
        xpub: &str,
        derivation_path: &str,
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;

        // Strip the leading "m/" / "m" if present — origin syntax in
        // descriptors is `[fingerprint/path]xpub` with no `m` prefix.
        let path = derivation_path
            .strip_prefix("m/")
            .or_else(|| derivation_path.strip_prefix("m"))
            .unwrap_or(derivation_path);

        let external_descriptor = format!("wpkh([{fingerprint}/{path}]{xpub}/0/*)");
        let internal_descriptor = format!("wpkh([{fingerprint}/{path}]{xpub}/1/*)");

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        let descriptors =
            WalletDescriptors::new(external_descriptor.clone(), internal_descriptor.clone());

        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: POLICY_TYPE_SINGLESIG_HW.to_string(),
            external_descriptor,
            internal_descriptor,
            fingerprints: vec![fingerprint.to_string()],
            has_hot_keys: false,
            created_at: now_unix_seconds(),
            recovery_only: false,
            derivation_path: None,
        };
        self.write_metadata(&layout, &meta)?;

        Ok(id)
    }

    /// Create an M-of-N multisig watch-only wallet from a list of
    /// cosigner descriptor keys (QBL-224). The wallet is watch-only on
    /// disk — no `seed.enc` — because the private keys live with each
    /// individual cosigner (HW or external software). Signing happens
    /// per-cosigner via `cmd_local_sign_psbt_hardware` (HW) or by
    /// exporting the PSBT to wherever the hot key lives.
    ///
    /// Builds `wsh(sortedmulti(M, k1/0/*, k2/0/*, ...))` for receive
    /// and `wsh(sortedmulti(M, k1/1/*, k2/1/*, ...))` for change. Uses
    /// `sortedmulti` (BIP67) so cosigner order doesn't matter — the
    /// resulting addresses are deterministic for any permutation.
    pub async fn create_multisig(
        &self,
        name: &str,
        network: Network,
        threshold: u32,
        cosigners: Vec<MultisigCosigner>,
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;

        let n = cosigners.len();
        if n < 2 {
            return Err(ManagerError::Runtime(
                "multisig wallets need at least 2 cosigners".to_string(),
            ));
        }
        if n > 15 {
            return Err(ManagerError::Runtime(format!(
                "multisig of more than 15 cosigners is not supported (got {n})"
            )));
        }
        if threshold < 1 || threshold as usize > n {
            return Err(ManagerError::Runtime(format!(
                "threshold {threshold} out of range for {n} cosigners"
            )));
        }

        let keys_external = cosigners
            .iter()
            .map(|c| format!("{}/0/*", c.key.trim()))
            .collect::<Vec<_>>()
            .join(",");
        let keys_internal = cosigners
            .iter()
            .map(|c| format!("{}/1/*", c.key.trim()))
            .collect::<Vec<_>>()
            .join(",");
        let external_descriptor = format!("wsh(sortedmulti({threshold},{keys_external}))");
        let internal_descriptor = format!("wsh(sortedmulti({threshold},{keys_internal}))");

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        let descriptors =
            WalletDescriptors::new(external_descriptor.clone(), internal_descriptor.clone());

        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        let fingerprints = cosigners
            .iter()
            .filter_map(|c| c.fingerprint.clone())
            .collect();

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: POLICY_TYPE_MULTISIG.to_string(),
            external_descriptor,
            internal_descriptor,
            fingerprints,
            has_hot_keys: false,
            created_at: now_unix_seconds(),
            recovery_only: false,
            derivation_path: None,
        };
        self.write_metadata(&layout, &meta)?;

        Ok(id)
    }

    /// Create a watch-only or descriptor-spendable wallet from descriptor
    /// strings the user provided directly. Watch-only (QBL-226) cannot
    /// sign; descriptor-spendable (QBL-234) is identical on disk but
    /// flagged as eligible for the Send flow — signing happens via HW
    /// or external PSBT export, depending on which devices match the
    /// descriptor's fingerprints at sign time.
    ///
    /// The caller has to hand us both the external (receive) and
    /// internal (change) descriptors — we don't try to auto-derive one
    /// from the other because that's a guess (e.g. a wpkh /0/* receive
    /// doesn't always pair with /1/*; mixed descriptors and Liana
    /// flavours don't follow the convention). BDK rejects descriptors
    /// that contain private keys, which double-guards us against a hot
    /// wallet sneaking in through this path. `fingerprints` is whatever
    /// origin info the caller could parse out of the descriptors —
    /// purely informational, surfaced in the wallet list metadata column.
    pub async fn create_watch_only(
        &self,
        name: &str,
        network: Network,
        external_descriptor: &str,
        internal_descriptor: &str,
        fingerprints: Vec<String>,
        spendable: bool,
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        let descriptors = WalletDescriptors::new(
            external_descriptor.to_string(),
            internal_descriptor.to_string(),
        );

        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        let policy_type = if spendable {
            POLICY_TYPE_DESCRIPTOR
        } else {
            POLICY_TYPE_WATCH_ONLY
        };

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: policy_type.to_string(),
            external_descriptor: external_descriptor.to_string(),
            internal_descriptor: internal_descriptor.to_string(),
            fingerprints,
            has_hot_keys: false,
            created_at: now_unix_seconds(),
            recovery_only: false,
            derivation_path: None,
        };
        self.write_metadata(&layout, &meta)?;

        Ok(id)
    }

    /// Create a Liana timelocked-policy wallet (QBL-225). Always taproot
    /// — policy-core rejects SegwitV0 for timelocked policies — with one
    /// primary spending path that's spendable immediately and one or more
    /// recovery paths that unlock after their respective block-count
    /// timelocks. v1 collects all keys from hardware devices or pasted
    /// xpubs; hot keys for the primary path are deferred to a follow-up
    /// (see QBL-235 for the related "unspendable primary" affordance).
    ///
    /// Descriptor construction goes through `policy_core::build_descriptor`
    /// against a `WalletShape::TimelockedPolicy`, which produces the
    /// canonical Liana descriptor pair. BDK persists from the receive +
    /// change descriptor strings the same way as the other wallet shapes;
    /// at sign time, miniscript's `finalize_mut` selects the right path
    /// based on which keys signed.
    pub async fn create_liana(
        &self,
        name: &str,
        network: Network,
        primary: LianaPrimary,
        recoveries: Vec<LianaRecoveryPath>,
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;

        if recoveries.is_empty() {
            return Err(ManagerError::Runtime(
                "Liana wallets require at least one recovery path".to_string(),
            ));
        }
        for (idx, rec) in recoveries.iter().enumerate() {
            if rec.timelock_blocks == 0 {
                return Err(ManagerError::Runtime(format!(
                    "recovery path {idx} timelock must be greater than zero"
                )));
            }
            validate_liana_path(&rec.path, &format!("recovery {idx}"))?;
        }

        // Resolve the primary path. The `Keys` arm is the user-driven
        // case (existing QBL-225 flow). `Unspendable` (QBL-235)
        // substitutes a deterministic NUMS-derived xpub computed from
        // the recovery key set — same construction Liana Desktop uses
        // so the resulting wallet round-trips through Liana's policy
        // parser as "no primary key".
        let recovery_only = matches!(primary, LianaPrimary::Unspendable);
        let primary_keys: Vec<&LianaKeyInput> = match &primary {
            LianaPrimary::Keys(p) => {
                validate_liana_path(p, "primary")?;
                p.keys.iter().collect()
            }
            LianaPrimary::Unspendable => Vec::new(),
        };
        let primary_path: CorePolicyPath = match &primary {
            LianaPrimary::Keys(p) => build_core_policy_path(p)?,
            LianaPrimary::Unspendable => unspendable_primary_path(network, &recoveries)?,
        };

        let recovery_paths: Vec<CoreRecoveryPath> = recoveries
            .iter()
            .enumerate()
            .map(|(idx, rec)| {
                Ok(CoreRecoveryPath {
                    id: format!("recovery_{idx}"),
                    timelock: rec.timelock_blocks,
                    path: build_core_policy_path(&rec.path)?,
                })
            })
            .collect::<Result<Vec<_>, ManagerError>>()?;

        let shape = WalletShape::TimelockedPolicy {
            primary_id: "primary".to_string(),
            primary: primary_path,
            recoveries: recovery_paths,
        };
        let pair = build_descriptor(&shape)
            .map_err(|e| ManagerError::Runtime(format!("liana descriptor: {e}")))?;

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        let descriptors = WalletDescriptors::new(pair.external.clone(), pair.internal.clone());
        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let _wallet = wr_create_wallet(&mut persister, network, &descriptors)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?;

        // Track fingerprints for the dashboard sidebar. Skip the
        // unspendable-primary's all-zeros fingerprint — it would just
        // be visual noise in the wallet list.
        let mut fingerprints: Vec<String> = Vec::new();
        let mut push_fp = |fp: &str| {
            if !fp.is_empty() && !fingerprints.iter().any(|seen| seen == fp) {
                fingerprints.push(fp.to_string());
            }
        };
        for k in &primary_keys {
            push_fp(&k.fingerprint);
        }
        for rec in &recoveries {
            for k in &rec.path.keys {
                push_fp(&k.fingerprint);
            }
        }

        let meta = LocalWalletMetadata {
            id: id.clone(),
            name: name.to_string(),
            network: network.to_string(),
            policy_type: POLICY_TYPE_LIANA.to_string(),
            external_descriptor: pair.external,
            internal_descriptor: pair.internal,
            fingerprints,
            has_hot_keys: false,
            created_at: now_unix_seconds(),
            recovery_only,
            derivation_path: None,
        };
        self.write_metadata(&layout, &meta)?;

        Ok(id)
    }

    /// Decrypt the wallet's `seed.enc`, reload its BDK state from
    /// `bdk_store`, and put the resulting handle into the in-memory
    /// unlocked map. Idempotent on success only when the wallet was not
    /// already unlocked — re-unlocking returns `AlreadyUnlocked` so
    /// callers don't accidentally double-derive signers.
    pub async fn unlock_wallet(
        &self,
        id: &WalletId,
        passphrase: &[u8],
    ) -> Result<(), ManagerError> {
        if self.state.is_unlocked(id).await {
            return Err(ManagerError::AlreadyUnlocked(id.to_string()));
        }
        let meta = self.read_metadata(id)?;
        let network = parse_network(&meta.network)?;

        let layout = self.layout(id);
        // Hot wallets: decrypt seed.enc with the passphrase so the
        // bytes are available in memory for signing. HW / watch-only
        // wallets have no on-disk seed — we still load the BDK store
        // so balance / address peeking works, and signing routes
        // through the HW manager instead of an in-memory mnemonic.
        let seed_bytes = if meta.has_hot_keys {
            let payload = read_seed_file(&layout, passphrase)?
                .ok_or_else(|| ManagerError::NotFound(format!("seed.enc for {id}")))?;
            // The unlocked handle holds this for Zeroize-on-drop
            // hygiene; the actual signing path re-reads the encrypted
            // file with the user's passphrase, so storing just the
            // mnemonic string here (vs the full payload) keeps the
            // in-memory footprint minimal and avoids leaking the BIP39
            // passphrase into more places than necessary.
            payload.mnemonic.into_bytes()
        } else {
            Vec::new()
        };

        let mut persister = LocalBdkPersister::open_or_create(&layout.bdk_store_path())?;
        let wallet = wr_load_wallet(&mut persister, network)
            .map_err(|e| ManagerError::Runtime(e.to_string()))?
            .ok_or_else(|| ManagerError::NotFound(format!("bdk_store for {id}")))?;

        let handle = UnlockedHandle {
            wallet,
            persister,
            mnemonic: Zeroizing::new(seed_bytes),
        };
        self.state.insert(id.clone(), handle).await;
        Ok(())
    }

    /// Drop the in-memory handle, which Zeroize-wipes the seed bytes.
    /// No-op for already-locked wallets.
    pub async fn lock_wallet(&self, id: &WalletId) -> bool {
        self.state.lock_wallet(id).await
    }

    /// Verify the passphrase by decrypting `seed.enc`, then remove the
    /// wallet directory. For watch-only / hardware-only wallets the
    /// passphrase is ignored (no `seed.enc` exists). Locks first to
    /// release the in-memory handle before removing files.
    pub async fn delete_wallet(
        &self,
        id: &WalletId,
        passphrase: Option<&[u8]>,
    ) -> Result<(), ManagerError> {
        let layout = self.layout(id);
        if !layout.metadata_path().exists() {
            return Err(ManagerError::NotFound(id.to_string()));
        }

        let meta = self.read_metadata(id)?;
        if meta.has_hot_keys {
            let pass = passphrase.ok_or_else(|| {
                ManagerError::Seed(SeedStoreError::Malformed(
                    "passphrase required to delete hot wallet".to_string(),
                ))
            })?;
            // Confirms the user can actually decrypt — same gate as unlock.
            let _ = read_seed_file(&layout, pass)?
                .ok_or_else(|| ManagerError::NotFound(format!("seed.enc for {id}")))?;
        }

        self.state.lock_wallet(id).await;
        layout.delete()?;
        Ok(())
    }

    /// Address peek for a specific keychain + index. Useful for the
    /// receive screen (QBL-228) and for tests; does not advance any
    /// internal counter.
    pub async fn peek_address(
        &self,
        id: &WalletId,
        kind: KeychainKind,
        index: u32,
    ) -> Result<String, ManagerError> {
        let handle_arc = self
            .state
            .get(id)
            .await
            .ok_or_else(|| ManagerError::NotUnlocked(id.to_string()))?;
        let handle = handle_arc.lock().await;
        Ok(wallet_runtime::peek_address(&handle.wallet, kind, index).to_string())
    }
}

/// Parse a BIP39 mnemonic + optional passphrase and derive the master
/// xprv. The master fingerprint (first 4 bytes of hash160 of the
/// master xpub) is what appears in descriptor `[fingerprint/path]`
/// origin blocks, so this is the right entry point for matching a
/// pasted descriptor against a recovery seed.
pub fn derive_master_xpriv(
    network: Network,
    mnemonic_str: &str,
    bip39_passphrase: &str,
) -> Result<Xpriv, ManagerError> {
    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_str)
        .map_err(|e| ManagerError::InvalidMnemonic(e.to_string()))?;
    let seed = mnemonic.to_seed(bip39_passphrase);
    Xpriv::new_master(network, &seed).map_err(|e| ManagerError::InvalidMnemonic(e.to_string()))
}

/// Derive an arbitrary-path account xprv from a mnemonic. `path` is
/// the BIP32 path without the leading `m/` (e.g. `48'/1'/0'/2'`).
/// Cosigner recovery uses this to slice the master xprv at the path
/// the descriptor's origin block requires.
pub fn derive_account_at_path(
    network: Network,
    mnemonic_str: &str,
    bip39_passphrase: &str,
    path: &str,
) -> Result<Xpriv, ManagerError> {
    use std::str::FromStr;
    let master = derive_master_xpriv(network, mnemonic_str, bip39_passphrase)?;
    let secp = bdk_wallet::bitcoin::secp256k1::Secp256k1::new();
    let normalized = path
        .strip_prefix("m/")
        .or_else(|| path.strip_prefix('m'))
        .unwrap_or(path);
    let dp = bdk_wallet::bitcoin::bip32::DerivationPath::from_str(normalized)
        .map_err(|e| ManagerError::Runtime(format!("invalid derivation path '{path}': {e}")))?;
    master
        .derive_priv(&secp, &dp)
        .map_err(|e| ManagerError::Runtime(format!("derive at '{path}': {e}")))
}

/// Scan every origin block in a descriptor and collect the
/// fingerprints. Cosigner recovery uses this to populate the
/// wallet-list metadata column without requiring the user to
/// re-enter each cosigner's fingerprint by hand.
pub fn scan_descriptor_fingerprints(descriptor: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut search_from = 0;
    while let Some(open) = descriptor[search_from..].find('[') {
        let abs_open = search_from + open;
        let after_open = abs_open + 1;
        let close = match descriptor[after_open..].find(']') {
            Some(c) => c,
            None => break,
        };
        let abs_close = after_open + close;
        let inner = &descriptor[after_open..abs_close];
        let fp_part = inner.split('/').next().unwrap_or("");
        if fp_part.len() == 8 && fp_part.chars().all(|c| c.is_ascii_hexdigit()) {
            let lower = fp_part.to_lowercase();
            if !out.iter().any(|seen| seen.eq_ignore_ascii_case(&lower)) {
                out.push(lower);
            }
        }
        search_from = abs_close + 1;
    }
    out
}

/// Scan a descriptor string for an origin block whose fingerprint
/// matches `target_fp` (case-insensitive) and return the derivation
/// path inside. Returns `None` if the fingerprint doesn't appear.
/// Used by cosigner recovery to figure out which slot the user's
/// mnemonic should fill, and at what derivation path.
pub fn extract_origin_path_for_fingerprint(descriptor: &str, target_fp: &str) -> Option<String> {
    let target = target_fp.to_lowercase();
    let lower = descriptor.to_lowercase();
    let mut search_from = 0;
    while let Some(open) = lower[search_from..].find('[') {
        let abs_open = search_from + open;
        let after_open = abs_open + 1;
        let close = lower[after_open..].find(']')?;
        let abs_close = after_open + close;
        let inner = &descriptor[after_open..abs_close];
        let (fp_part, path_part) = match inner.split_once('/') {
            Some((fp, rest)) => (fp, rest),
            None => (inner, ""),
        };
        if fp_part.to_lowercase() == target {
            return Some(path_part.to_string());
        }
        search_from = abs_close + 1;
    }
    None
}

fn ensure_supported_network(network: Network) -> Result<(), ManagerError> {
    // Single source of truth: settings::network_key returns Some for
    // any network the build accepts (which itself consults
    // settings::MAINNET_ENABLED — flip that flag to enable mainnet).
    if super::settings::network_key(network).is_some() {
        Ok(())
    } else {
        Err(ManagerError::UnsupportedNetwork(format!(
            "{network} is not enabled in this build"
        )))
    }
}

fn parse_network(s: &str) -> Result<Network, ManagerError> {
    Network::from_str(s).map_err(|_| ManagerError::UnsupportedNetwork(s.to_string()))
}

fn now_unix_seconds() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn validate_liana_path(path: &LianaSpendingPath, label: &str) -> Result<(), ManagerError> {
    if path.keys.is_empty() {
        return Err(ManagerError::Runtime(format!(
            "{label} path requires at least one key"
        )));
    }
    if path.threshold == 0 || path.threshold as usize > path.keys.len() {
        return Err(ManagerError::Runtime(format!(
            "{label} path threshold {} is out of range for {} keys",
            path.threshold,
            path.keys.len()
        )));
    }
    Ok(())
}

fn build_core_policy_path(path: &LianaSpendingPath) -> Result<CorePolicyPath, ManagerError> {
    let keys: Vec<DescriptorPublicKey> = path
        .keys
        .iter()
        .map(key_input_to_descriptor_public_key)
        .collect::<Result<Vec<_>, _>>()?;
    if keys.len() == 1 && path.threshold == 1 {
        Ok(CorePolicyPath::Single(keys.into_iter().next().unwrap()))
    } else {
        Ok(CorePolicyPath::Multi {
            threshold: path.threshold as usize,
            keys,
        })
    }
}

/// Format an HW-collected (or pasted) xpub into the canonical
/// `[fp/origin]xpub/<0;1>/*` shape that Liana / miniscript expects, then
/// parse it as a `DescriptorPublicKey`. Mirrors how the cloud-mode flow
/// builds Liana descriptors via walletrs.
fn key_input_to_descriptor_public_key(
    k: &LianaKeyInput,
) -> Result<DescriptorPublicKey, ManagerError> {
    let formatted = KeyUtils::format_key_for_liana(&k.fingerprint, &k.derivation_path, &k.xpub);
    DescriptorPublicKey::from_str(&formatted).map_err(|e| {
        ManagerError::Runtime(format!(
            "invalid xpub for fingerprint {}: {}",
            k.fingerprint, e
        ))
    })
}

/// Build the deterministic NUMS-derived primary path for an
/// unspendable-primary Liana wallet (QBL-235). Walks every recovery
/// key in declared order, hashes the concatenated pubkeys to derive
/// the chain code, then formats the resulting xpub as a single-key
/// primary at fingerprint `00000000`.
fn unspendable_primary_path(
    network: Network,
    recoveries: &[LianaRecoveryPath],
) -> Result<CorePolicyPath, ManagerError> {
    let mut recovery_xpubs = Vec::new();
    for rec in recoveries {
        for k in &rec.path.keys {
            let dpk = key_input_to_descriptor_public_key(k)?;
            let xpub = match dpk {
                DescriptorPublicKey::XPub(ref x) => x.xkey,
                DescriptorPublicKey::MultiXPub(ref m) => m.xkey,
                DescriptorPublicKey::Single(_) => {
                    return Err(ManagerError::Runtime(
                        "unspendable-primary wallets require xpub recovery keys (single pubkeys not supported)"
                            .to_string(),
                    ));
                }
            };
            recovery_xpubs.push(xpub);
        }
    }
    let xpub = unspendable_primary_xpub(&recovery_xpubs, network);
    // Format as a Liana key expression. Fingerprint `00000000` and
    // empty origin path is the convention for keys that aren't
    // derived from any wallet — it matches what Liana Desktop emits.
    let formatted = KeyUtils::format_key_for_liana("00000000", "", &xpub.to_string());
    let dpk = DescriptorPublicKey::from_str(&formatted)
        .map_err(|e| ManagerError::Runtime(format!("unspendable primary descriptor key: {e}")))?;
    Ok(CorePolicyPath::Single(dpk))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_wallet::state::LocalWalletState;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn manager_in(tmp: &TempDir) -> LocalWalletManager {
        let state = Arc::new(LocalWalletState::new());
        LocalWalletManager::new(tmp.path().to_path_buf(), state)
    }

    #[tokio::test]
    async fn create_then_list_then_unlock_then_lock_then_delete() {
        let tmp = TempDir::new().unwrap();
        let mgr = manager_in(&tmp);

        // Empty list before create.
        let summaries = mgr.list_wallets().await.expect("list");
        assert!(summaries.is_empty());

        // Create singlesig hot regtest wallet.
        let (id, words) = mgr
            .create_singlesig_hot("My Test Wallet", Network::Regtest, b"passw0rd!")
            .await
            .expect("create");
        assert_eq!(words.len(), 24, "policy-core generates 24 BIP39 words");

        // List shows it as locked.
        let summaries = mgr.list_wallets().await.expect("list 2");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, id);
        assert_eq!(summaries[0].name, "My Test Wallet");
        assert_eq!(summaries[0].network, "regtest");
        assert!(summaries[0].locked);
        assert!(summaries[0].has_hot_keys);

        // Wrong passphrase fails with AuthFailed-flavored seed error.
        match mgr.unlock_wallet(&id, b"WRONG").await {
            Err(ManagerError::Seed(SeedStoreError::AuthFailed)) => {}
            other => panic!("expected AuthFailed, got {other:?}"),
        }

        // Right passphrase unlocks and exposes a peekable address.
        mgr.unlock_wallet(&id, b"passw0rd!").await.expect("unlock");
        let summaries = mgr.list_wallets().await.expect("list 3");
        assert!(!summaries[0].locked);

        let addr = mgr
            .peek_address(&id, KeychainKind::External, 0)
            .await
            .expect("peek");
        assert!(addr.starts_with("bcrt1q"), "regtest p2wpkh address: {addr}");

        // Lock zeroizes the seed and locks list view back.
        let was_unlocked = mgr.lock_wallet(&id).await;
        assert!(was_unlocked);
        let summaries = mgr.list_wallets().await.expect("list 4");
        assert!(summaries[0].locked);

        // Delete needs the passphrase for hot wallets.
        match mgr.delete_wallet(&id, Some(b"WRONG")).await {
            Err(ManagerError::Seed(SeedStoreError::AuthFailed)) => {}
            other => panic!("expected AuthFailed on wrong pass delete, got {other:?}"),
        }
        mgr.delete_wallet(&id, Some(b"passw0rd!"))
            .await
            .expect("delete");

        let summaries = mgr.list_wallets().await.expect("list 5");
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn cannot_double_unlock() {
        let tmp = TempDir::new().unwrap();
        let mgr = manager_in(&tmp);
        let (id, _) = mgr
            .create_singlesig_hot("dup", Network::Regtest, b"x")
            .await
            .unwrap();
        mgr.unlock_wallet(&id, b"x").await.expect("first unlock");
        match mgr.unlock_wallet(&id, b"x").await {
            Err(ManagerError::AlreadyUnlocked(_)) => {}
            other => panic!("expected AlreadyUnlocked, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mainnet_is_refused() {
        let tmp = TempDir::new().unwrap();
        let mgr = manager_in(&tmp);
        let result = mgr.create_singlesig_hot("mn", Network::Bitcoin, b"x").await;
        assert!(matches!(result, Err(ManagerError::UnsupportedNetwork(_))));
    }

    #[tokio::test]
    async fn recover_round_trip_matches_known_mnemonic() {
        let tmp = TempDir::new().unwrap();
        let mgr = manager_in(&tmp);
        // BIP39 test vector (12 words). policy-core's KeyUtils generates 24 words
        // but recover should accept any valid BIP39 length.
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let id = mgr
            .recover_singlesig_hot("recovered", Network::Regtest, mnemonic, "", b"hunter2")
            .await
            .expect("recover");
        mgr.unlock_wallet(&id, b"hunter2")
            .await
            .expect("unlock recovered");
        let addr = mgr
            .peek_address(&id, KeychainKind::External, 0)
            .await
            .expect("peek");
        assert!(addr.starts_with("bcrt1q"));
    }

    #[test]
    fn extract_origin_path_finds_target_fingerprint() {
        let desc = "wsh(sortedmulti(2,[abcd1234/48'/1'/0'/2']xpub1.../0/*,[deadbeef/48'/1'/0'/2']xpub2.../0/*))";
        let path = extract_origin_path_for_fingerprint(desc, "abcd1234");
        assert_eq!(path.as_deref(), Some("48'/1'/0'/2'"));
        let path_case_insensitive = extract_origin_path_for_fingerprint(desc, "DEADBEEF");
        assert_eq!(path_case_insensitive.as_deref(), Some("48'/1'/0'/2'"));
        // Fingerprint not in descriptor → None.
        let missing = extract_origin_path_for_fingerprint(desc, "00000000");
        assert!(missing.is_none());
    }

    #[test]
    fn scan_descriptor_fingerprints_collects_unique_origins() {
        let desc = "wsh(sortedmulti(2,[abcd1234/48'/1'/0'/2']xpub.../0/*,[DEADBEEF/48'/1'/0'/2']xpub.../0/*,[abcd1234/48'/1'/0'/2']xpub.../0/*))";
        let fps = scan_descriptor_fingerprints(desc);
        // Two distinct fingerprints; the repeat should be deduplicated.
        assert_eq!(fps.len(), 2);
        assert!(fps.contains(&"abcd1234".to_string()));
        assert!(fps.contains(&"deadbeef".to_string()));
    }
}
