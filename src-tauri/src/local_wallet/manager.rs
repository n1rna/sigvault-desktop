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
//! Scope for QBL-216:
//! - singlesig hot wallets (segwit v0) end-to-end
//! - list / unlock / lock / delete
//! - `recover_from_mnemonic` shares the create code path
//!
//! Multisig (QBL-224), Liana (QBL-225), and watch-only (QBL-226) flows
//! land later. Sync / PSBT / HW signing are wired in QBL-218–QBL-220.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bdk_wallet::bitcoin::bip32::{Fingerprint, Xpriv};
use bdk_wallet::bitcoin::Network;
use bdk_wallet::keys::bip39::{Language, Mnemonic};
use bdk_wallet::KeychainKind;
use policy_core::KeyUtils;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wallet_runtime::{
    create_wallet as wr_create_wallet, load_wallet as wr_load_wallet, WalletDescriptors,
};
use zeroize::Zeroizing;

use super::persister::{LocalBdkPersister, LocalPersisterError};
use super::state::{SharedLocalWalletState, UnlockedHandle};
use super::storage::{read_seed_file, write_seed_file, SeedStoreError, WalletDirLayout, WalletId};

const POLICY_TYPE_SINGLESIG: &str = "singlesig";
const POLICY_TYPE_SINGLESIG_HW: &str = "singlesig_hardware";

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

        // The mnemonic is the durable secret. Persist as an encrypted
        // UTF-8 string of space-separated words; on recovery we re-derive
        // the xprv from it via Mnemonic::parse + to_seed.
        let words = keyset.words.clone();
        let mnemonic_str = words.join(" ");
        write_seed_file(&layout, mnemonic_str.as_bytes(), passphrase)?;

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
        };
        self.write_metadata(&layout, &meta)?;

        Ok((id, words))
    }

    /// Recover from a BIP39 mnemonic the user typed in. v1 only handles
    /// the singlesig segwit-v0 case (mirrors `create_singlesig_hot`); a
    /// later ticket will accept a descriptor for multisig / Liana hot
    /// cosigner recovery.
    pub async fn recover_singlesig_hot(
        &self,
        name: &str,
        network: Network,
        mnemonic_str: &str,
        passphrase: &[u8],
    ) -> Result<WalletId, ManagerError> {
        ensure_supported_network(network)?;

        let (account_xpriv, _) = derive_account_from_mnemonic(network, mnemonic_str)?;
        let (external_descriptor, internal_descriptor, _xpub, fingerprint) =
            KeyUtils::get_account_extended_descriptor(account_xpriv);

        let id = WalletId::new();
        let layout = self.layout(&id);
        layout.ensure_dir()?;

        write_seed_file(&layout, mnemonic_str.as_bytes(), passphrase)?;

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
            read_seed_file(&layout, passphrase)?
                .ok_or_else(|| ManagerError::NotFound(format!("seed.enc for {id}")))?
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

/// Parse a BIP39 mnemonic and derive the singlesig segwit-v0 account
/// xprv (`m/84'/{coin}'/0'`) along with its fingerprint. Shared by
/// `recover_singlesig_hot` (wallet creation) and `cmd_local_sign_psbt_*`
/// (signing — re-derives the xprv from the just-decrypted seed rather
/// than persisting it in memory across the unlock session).
pub fn derive_account_from_mnemonic(
    network: Network,
    mnemonic_str: &str,
) -> Result<(Xpriv, Fingerprint), ManagerError> {
    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_str)
        .map_err(|e| ManagerError::InvalidMnemonic(e.to_string()))?;
    let seed = mnemonic.to_seed("");
    let master_xpriv = Xpriv::new_master(network, &seed)
        .map_err(|e| ManagerError::InvalidMnemonic(e.to_string()))?;
    let secp = bdk_wallet::bitcoin::secp256k1::Secp256k1::new();
    let account_path = KeyUtils::get_primary_derivation_path(network);
    let account_xpriv = master_xpriv
        .derive_priv(&secp, &account_path)
        .map_err(|e| ManagerError::InvalidMnemonic(e.to_string()))?;
    let fingerprint = account_xpriv.fingerprint(&secp);
    Ok((account_xpriv, fingerprint))
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
            .recover_singlesig_hot("recovered", Network::Regtest, mnemonic, b"hunter2")
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
}
