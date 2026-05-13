//! On-disk layout + passphrase encryption for local wallets.
//!
//! Directory layout under the app data dir (`~/Library/Application
//! Support/sigvault-desktop/local/<wallet_id>/` on macOS, equivalent on
//! Linux / Windows):
//!
//! ```text
//! local/
//!   <wallet_id>/
//!     metadata.json   # name, network, policy_type, descriptor pair, …
//!     descriptor.json # full WalletShape (re-serialisable via policy-core)
//!     seed.enc        # Argon2id+ChaCha20-Poly1305 envelope around the
//!                     # BIP39 mnemonic / xprv. Absent for watch-only and
//!                     # hardware-only wallets — its absence IS the
//!                     # signal that no hot key material is held.
//!     bdk_store       # bdk_file_store ChangeSet log (BDK magic header)
//! ```
//!
//! `seed.enc` envelope format (JSON):
//!
//! ```json
//! { "salt_b64": "...", "nonce_b64": "...", "ciphertext_b64": "..." }
//! ```
//!
//! The salt is 16 random bytes (per-wallet, written exactly once). The
//! nonce is 12 random bytes. Wrong passphrase ⇒ different KDF output ⇒
//! ChaCha20-Poly1305 auth tag mismatch ⇒ `DecryptError::AuthFailed` and
//! the file is left untouched.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as base64_std;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::kdf;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

/// Stable identifier for a local wallet on disk. Generated as a UUID v4
/// at wallet-create time so two wallets created on different installs
/// can never clash if they're later merged into the same data dir.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WalletId(String);

impl WalletId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for WalletId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WalletId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for WalletId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for WalletId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for WalletId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Resolves filesystem paths for a wallet's sidecars relative to a root
/// `local/` directory.
#[derive(Debug, Clone)]
pub struct WalletDirLayout {
    root: PathBuf,
}

impl WalletDirLayout {
    pub fn for_wallet(local_root: impl Into<PathBuf>, id: &WalletId) -> Self {
        Self {
            root: local_root.into().join(id.as_str()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.root.join("metadata.json")
    }

    pub fn descriptor_path(&self) -> PathBuf {
        self.root.join("descriptor.json")
    }

    pub fn seed_path(&self) -> PathBuf {
        self.root.join("seed.enc")
    }

    pub fn bdk_store_path(&self) -> PathBuf {
        self.root.join("bdk_store")
    }

    /// Create the wallet directory if it doesn't exist. Idempotent.
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)
    }

    /// Recursively delete the wallet directory and all its sidecars.
    /// Caller is responsible for any passphrase-confirmation flow.
    pub fn delete(&self) -> std::io::Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SeedStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("malformed seed envelope: {0}")]
    Malformed(String),
    #[error("decryption failed (wrong passphrase or tampered file)")]
    AuthFailed,
    #[error("encryption failed: {0}")]
    EncryptFailed(String),
    #[error("rng error: {0}")]
    Rng(String),
}

/// Persisted form of an encrypted seed. Serialised as JSON to `seed.enc`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSeed {
    pub salt_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

impl EncryptedSeed {
    fn salt(&self) -> Result<[u8; SALT_LEN], SeedStoreError> {
        let bytes = base64_std.decode(self.salt_b64.as_bytes())?;
        if bytes.len() != SALT_LEN {
            return Err(SeedStoreError::Malformed(format!(
                "expected {SALT_LEN}-byte salt, got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; SALT_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    fn nonce(&self) -> Result<[u8; NONCE_LEN], SeedStoreError> {
        let bytes = base64_std.decode(self.nonce_b64.as_bytes())?;
        if bytes.len() != NONCE_LEN {
            return Err(SeedStoreError::Malformed(format!(
                "expected {NONCE_LEN}-byte nonce, got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; NONCE_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    fn ciphertext(&self) -> Result<Vec<u8>, SeedStoreError> {
        Ok(base64_std.decode(self.ciphertext_b64.as_bytes())?)
    }
}

/// Encrypt raw seed bytes (BIP39 mnemonic UTF-8 string or xprv bytes —
/// caller's choice) under the user's passphrase. Generates a fresh random
/// salt and nonce; both are stored alongside the ciphertext in the
/// returned envelope so decryption is fully self-contained.
pub fn encrypt_seed(seed: &[u8], passphrase: &[u8]) -> Result<EncryptedSeed, SeedStoreError> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::getrandom(&mut salt).map_err(|e| SeedStoreError::Rng(e.to_string()))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| SeedStoreError::Rng(e.to_string()))?;

    let key_bytes = kdf::derive_passphrase_key(passphrase, &salt);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), seed)
        .map_err(|e| SeedStoreError::EncryptFailed(e.to_string()))?;

    Ok(EncryptedSeed {
        salt_b64: base64_std.encode(salt),
        nonce_b64: base64_std.encode(nonce_bytes),
        ciphertext_b64: base64_std.encode(&ciphertext),
    })
}

/// Decrypt a seed envelope under a passphrase. Wrong passphrase returns
/// `SeedStoreError::AuthFailed`; the caller must NOT mutate the
/// underlying file on this error — the user just typed the wrong thing.
pub fn decrypt_seed(env: &EncryptedSeed, passphrase: &[u8]) -> Result<Vec<u8>, SeedStoreError> {
    let salt = env.salt()?;
    let nonce = env.nonce()?;
    let ciphertext = env.ciphertext()?;

    let key_bytes = kdf::derive_passphrase_key(passphrase, &salt);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
        .map_err(|_| SeedStoreError::AuthFailed)
}

/// The plaintext payload `seed.enc` decrypts to. Always JSON-encoded
/// before being handed to `encrypt_seed` so we can add fields later
/// (or in this case: bundle the BIP39 passphrase alongside the
/// mnemonic) without changing the on-disk envelope format.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SeedPayload {
    pub mnemonic: String,
    /// Optional BIP39 passphrase used to derive the seed from the
    /// mnemonic. Cryptographically equivalent to a 25th word; the
    /// resulting wallet is different from the same mnemonic with an
    /// empty passphrase. Stored encrypted so signing later doesn't
    /// have to re-prompt the user.
    #[serde(default)]
    pub bip39_passphrase: String,
}

/// Encrypt and write a `SeedPayload` to `seed.enc`. JSON-serializes the
/// payload first so future fields can be added without breaking the
/// on-disk envelope shape.
pub fn write_seed_file(
    layout: &WalletDirLayout,
    payload: &SeedPayload,
    passphrase: &[u8],
) -> Result<(), SeedStoreError> {
    layout.ensure_dir()?;
    let plaintext = serde_json::to_vec(payload)?;
    let env = encrypt_seed(&plaintext, passphrase)?;
    let json = serde_json::to_vec_pretty(&env)?;
    fs::write(layout.seed_path(), json)?;
    Ok(())
}

/// Read + decrypt `seed.enc`, then parse the plaintext as a JSON
/// `SeedPayload`. Returns `Ok(None)` when the file is absent
/// (watch-only / hardware-only wallets), `Err(AuthFailed)` on wrong
/// passphrase, `Err(Malformed)` if the plaintext isn't a valid
/// payload.
pub fn read_seed_file(
    layout: &WalletDirLayout,
    passphrase: &[u8],
) -> Result<Option<SeedPayload>, SeedStoreError> {
    let path = layout.seed_path();
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    let env: EncryptedSeed = serde_json::from_slice(&bytes)?;
    let plaintext = decrypt_seed(&env, passphrase)?;
    let payload: SeedPayload = serde_json::from_slice(&plaintext)
        .map_err(|e| SeedStoreError::Malformed(format!("seed payload: {e}")))?;
    Ok(Some(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const SEED: &[u8] = b"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn round_trip_correct_passphrase() {
        let env = encrypt_seed(SEED, b"correct horse battery staple").expect("encrypt");
        let plaintext = decrypt_seed(&env, b"correct horse battery staple").expect("decrypt");
        assert_eq!(plaintext, SEED);
    }

    #[test]
    fn wrong_passphrase_fails_with_auth_failed() {
        let env = encrypt_seed(SEED, b"right").expect("encrypt");
        match decrypt_seed(&env, b"wrong") {
            Err(SeedStoreError::AuthFailed) => {}
            Err(other) => panic!("expected AuthFailed, got {other:?}"),
            Ok(_) => panic!("decrypt should not succeed with wrong passphrase"),
        }
    }

    #[test]
    fn distinct_salts_per_envelope() {
        let a = encrypt_seed(SEED, b"pass").expect("a");
        let b = encrypt_seed(SEED, b"pass").expect("b");
        assert_ne!(a.salt_b64, b.salt_b64, "salt must be random per envelope");
        assert_ne!(
            a.nonce_b64, b.nonce_b64,
            "nonce must be random per envelope"
        );
        assert_ne!(
            a.ciphertext_b64, b.ciphertext_b64,
            "ciphertext must differ when nonce + salt do"
        );
    }

    #[test]
    fn disk_round_trip_writes_and_reads() {
        let tmp = TempDir::new().expect("tempdir");
        let id = WalletId::new();
        let layout = WalletDirLayout::for_wallet(tmp.path().to_path_buf(), &id);

        let payload = SeedPayload {
            mnemonic: String::from_utf8(SEED.to_vec()).unwrap(),
            bip39_passphrase: String::new(),
        };
        write_seed_file(&layout, &payload, b"unlock").expect("write");
        assert!(layout.seed_path().exists());
        assert!(!layout.metadata_path().exists(), "only seed file written");

        let loaded = read_seed_file(&layout, b"unlock").expect("read");
        let loaded = loaded.expect("payload");
        assert_eq!(loaded.mnemonic.as_bytes(), SEED);
        assert_eq!(loaded.bip39_passphrase, "");
    }

    #[test]
    fn seed_payload_round_trips_bip39_passphrase() {
        let tmp = TempDir::new().expect("tempdir");
        let id = WalletId::new();
        let layout = WalletDirLayout::for_wallet(tmp.path().to_path_buf(), &id);

        let payload = SeedPayload {
            mnemonic: String::from_utf8(SEED.to_vec()).unwrap(),
            bip39_passphrase: "my-25th-word".to_string(),
        };
        write_seed_file(&layout, &payload, b"unlock").expect("write");
        let loaded = read_seed_file(&layout, b"unlock").expect("read").unwrap();
        assert_eq!(loaded.bip39_passphrase, "my-25th-word");
    }

    #[test]
    fn read_seed_file_returns_none_when_absent() {
        let tmp = TempDir::new().expect("tempdir");
        let id = WalletId::new();
        let layout = WalletDirLayout::for_wallet(tmp.path().to_path_buf(), &id);
        layout.ensure_dir().expect("mkdir");

        let loaded = read_seed_file(&layout, b"anything").expect("read");
        assert!(loaded.is_none());
    }

    #[test]
    fn wallet_id_unique_per_call() {
        let a = WalletId::new();
        let b = WalletId::new();
        assert_ne!(a, b);
        assert_eq!(a.as_str().len(), 36, "uuid v4 string is 36 chars");
    }

    #[test]
    fn delete_removes_directory() {
        let tmp = TempDir::new().expect("tempdir");
        let id = WalletId::new();
        let layout = WalletDirLayout::for_wallet(tmp.path().to_path_buf(), &id);
        let payload = SeedPayload {
            mnemonic: String::from_utf8(SEED.to_vec()).unwrap(),
            bip39_passphrase: String::new(),
        };
        write_seed_file(&layout, &payload, b"x").expect("write");
        assert!(layout.root().exists());

        layout.delete().expect("delete");
        assert!(!layout.root().exists());
    }

    /// Loose perf sanity check: a single Argon2id derivation at our
    /// production parameters should land well under 1 s on developer
    /// hardware. We assert <1500ms to leave headroom for slower CI
    /// runners; if this trips, recheck Params before relaxing the bound.
    #[test]
    fn passphrase_derivation_under_perf_budget() {
        let start = std::time::Instant::now();
        let _ = kdf::derive_passphrase_key(b"some passphrase", &[7u8; 16]);
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "passphrase KDF took {:?}, expected <1500ms",
            elapsed
        );
    }
}
