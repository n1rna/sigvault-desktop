//! Key library ("Keys" section of local mode).
//!
//! Keys are added once — generated, imported from a recovery phrase / xprv /
//! key file, read from a hardware wallet, or pasted as an xpub — and then
//! picked when creating or restoring wallets. This decouples key management
//! from wallet creation.
//!
//! Storage: `<data dir>/local/keychain.json`. Public material (label,
//! fingerprint, derivation path, xpub) is stored in the clear, exactly like a
//! wallet's `metadata.json`. Private material (`HotSecret`) is encrypted with
//! the core's seed envelope (`local_wallet::storage::encrypt_seed`, Argon2id +
//! ChaCha20-Poly1305) under a single *keychain password*, which the user
//! unlocks once per session. A verifier envelope lets us check the password
//! before touching any key.
//!
//! When a wallet is created from library keys, the secrets are decrypted and
//! handed to `cmd_local_create_from_slots`, which re-encrypts them into the
//! wallet's own seed store under the wallet passphrase — so wallets never
//! depend on the keychain after creation.

use std::path::PathBuf;

use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};
use sigvault_core::local_wallet::storage::{decrypt_seed, encrypt_seed, EncryptedSeed, HotSecret};
use zeroize::Zeroizing;

use crate::runtime::backend;

const FILE_VERSION: u32 = 1;
const VERIFIER_PLAINTEXT: &[u8] = b"sigvault-keychain-v1";

/// Where a key came from. Mirrors the `source` strings of the core's
/// `SlotInput` so a record maps 1:1 onto a wallet slot.
pub const SOURCES: &[(&str, &str)] = &[
    ("generated", "Generated"),
    ("seed", "Recovery phrase"),
    ("xprv", "Extended private key"),
    ("hardware", "Hardware wallet"),
    ("xpub", "Extended public key"),
    ("key_file", "Key file"),
];

pub fn source_label(source: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|(s, _)| *s == source)
        .map(|(_, l)| *l)
        .unwrap_or("Key")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRecord {
    pub id: String,
    pub label: String,
    /// `regtest` | `signet` | `testnet4` | `testnet` | `bitcoin`
    pub network: String,
    /// See [`SOURCES`].
    pub source: String,
    pub fingerprint: String,
    /// Account path, e.g. `m/48'/1'/0'/2'`.
    pub derivation_path: String,
    pub xpub: String,
    /// Hardware model (`Trezor`, `Ledger`, …) for `hardware` keys.
    #[serde(default)]
    pub device: Option<String>,
    pub created_at: i64,
    /// `HotSecret` JSON encrypted under the keychain password.
    #[serde(default)]
    pub secret: Option<EncryptedSeed>,
}

impl KeyRecord {
    /// True when the private key is stored on this computer.
    pub fn is_hot(&self) -> bool {
        self.secret.is_some()
    }

    /// Output-descriptor key expression `[fp/path]xpub`.
    pub fn key_expr(&self) -> String {
        let path = self.derivation_path.trim_start_matches("m/").trim_start_matches('m');
        if path.is_empty() {
            format!("[{}]{}", self.fingerprint, self.xpub)
        } else {
            format!("[{}/{}]{}", self.fingerprint, path, self.xpub)
        }
    }
}

/// Public fields for a new key; the store fills `id`, `created_at`, `secret`.
#[derive(Debug, Clone)]
pub struct NewKey {
    pub label: String,
    pub network: String,
    pub source: String,
    pub fingerprint: String,
    pub derivation_path: String,
    pub xpub: String,
    pub device: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct KeychainFile {
    version: u32,
    #[serde(default)]
    verifier: Option<EncryptedSeed>,
    #[serde(default)]
    keys: Vec<KeyRecord>,
}

#[derive(Debug, Clone)]
pub enum KeychainEvent {
    Changed,
    LockChanged,
}

pub struct Keychain {
    path: PathBuf,
    file: KeychainFile,
    password: Option<Zeroizing<String>>,
    load_error: Option<String>,
}

impl EventEmitter<KeychainEvent> for Keychain {}

struct KeychainHandle(Entity<Keychain>);
impl Global for KeychainHandle {}

pub fn init(cx: &mut App) {
    let dir = backend(cx)
        .app
        .path()
        .app_data_dir()
        .map(|d| d.join("local"))
        .unwrap_or_else(|_| PathBuf::from("local"));
    let kc = cx.new(|_| Keychain::load(dir.join("keychain.json")));
    cx.set_global(KeychainHandle(kc));
}

/// The global keychain entity.
pub fn keychain(cx: &App) -> Entity<Keychain> {
    cx.global::<KeychainHandle>().0.clone()
}

impl Keychain {
    fn load(path: PathBuf) -> Self {
        let (file, load_error) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<KeychainFile>(&bytes) {
                Ok(f) => (f, None),
                Err(e) => (KeychainFile::default(), Some(format!("keychain.json is unreadable: {e}"))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (KeychainFile::default(), None),
            Err(e) => (KeychainFile::default(), Some(format!("cannot read keychain: {e}"))),
        };
        Self { path, file, password: None, load_error }
    }

    // ── queries ──

    pub fn keys(&self) -> &[KeyRecord] {
        &self.file.keys
    }

    pub fn get(&self, id: &str) -> Option<&KeyRecord> {
        self.file.keys.iter().find(|k| k.id == id)
    }

    /// Keys usable for a wallet on `network` (testnet/testnet4/signet/regtest
    /// share tpub encoding but not coin-type paths, so match exactly).
    pub fn keys_for_network<'a>(&'a self, network: &'a str) -> impl Iterator<Item = &'a KeyRecord> + 'a {
        self.file.keys.iter().filter(move |k| k.network == network)
    }

    /// Find a library key matching a fingerprint + xpub (used by restore).
    pub fn find_match(&self, fingerprint: Option<&str>, xpub: &str) -> Option<&KeyRecord> {
        self.file.keys.iter().find(|k| {
            k.xpub == xpub && fingerprint.is_none_or(|fp| fp.eq_ignore_ascii_case(&k.fingerprint))
        })
    }

    pub fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }

    /// A keychain password has been set (at least one hot key was stored).
    pub fn has_password(&self) -> bool {
        self.file.verifier.is_some()
    }

    pub fn is_unlocked(&self) -> bool {
        self.password.is_some()
    }

    pub fn hot_count(&self) -> usize {
        self.file.keys.iter().filter(|k| k.is_hot()).count()
    }

    // ── lock / unlock ──

    /// Verify and remember the password. On a keychain without a password
    /// this *sets* it (minimum 8 characters).
    pub fn unlock(&mut self, password: String, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        let verifier = self.file.verifier.clone();
        let pw = Zeroizing::new(password);
        let work = {
            let pw = pw.clone();
            backend(cx).call(move |_, _| async move {
                match verifier {
                    Some(v) => decrypt_seed(&v, pw.as_bytes())
                        .map(|_| None)
                        .map_err(|_| "Wrong keychain password".to_string()),
                    None => {
                        if pw.chars().count() < 8 {
                            return Err("Use at least 8 characters".into());
                        }
                        encrypt_seed(VERIFIER_PLAINTEXT, pw.as_bytes())
                            .map(Some)
                            .map_err(|e| e.to_string())
                    }
                }
            })
        };
        cx.spawn(async move |this, cx| {
            let new_verifier = work.await?;
            this.update(cx, |this, cx| {
                if let Some(v) = new_verifier {
                    this.file.verifier = Some(v);
                    this.persist(cx);
                }
                this.password = Some(pw);
                cx.emit(KeychainEvent::LockChanged);
                cx.notify();
            })
            .map_err(|e| e.to_string())
        })
    }

    pub fn lock(&mut self, cx: &mut Context<Self>) {
        if self.password.take().is_some() {
            cx.emit(KeychainEvent::LockChanged);
            cx.notify();
        }
    }

    // ── mutations ──

    /// Add a public-only key (hardware, xpub). No password needed.
    pub fn add_public(&mut self, key: NewKey, cx: &mut Context<Self>) -> Result<String, String> {
        self.check_duplicate(&key)?;
        let record = Self::record(key, None);
        let id = record.id.clone();
        self.file.keys.push(record);
        self.persist(cx);
        cx.emit(KeychainEvent::Changed);
        cx.notify();
        Ok(id)
    }

    /// Add a key whose private material is stored (generated, seed, xprv,
    /// key file). Requires the keychain to be unlocked.
    pub fn add_hot(
        &mut self,
        key: NewKey,
        secret: HotSecret,
        cx: &mut Context<Self>,
    ) -> Task<Result<String, String>> {
        if let Err(e) = self.check_duplicate(&key) {
            return Task::ready(Err(e));
        }
        let Some(pw) = self.password.clone() else {
            return Task::ready(Err("Unlock the keychain first".into()));
        };
        let work = backend(cx).call(move |_, _| async move {
            let json = Zeroizing::new(serde_json::to_vec(&secret).map_err(|e| e.to_string())?);
            encrypt_seed(&json, pw.as_bytes()).map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let env = work.await?;
            this.update(cx, |this, cx| {
                let record = Self::record(key, Some(env));
                let id = record.id.clone();
                this.file.keys.push(record);
                this.persist(cx);
                cx.emit(KeychainEvent::Changed);
                cx.notify();
                id
            })
            .map_err(|e| e.to_string())
        })
    }

    /// Decrypt a hot key's secret. Requires the keychain to be unlocked.
    pub fn secret(&self, id: &str, cx: &mut Context<Self>) -> Task<Result<HotSecret, String>> {
        let Some(env) = self.get(id).and_then(|k| k.secret.clone()) else {
            return Task::ready(Err("This key has no private material on this computer".into()));
        };
        let Some(pw) = self.password.clone() else {
            return Task::ready(Err("Unlock the keychain first".into()));
        };
        let work = backend(cx).call(move |_, _| async move {
            let plain = Zeroizing::new(
                decrypt_seed(&env, pw.as_bytes()).map_err(|_| "Wrong keychain password".to_string())?,
            );
            serde_json::from_slice::<HotSecret>(&plain).map_err(|e| e.to_string())
        });
        cx.spawn(async move |_, _| work.await)
    }

    pub fn rename(&mut self, id: &str, label: String, cx: &mut Context<Self>) {
        if let Some(k) = self.file.keys.iter_mut().find(|k| k.id == id) {
            k.label = label;
            self.persist(cx);
            cx.emit(KeychainEvent::Changed);
            cx.notify();
        }
    }

    /// Remove a key from the library. Wallets already created from it keep
    /// their own copy and are unaffected.
    pub fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        let before = self.file.keys.len();
        self.file.keys.retain(|k| k.id != id);
        if self.file.keys.len() != before {
            self.persist(cx);
            cx.emit(KeychainEvent::Changed);
            cx.notify();
        }
    }

    // ── internals ──

    fn check_duplicate(&self, key: &NewKey) -> Result<(), String> {
        if let Some(existing) = self.file.keys.iter().find(|k| {
            k.xpub == key.xpub && k.fingerprint.eq_ignore_ascii_case(&key.fingerprint)
        }) {
            return Err(format!("This key is already in your library as “{}”", existing.label));
        }
        Ok(())
    }

    fn record(key: NewKey, secret: Option<EncryptedSeed>) -> KeyRecord {
        KeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            label: key.label,
            network: key.network,
            source: key.source,
            fingerprint: key.fingerprint.to_lowercase(),
            derivation_path: key.derivation_path,
            xpub: key.xpub,
            device: key.device,
            created_at: chrono::Utc::now().timestamp(),
            secret,
        }
    }

    /// Write the file off the UI thread (atomic rename).
    fn persist(&mut self, cx: &mut Context<Self>) {
        self.file.version = FILE_VERSION;
        let path = self.path.clone();
        let bytes = match serde_json::to_vec_pretty(&self.file) {
            Ok(b) => b,
            Err(e) => {
                log::error!("serialize keychain: {e}");
                return;
            }
        };
        let task = backend(cx).spawn(async move { write_atomic(&path, &bytes) });
        cx.background_spawn(async move {
            if let Some(Err(e)) = task.await {
                log::error!("write keychain: {e}");
            }
        })
        .detach();
    }
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_expr_formats_origin() {
        let k = KeyRecord {
            id: "x".into(),
            label: "A".into(),
            network: "regtest".into(),
            source: "xpub".into(),
            fingerprint: "deadbeef".into(),
            derivation_path: "m/84'/1'/0'".into(),
            xpub: "tpubX".into(),
            device: None,
            created_at: 0,
            secret: None,
        };
        assert_eq!(k.key_expr(), "[deadbeef/84'/1'/0']tpubX");
        assert!(!k.is_hot());
    }

    #[test]
    fn file_round_trips_and_tolerates_missing_fields() {
        let f: KeychainFile = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert!(f.keys.is_empty() && f.verifier.is_none());
        let env = encrypt_seed(VERIFIER_PLAINTEXT, b"password1").unwrap();
        assert_eq!(decrypt_seed(&env, b"password1").unwrap(), VERIFIER_PLAINTEXT);
        assert!(decrypt_seed(&env, b"wrong-pass").is_err());
    }
}
