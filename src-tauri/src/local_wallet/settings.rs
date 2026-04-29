//! Non-secret local-mode settings (Electrs endpoints + default network).
//!
//! Persisted as plain JSON at `<app_data_dir>/local-settings.json`. None
//! of these fields are sensitive — leaking them just reveals which public
//! Electrs server the user pointed at. Encrypting them adds friction
//! without a security benefit, so this store is a sibling of the
//! encrypted `auth.dat` / `env.dat` rather than another encrypted file.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SETTINGS_FILENAME: &str = "local-settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSettings {
    /// Default network preselected in the wallet-creation wizard. Falls
    /// back to "regtest" on first launch.
    pub default_network: String,
    /// Electrs endpoint URLs keyed by network (`regtest`, `testnet4`,
    /// `signet`). Mainnet entry is intentionally absent in v1; QBL-232
    /// gates mainnet off across the stack.
    pub electrs_urls: BTreeMap<String, String>,
}

impl Default for LocalSettings {
    fn default() -> Self {
        let mut electrs_urls = BTreeMap::new();
        electrs_urls.insert(
            "regtest".to_string(),
            "tcp://regtest.sigvault.org:50001".to_string(),
        );
        // testnet4 + signet defaults are filled in by QBL-217 once we
        // pick the public servers we want to ship with. Empty-string
        // here means "user must override before sync".
        electrs_urls.insert("testnet4".to_string(), String::new());
        electrs_urls.insert("signet".to_string(), String::new());
        Self {
            default_network: "regtest".to_string(),
            electrs_urls,
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            path: app_data_dir.join(SETTINGS_FILENAME),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn load(&self) -> Result<LocalSettings, SettingsError> {
        if !self.path.exists() {
            return Ok(LocalSettings::default());
        }
        let bytes = fs::read(&self.path)?;
        // Tolerate older / corrupt files by falling back to defaults
        // rather than refusing to start; the user can re-set values from
        // the settings page.
        match serde_json::from_slice::<LocalSettings>(&bytes) {
            Ok(s) => Ok(s),
            Err(_) => Ok(LocalSettings::default()),
        }
    }

    pub fn save(&self, settings: &LocalSettings) -> Result<(), SettingsError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(settings)?;
        fs::write(&self.path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn defaults_carry_regtest_endpoint() {
        let s = LocalSettings::default();
        assert_eq!(s.default_network, "regtest");
        assert_eq!(
            s.electrs_urls.get("regtest"),
            Some(&"tcp://regtest.sigvault.org:50001".to_string())
        );
    }

    #[test]
    fn save_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());

        let mut original = LocalSettings::default();
        original.default_network = "testnet4".to_string();
        original
            .electrs_urls
            .insert("testnet4".to_string(), "ssl://example:50002".to_string());

        store.save(&original).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded, original);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());
        let loaded = store.load().expect("load");
        assert_eq!(loaded, LocalSettings::default());
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());
        fs::write(store.path(), b"not json{").unwrap();
        let loaded = store.load().expect("load");
        assert_eq!(loaded, LocalSettings::default());
    }
}
