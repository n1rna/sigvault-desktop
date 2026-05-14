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

use bdk_wallet::bitcoin::Network;
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
        // Regtest: the sigvault-hosted electrs, exposed via Traefik
        // TCP+SNI on :443 so it shares the existing TLS infra (no
        // dedicated Electrum port to firewall-allow, free LE cert).
        electrs_urls.insert(
            "regtest".to_string(),
            "ssl://ers.regtest.sigvault.org:443".to_string(),
        );
        // Testnet4 and signet ship empty: there is no single public
        // electrs server with both broad uptime and a stable URL we
        // can point users at without setting them up for downtime.
        // The settings UI (QBL-231) shows these as "configure your own
        // server" rather than as preconfigured working endpoints. Users
        // who run their own node (Sparrow / Mempool.space self-hosted /
        // electrs.bublina) supply the URL there.
        electrs_urls.insert("testnet4".to_string(), String::new());
        electrs_urls.insert("signet".to_string(), String::new());
        Self {
            default_network: "regtest".to_string(),
            electrs_urls,
        }
    }
}

impl LocalSettings {
    /// Resolve the configured Electrs URL for `network`. Returns an
    /// `EmptyEndpoint` error when the user has not configured a URL for
    /// a non-mainnet network the manager supports — the sync engine
    /// uses this and surfaces the error to the UI as "Configure an
    /// Electrs server in Settings".
    ///
    /// Mainnet is intentionally rejected here as well so the "no
    /// mainnet in v1" gate (QBL-232) holds even if a wallet's metadata
    /// somehow declares mainnet.
    pub fn electrs_url_for(&self, network: Network) -> Result<String, SettingsError> {
        let key =
            network_key(network).ok_or(SettingsError::UnsupportedNetwork(network.to_string()))?;
        match self.electrs_urls.get(key) {
            Some(url) if !url.is_empty() => Ok(url.clone()),
            _ => Err(SettingsError::EmptyEndpoint(key.to_string())),
        }
    }
}

/// Single feature flag for mainnet support across the standalone-wallet
/// stack. Mirror of `MAINNET_ENABLED` in `src/constants/networks.ts` —
/// flip both in lockstep when mainnet support lands. Until then every
/// network gate (`network_key`, `ensure_supported_network`, the
/// settings UI's network selector, the create-wallet wizard's network
/// picker) consults this single bool rather than hardcoding a check.
pub const MAINNET_ENABLED: bool = false;

/// Map a `bitcoin::Network` to the JSON key used in `electrs_urls`.
/// Returns `None` for networks not supported by the current build —
/// mainnet today (gated by `MAINNET_ENABLED`), plus any future variants
/// bitcoin's enum may gain.
pub fn network_key(network: Network) -> Option<&'static str> {
    match network {
        Network::Regtest => Some("regtest"),
        Network::Signet => Some("signet"),
        Network::Testnet4 => Some("testnet4"),
        Network::Testnet => Some("testnet"),
        Network::Bitcoin if MAINNET_ENABLED => Some("bitcoin"),
        Network::Bitcoin => None,
    }
}

/// Validate an electrs URL accepted by `electrum_client::Client::new`.
/// Accepts `tcp://host:port`, `ssl://host:port`, or bare `host:port`.
/// We do not attempt a TCP connection here — connecting is QBL-218 sync
/// territory; this is just structural validation so set_settings rejects
/// obvious typos before they get persisted.
pub fn validate_electrs_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    let host_port = url
        .strip_prefix("tcp://")
        .or_else(|| url.strip_prefix("ssl://"))
        .unwrap_or(url);
    let mut parts = host_port.rsplitn(2, ':');
    let port_str = parts.next().unwrap_or("");
    let host = parts.next().unwrap_or("");
    if host.is_empty() {
        return Err(format!(
            "URL must include host:port (got '{url}'); use tcp://host:port, ssl://host:port, or host:port"
        ));
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| format!("URL port must be a number 1-65535 (got '{port_str}')"))?;
    if port == 0 {
        return Err("URL port must be in 1-65535".to_string());
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid electrs URL for {network}: {message}")]
    InvalidUrl { network: String, message: String },
    #[error("no electrs URL configured for {0} — set one in Settings")]
    EmptyEndpoint(String),
    #[error("network '{0}' is not supported in v1 standalone mode")]
    UnsupportedNetwork(String),
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

    /// Validate every non-empty URL in the settings, then persist. Empty
    /// strings are accepted (they mean "user has not configured this
    /// network yet"); structural junk is rejected before disk write.
    pub fn save(&self, settings: &LocalSettings) -> Result<(), SettingsError> {
        for (network, url) in &settings.electrs_urls {
            if url.is_empty() {
                continue;
            }
            validate_electrs_url(url).map_err(|message| SettingsError::InvalidUrl {
                network: network.clone(),
                message,
            })?;
        }
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
            Some(&"ssl://ers.regtest.sigvault.org:443".to_string())
        );
    }

    #[test]
    fn defaults_have_blank_testnet4_and_signet() {
        let s = LocalSettings::default();
        assert_eq!(s.electrs_urls.get("testnet4"), Some(&String::new()));
        assert_eq!(s.electrs_urls.get("signet"), Some(&String::new()));
    }

    #[test]
    fn defaults_have_no_mainnet_entry() {
        let s = LocalSettings::default();
        assert!(!s.electrs_urls.contains_key("bitcoin"));
    }

    #[test]
    fn save_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());

        let mut original = LocalSettings {
            default_network: "testnet4".to_string(),
            ..LocalSettings::default()
        };
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

    #[test]
    fn validate_accepts_known_good_urls() {
        for ok in [
            "tcp://regtest.sigvault.org:50001",
            "ssl://electrum.example.org:50002",
            "host.local:8080",
            "127.0.0.1:50001",
        ] {
            validate_electrs_url(ok)
                .unwrap_or_else(|e| panic!("expected '{ok}' to validate, got error: {e}"));
        }
    }

    #[test]
    fn validate_rejects_obvious_junk() {
        for (bad, why) in [
            ("", "empty"),
            ("nohost", "missing port"),
            ("host:notaport", "non-numeric port"),
            ("host:0", "zero port"),
            (":50001", "missing host"),
            ("ssl://", "scheme only"),
        ] {
            assert!(
                validate_electrs_url(bad).is_err(),
                "expected '{bad}' to fail ({why})"
            );
        }
    }

    #[test]
    fn save_rejects_invalid_url_and_does_not_write() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());

        let mut bad = LocalSettings::default();
        bad.electrs_urls
            .insert("regtest".to_string(), "broken-no-port".to_string());

        match store.save(&bad) {
            Err(SettingsError::InvalidUrl { network, .. }) => {
                assert_eq!(network, "regtest");
            }
            other => panic!("expected InvalidUrl, got {other:?}"),
        }

        assert!(
            !store.path().exists(),
            "settings file must NOT be written when validation fails"
        );
    }

    #[test]
    fn save_accepts_blank_url_for_unconfigured_network() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());
        // Defaults have empty testnet4/signet entries; save must accept
        // them (they mean "user hasn't configured this network yet").
        store
            .save(&LocalSettings::default())
            .expect("save defaults");
    }

    #[test]
    fn electrs_url_for_returns_configured_url() {
        let s = LocalSettings::default();
        assert_eq!(
            s.electrs_url_for(Network::Regtest).unwrap(),
            "ssl://ers.regtest.sigvault.org:443"
        );
    }

    #[test]
    fn electrs_url_for_errors_when_blank() {
        let s = LocalSettings::default();
        match s.electrs_url_for(Network::Testnet4) {
            Err(SettingsError::EmptyEndpoint(net)) => assert_eq!(net, "testnet4"),
            other => panic!("expected EmptyEndpoint(testnet4), got {other:?}"),
        }
    }

    #[test]
    fn electrs_url_for_rejects_mainnet() {
        let mut s = LocalSettings::default();
        // Even if someone shoves a mainnet URL into the map, the v1
        // gate refuses it: lookup goes through network_key which
        // returns None for Bitcoin → UnsupportedNetwork.
        s.electrs_urls.insert(
            "bitcoin".to_string(),
            "ssl://main.example:50002".to_string(),
        );
        match s.electrs_url_for(Network::Bitcoin) {
            Err(SettingsError::UnsupportedNetwork(n)) => assert_eq!(n, "bitcoin"),
            other => panic!("expected UnsupportedNetwork(bitcoin), got {other:?}"),
        }
    }
}
