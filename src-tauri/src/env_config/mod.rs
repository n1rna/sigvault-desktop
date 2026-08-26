// Runtime environment (network) configuration.
//
// The desktop app supports multiple SigVault deployments (regtest, signet,
// testnet, mainnet). The list of available environments is fetched from
// `https://sigvault.org/environments.json` at boot, cached on disk for
// offline use, and refreshed at most once per hour.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const ENVIRONMENTS_URL: &str = "https://sigvault.org/environments.json";
const CACHE_FILENAME: &str = "environments.cache.json";
const CACHE_TTL_SECS: u64 = 60 * 60;
const FETCH_TIMEOUT_SECS: u64 = 5;

/// Built-in OAuth client registrations, keyed by environment `id`.
/// `(id, client_id, token_url)`
///
/// Every network runs its own self-hosted Zitadel with its own desktop
/// app registration, so these cannot be a single compile-time value —
/// one binary has to be able to authenticate against any environment the
/// user picks. The client IDs mirror `ZITADEL_DESKTOP_APP_CLIENT_ID` in
/// `projects/api/<network>.env` in the private sigvault-secrets repo.
///
/// A desktop OAuth client ID is public by design: this is a PKCE public
/// client, there is no client secret here, and the same ID ships in every
/// copy of the binary.
///
/// `mainnet` is deliberately absent — it is not deployed yet (the manifest
/// marks it `comingSoon`, and `cmd_select_env` refuses those). When it
/// launches, the manifest can carry `clientId`/`tokenUrl` for it and
/// existing installs will pick it up without a new release.
const BUILTIN_OAUTH_CLIENTS: &[(&str, &str, &str)] = &[
    (
        "regtest",
        "385423931282030606",
        "https://auth.regtest.sigvault.org/oauth/v2/token",
    ),
    (
        "signet",
        "386571583755386893",
        "https://auth.signet.sigvault.org/oauth/v2/token",
    ),
    (
        "testnet",
        "386574003667468301",
        "https://auth.testnet.sigvault.org/oauth/v2/token",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvConfig {
    pub id: String,
    pub name: String,
    pub network: String,
    #[serde(rename = "apiBaseUrl")]
    pub api_base_url: String,
    /// Per-env OAuth authorization URL (wrapper page hosted on the env's
    /// web app that initiates the Zitadel flow). Optional in the manifest
    /// — when absent, falls back to deriving it from `apiBaseUrl`.
    #[serde(default, rename = "authUrl")]
    pub auth_url: Option<String>,
    /// Per-env OAuth client ID. Optional in the manifest — when absent,
    /// falls back to `BUILTIN_OAUTH_CLIENTS`. Lets a new environment ship
    /// without a desktop release.
    #[serde(default, rename = "clientId")]
    pub client_id: Option<String>,
    /// Per-env OAuth token exchange URL. Optional; same fallback as
    /// `client_id`.
    #[serde(default, rename = "tokenUrl")]
    pub token_url: Option<String>,
    #[serde(default, rename = "comingSoon")]
    pub coming_soon: bool,
}

impl EnvConfig {
    pub fn websocket_base_url(&self) -> String {
        self.api_base_url
            .replace("https://", "wss://")
            .replace("http://", "ws://")
    }

    /// Resolve the OAuth authorization URL for this environment. Uses the
    /// explicit `authUrl` when present, otherwise derives it from
    /// `apiBaseUrl` by stripping the `api.` host prefix and appending
    /// `/auth/desktop-login`.
    pub fn resolved_auth_url(&self) -> String {
        if let Some(url) = &self.auth_url {
            return url.clone();
        }
        let derived = self
            .api_base_url
            .replacen("://api.", "://", 1)
            .trim_end_matches('/')
            .to_string();
        format!("{derived}/auth/desktop-login")
    }

    /// OAuth client ID for this environment: manifest value first, then the
    /// built-in table.
    pub fn resolved_client_id(&self) -> Result<String, String> {
        if let Some(id) = &self.client_id {
            return Ok(id.clone());
        }
        self.builtin()
            .map(|(_, client_id, _)| client_id.to_string())
            .ok_or_else(|| self.missing_oauth_error())
    }

    /// OAuth token exchange URL for this environment: manifest value first,
    /// then the built-in table.
    pub fn resolved_token_url(&self) -> Result<String, String> {
        if let Some(url) = &self.token_url {
            return Ok(url.clone());
        }
        self.builtin()
            .map(|(_, _, token_url)| token_url.to_string())
            .ok_or_else(|| self.missing_oauth_error())
    }

    fn builtin(&self) -> Option<&'static (&'static str, &'static str, &'static str)> {
        BUILTIN_OAUTH_CLIENTS
            .iter()
            .find(|(id, _, _)| *id == self.id)
    }

    fn missing_oauth_error(&self) -> String {
        format!(
            "No OAuth client configured for environment '{}'. This build of \
             SigVault Desktop predates that environment — update the app, or \
             the environments manifest must supply `clientId` and `tokenUrl`.",
            self.id
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvManifest {
    #[serde(default)]
    pub version: u32,
    pub environments: Vec<EnvConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    fetched_at: u64,
    manifest: EnvManifest,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;
    Ok(dir.join(CACHE_FILENAME))
}

fn read_cache(app: &AppHandle) -> Option<CacheFile> {
    let path = cache_path(app).ok()?;
    if !path.exists() {
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice::<CacheFile>(&bytes).ok()
}

fn write_cache(app: &AppHandle, manifest: &EnvManifest) {
    let Ok(path) = cache_path(app) else {
        return;
    };
    let cache = CacheFile {
        fetched_at: now_secs(),
        manifest: manifest.clone(),
    };
    if let Ok(bytes) = serde_json::to_vec(&cache) {
        if let Err(e) = fs::write(&path, bytes) {
            warn!("Failed to write environments cache: {e}");
        }
    }
}

async fn fetch_remote() -> Result<EnvManifest, String> {
    debug!("Fetching environments manifest from {ENVIRONMENTS_URL}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let resp = client
        .get(ENVIRONMENTS_URL)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<EnvManifest>()
        .await
        .map_err(|e| format!("Failed to parse JSON: {e}"))
}

/// Load environments. Tries network first (with timeout); on failure or
/// stale-but-acceptable cache, returns the cached manifest. Returns an
/// error only when neither network nor cache are available.
pub async fn load(app: &AppHandle) -> Result<EnvManifest, String> {
    let cache = read_cache(app);

    // If cache is fresh (< TTL), return it without hitting the network.
    if let Some(cached) = &cache {
        let age = now_secs().saturating_sub(cached.fetched_at);
        if age < CACHE_TTL_SECS {
            debug!("Using fresh environments cache (age {age}s)");
            return Ok(cached.manifest.clone());
        }
    }

    match fetch_remote().await {
        Ok(manifest) => {
            info!(
                "Fetched {} environments from manifest",
                manifest.environments.len()
            );
            write_cache(app, &manifest);
            Ok(manifest)
        }
        Err(e) => {
            warn!("Environments fetch failed: {e}");
            if let Some(cached) = cache {
                info!("Falling back to cached environments");
                Ok(cached.manifest)
            } else {
                error!("No cached environments available");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with_id(id: &str) -> EnvConfig {
        EnvConfig {
            id: id.to_string(),
            name: id.to_string(),
            network: id.to_string(),
            api_base_url: format!("https://api.{id}.sigvault.org"),
            auth_url: None,
            client_id: None,
            token_url: None,
            coming_soon: false,
        }
    }

    /// Every environment the live manifest offers for selection must
    /// resolve to an OAuth client, or a user picking it hits a dead end at
    /// the login button. `mainnet` is excluded on purpose (comingSoon).
    #[test]
    fn every_selectable_env_resolves_an_oauth_client() {
        for id in ["regtest", "signet", "testnet"] {
            let env = env_with_id(id);
            let client_id = env.resolved_client_id().expect(id);
            let token_url = env.resolved_token_url().expect(id);
            assert!(!client_id.is_empty(), "{id} has an empty client id");
            assert_eq!(
                token_url,
                format!("https://auth.{id}.sigvault.org/oauth/v2/token"),
            );
        }
    }

    /// The three networks must not share a client ID — each Zitadel has its
    /// own registration, and a copy/paste slip here would silently send one
    /// network's users at another's identity provider.
    #[test]
    fn builtin_client_ids_are_distinct() {
        for (i, (id_a, client_a, _)) in BUILTIN_OAUTH_CLIENTS.iter().enumerate() {
            for (id_b, client_b, _) in &BUILTIN_OAUTH_CLIENTS[i + 1..] {
                assert_ne!(client_a, client_b, "{id_a} and {id_b} share a client ID");
            }
        }
    }

    #[test]
    fn manifest_values_override_the_builtin_table() {
        let mut env = env_with_id("regtest");
        env.client_id = Some("999".to_string());
        env.token_url = Some("https://example.test/token".to_string());
        assert_eq!(env.resolved_client_id().unwrap(), "999");
        assert_eq!(
            env.resolved_token_url().unwrap(),
            "https://example.test/token"
        );
    }

    #[test]
    fn unknown_env_without_manifest_values_errors() {
        let env = env_with_id("mainnet");
        assert!(env.resolved_client_id().is_err());
        assert!(env.resolved_token_url().is_err());
    }

    /// A manifest entry for a brand-new network works with no app update,
    /// which is the whole point of the optional fields.
    #[test]
    fn unknown_env_works_when_the_manifest_supplies_oauth() {
        let json = r#"{
            "id": "mainnet",
            "name": "Mainnet",
            "network": "mainnet",
            "apiBaseUrl": "https://api.mainnet.sigvault.org",
            "clientId": "123456789",
            "tokenUrl": "https://auth.mainnet.sigvault.org/oauth/v2/token"
        }"#;
        let env: EnvConfig = serde_json::from_str(json).unwrap();
        assert_eq!(env.resolved_client_id().unwrap(), "123456789");
        assert_eq!(
            env.resolved_token_url().unwrap(),
            "https://auth.mainnet.sigvault.org/oauth/v2/token"
        );
    }

    /// Manifests written before these fields existed must still parse.
    #[test]
    fn manifest_without_oauth_fields_still_parses() {
        let json = r#"{
            "id": "regtest",
            "name": "Regtest",
            "network": "regtest",
            "apiBaseUrl": "https://api.regtest.sigvault.org",
            "authUrl": "https://regtest.sigvault.org/auth/desktop-login"
        }"#;
        let env: EnvConfig = serde_json::from_str(json).unwrap();
        assert!(env.client_id.is_none());
        assert_eq!(env.resolved_client_id().unwrap(), "385423931282030606");
    }
}
