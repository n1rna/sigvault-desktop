//! Non-secret local-mode settings (chain server endpoints + default network).
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
    /// Chain server URLs keyed by network (`regtest`, `testnet4`,
    /// `signet`): `https://…` for an Esplora server, `ssl://host:port` /
    /// `tcp://host:port` for an Electrum server (see `chain::BackendKind`).
    /// The field keeps its historical name for on-disk compatibility.
    /// Mainnet entry is intentionally absent in v1; QBL-232 gates mainnet
    /// off across the stack.
    pub electrs_urls: BTreeMap<String, String>,
}

/// Sigvault-hosted Esplora servers: the electrs of each sigvault
/// environment, published over HTTPS behind Cloudflare.
pub const HOSTED_ESPLORA_REGTEST: &str = "https://esplora.regtest.sigvault.org";
pub const HOSTED_ESPLORA_TESTNET4: &str = "https://esplora.testnet.sigvault.org";

/// The hosted default for `key`, if sigvault runs a server for it.
pub fn hosted_default(key: &str) -> Option<&'static str> {
    match key {
        "regtest" => Some(HOSTED_ESPLORA_REGTEST),
        "testnet4" => Some(HOSTED_ESPLORA_TESTNET4),
        _ => None,
    }
}

/// Earlier builds defaulted to Electrum at `ssl://ers.<env>.sigvault.org:443`.
/// That only worked while the servers ran on a VPS; behind Cloudflare the
/// Electrum protocol can't get through, so these are rewritten on load.
fn is_retired_hosted_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    let host = u
        .strip_prefix("ssl://")
        .or_else(|| u.strip_prefix("tcp://"))
        .unwrap_or(&u);
    host.starts_with("ers.")
        && host
            .split(':')
            .next()
            .unwrap_or("")
            .ends_with(".sigvault.org")
}

impl Default for LocalSettings {
    fn default() -> Self {
        let mut electrs_urls = BTreeMap::new();
        electrs_urls.insert("regtest".to_string(), HOSTED_ESPLORA_REGTEST.to_string());
        electrs_urls.insert("testnet4".to_string(), HOSTED_ESPLORA_TESTNET4.to_string());
        // Sigvault doesn't run a signet server; users supply their own.
        electrs_urls.insert("signet".to_string(), String::new());
        Self {
            default_network: "regtest".to_string(),
            electrs_urls,
        }
    }
}

impl LocalSettings {
    /// Bring settings saved by older builds up to date: retired hosted
    /// Electrum URLs and unset hosted networks get the hosted Esplora
    /// default. Returns whether anything changed.
    pub fn migrate(&mut self) -> bool {
        let mut changed = false;
        for key in ["regtest", "testnet4"] {
            let default = hosted_default(key).unwrap_or_default();
            let current = self.electrs_urls.get(key).map(String::as_str).unwrap_or("");
            if current.trim().is_empty() || is_retired_hosted_url(current) {
                self.electrs_urls
                    .insert(key.to_string(), default.to_string());
                changed = true;
            }
        }
        changed
    }

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

/// Validate a chain server URL. Accepts `http(s)://host[:port][/path]`
/// (Esplora) and `tcp://host:port`, `ssl://host:port`, or bare `host:port`
/// (Electrum).
/// We do not attempt a TCP connection here — connecting is QBL-218 sync
/// territory; this is just structural validation so set_settings rejects
/// obvious typos before they get persisted.
pub fn validate_electrs_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    {
        let authority = rest.split('/').next().unwrap_or("");
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        };
        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err(format!("URL must include a host (got '{url}')"));
        }
        if let Some(p) = port {
            match p.parse::<u16>() {
                Ok(n) if n > 0 => {}
                _ => return Err(format!("URL port must be a number 1-65535 (got '{p}')")),
            }
        }
        return Ok(());
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
            "URL must include host:port (got '{url}'); use https://host for Esplora, or ssl://host:port, tcp://host:port or host:port for Electrum"
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
    #[error("invalid server URL for {network}: {message}")]
    InvalidUrl { network: String, message: String },
    #[error("no server URL configured for {0} — set one in Settings")]
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
            Ok(mut s) => {
                if s.migrate() {
                    // Best effort: the migrated values are used either way.
                    if let Err(e) = self.save(&s) {
                        log::warn!("couldn't save migrated local settings: {e}");
                    }
                }
                Ok(s)
            }
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
    fn defaults_carry_hosted_esplora_endpoints() {
        let s = LocalSettings::default();
        assert_eq!(s.default_network, "regtest");
        assert_eq!(
            s.electrs_urls.get("regtest").map(String::as_str),
            Some("https://esplora.regtest.sigvault.org")
        );
        assert_eq!(
            s.electrs_urls.get("testnet4").map(String::as_str),
            Some("https://esplora.testnet.sigvault.org")
        );
    }

    #[test]
    fn defaults_have_blank_signet() {
        let s = LocalSettings::default();
        assert_eq!(s.electrs_urls.get("signet"), Some(&String::new()));
    }

    #[test]
    fn load_migrates_retired_and_blank_hosted_urls() {
        let tmp = TempDir::new().unwrap();
        let store = SettingsStore::new(tmp.path().to_path_buf());
        fs::write(
            store.path(),
            br#"{"default_network":"regtest","electrs_urls":{"regtest":"ssl://ers.regtest.sigvault.org:443","testnet4":"","signet":"ssl://signet.example:50002"}}"#,
        )
        .unwrap();
        let loaded = store.load().expect("load");
        assert_eq!(loaded.electrs_urls["regtest"], HOSTED_ESPLORA_REGTEST);
        assert_eq!(loaded.electrs_urls["testnet4"], HOSTED_ESPLORA_TESTNET4);
        assert_eq!(loaded.electrs_urls["signet"], "ssl://signet.example:50002");
        // Persisted, so the settings page shows the new values too.
        let on_disk: LocalSettings =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(on_disk, loaded);
    }

    #[test]
    fn migrate_keeps_user_configured_servers() {
        let mut s = LocalSettings::default();
        s.electrs_urls
            .insert("regtest".to_string(), "tcp://127.0.0.1:60401".to_string());
        s.electrs_urls.insert(
            "testnet4".to_string(),
            "ssl://mempool.space:40002".to_string(),
        );
        assert!(!s.migrate());
        assert_eq!(s.electrs_urls["regtest"], "tcp://127.0.0.1:60401");
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
            "https://esplora.regtest.sigvault.org",
            "https://mempool.space/testnet4/api/",
            "http://localhost:3002",
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
            ("https://", "https without host"),
            ("https://host:0", "https zero port"),
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
            "https://esplora.regtest.sigvault.org"
        );
    }

    #[test]
    fn electrs_url_for_errors_when_blank() {
        let s = LocalSettings::default();
        match s.electrs_url_for(Network::Signet) {
            Err(SettingsError::EmptyEndpoint(net)) => assert_eq!(net, "signet"),
            other => panic!("expected EmptyEndpoint(signet), got {other:?}"),
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
