// Secure storage for authentication data.
//
// Tokens are encrypted at rest with ChaCha20-Poly1305 using a key bound to
// the current machine via `kdf::derive_machine_key`. The file layout is:
//
//     [12-byte nonce] [ciphertext || 16-byte Poly1305 tag]
//
// The encryption does not protect against an attacker running code as the
// current user on this machine (they can simply ask the app to read the
// token), but it does protect against offline theft of the storage file
// alone — decrypting it requires the machine's identifier.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::kdf;

const STORE_FILENAME: &str = "auth.dat";
const ENV_STORE_FILENAME: &str = "env.dat";
const NONCE_LEN: usize = 12;
const KEY_PURPOSE: &[u8] = b"auth-storage";
const ENV_KEY_PURPOSE: &[u8] = b"env-storage";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredAuthData {
    pub oauth_access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

pub struct SecureStorage {
    app: AppHandle,
}

impl SecureStorage {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn get_storage_path(&self) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let app_data_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {e}"))?;

        fs::create_dir_all(&app_data_dir)?;

        Ok(app_data_dir.join(STORE_FILENAME))
    }

    fn cipher() -> ChaCha20Poly1305 {
        let key_bytes = kdf::derive_machine_key(KEY_PURPOSE);
        ChaCha20Poly1305::new(Key::from_slice(&key_bytes))
    }

    fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let cipher = Self::cipher();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce_bytes)
            .map_err(|e| format!("Failed to generate nonce: {e}"))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("Encryption failed: {e}"))?;

        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(blob: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        if blob.len() < NONCE_LEN {
            return Err("auth.dat is too short to contain a nonce".into());
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let cipher = Self::cipher();
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed (wrong machine or corrupt file): {e}").into())
    }

    /// Initialize the storage
    pub async fn initialize(
        &self,
        _password: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Initializing secure storage");
        Ok(())
    }

    /// Store authentication data
    pub async fn store_auth_data(
        &self,
        auth_data: &StoredAuthData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Storing auth data securely");

        let json = serde_json::to_vec(auth_data)?;
        let encrypted = Self::encrypt(&json)?;

        let path = self.get_storage_path()?;
        fs::write(&path, encrypted)?;

        debug!("Auth data stored successfully at: {path:?}");
        Ok(())
    }

    /// Retrieve authentication data
    pub async fn get_auth_data(
        &self,
    ) -> Result<StoredAuthData, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Retrieving auth data");

        let path = self.get_storage_path()?;

        if !path.exists() {
            debug!("No auth data found");
            return Ok(StoredAuthData::default());
        }

        let blob = fs::read(&path)?;
        // Recover gracefully if the file predates encryption or is corrupt:
        // callers treat an empty result as "not logged in" and will prompt
        // the user to authenticate again rather than crashing.
        let plaintext = match Self::decrypt(&blob) {
            Ok(pt) => pt,
            Err(e) => {
                debug!("Failed to decrypt auth data, clearing: {e}");
                let _ = fs::remove_file(&path);
                return Ok(StoredAuthData::default());
            }
        };

        let auth_data: StoredAuthData = serde_json::from_slice(&plaintext)?;

        debug!("Auth data retrieved successfully");
        Ok(auth_data)
    }

    /// Clear all stored authentication data
    pub async fn clear_auth_data(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Clearing auth data");

        let path = self.get_storage_path()?;

        if path.exists() {
            fs::remove_file(&path)?;
        }

        debug!("Auth data cleared");
        Ok(())
    }

    /// Update OAuth access token
    pub async fn update_oauth_access_token(
        &self,
        token: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut data = self.get_auth_data().await?;
        data.oauth_access_token = Some(token);
        self.store_auth_data(&data).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredEnvData {
    pub selected_env_id: Option<String>,
}

/// Encrypted persistence for the user's selected environment.
/// Mirrors `SecureStorage` but uses a separate file + key purpose so the
/// two stores are independently rotatable.
pub struct EnvStorage {
    app: AppHandle,
}

impl EnvStorage {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn path(&self) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {e}"))?;
        fs::create_dir_all(&dir)?;
        Ok(dir.join(ENV_STORE_FILENAME))
    }

    fn cipher() -> ChaCha20Poly1305 {
        let key_bytes = kdf::derive_machine_key(ENV_KEY_PURPOSE);
        ChaCha20Poly1305::new(Key::from_slice(&key_bytes))
    }

    fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let cipher = Self::cipher();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce_bytes)
            .map_err(|e| format!("Failed to generate nonce: {e}"))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("Encryption failed: {e}"))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(blob: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        if blob.len() < NONCE_LEN {
            return Err("env.dat is too short to contain a nonce".into());
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let cipher = Self::cipher();
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed (wrong machine or corrupt file): {e}").into())
    }

    pub async fn store(
        &self,
        data: &StoredEnvData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_vec(data)?;
        let encrypted = Self::encrypt(&json)?;
        let path = self.path()?;
        fs::write(&path, encrypted)?;
        debug!("Env data stored at: {path:?}");
        Ok(())
    }

    pub async fn load(&self) -> Result<StoredEnvData, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path()?;
        if !path.exists() {
            return Ok(StoredEnvData::default());
        }
        let blob = fs::read(&path)?;
        let plaintext = match Self::decrypt(&blob) {
            Ok(pt) => pt,
            Err(e) => {
                debug!("Failed to decrypt env data, clearing: {e}");
                let _ = fs::remove_file(&path);
                return Ok(StoredEnvData::default());
            }
        };
        let data: StoredEnvData = serde_json::from_slice(&plaintext)?;
        Ok(data)
    }

    pub async fn clear(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path()?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_auth_data_serialization() {
        let data = StoredAuthData {
            oauth_access_token: Some("oauth_token".to_string()),
            refresh_token: None,
            expires_at: Some(1234567890),
        };

        let serialized = serde_json::to_string(&data).unwrap();
        let deserialized: StoredAuthData = serde_json::from_str(&serialized).unwrap();

        assert_eq!(data.oauth_access_token, deserialized.oauth_access_token);
    }

    #[test]
    fn test_stored_auth_data_default() {
        let data = StoredAuthData::default();
        assert!(data.oauth_access_token.is_none());
        assert!(data.refresh_token.is_none());
        assert!(data.expires_at.is_none());
    }

    #[test]
    fn test_stored_auth_data_roundtrip_all_fields() {
        let data = StoredAuthData {
            oauth_access_token: Some("access".to_string()),
            refresh_token: Some("refresh".to_string()),
            expires_at: Some(9999999999),
        };

        let json = serde_json::to_string_pretty(&data).unwrap();
        let restored: StoredAuthData = serde_json::from_str(&json).unwrap();

        assert_eq!(data.oauth_access_token, restored.oauth_access_token);
        assert_eq!(data.refresh_token, restored.refresh_token);
        assert_eq!(data.expires_at, restored.expires_at);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let data = StoredAuthData {
            oauth_access_token: Some("tok".into()),
            refresh_token: Some("rfr".into()),
            expires_at: Some(42),
        };
        let json = serde_json::to_vec(&data).unwrap();
        let ct = SecureStorage::encrypt(&json).unwrap();
        assert_ne!(ct, json, "ciphertext must differ from plaintext");
        assert!(ct.len() > NONCE_LEN);
        let pt = SecureStorage::decrypt(&ct).unwrap();
        let restored: StoredAuthData = serde_json::from_slice(&pt).unwrap();
        assert_eq!(restored.oauth_access_token.as_deref(), Some("tok"));
    }

    #[test]
    fn decrypt_rejects_tampered_ciphertext() {
        let ct = SecureStorage::encrypt(b"hello").unwrap();
        let mut tampered = ct.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(SecureStorage::decrypt(&tampered).is_err());
    }
}
