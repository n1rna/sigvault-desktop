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
