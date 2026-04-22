// Application state management

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::api::types::{RemoteSession, UserProfile};
use crate::config::parse_network_str;
use crate::env_config::EnvConfig;
use crate::hwi::HardwareWalletManager;
use crate::oauth::OAuthState;
use crate::websocket::WebsocketHandler;

#[derive(Default, Clone)]
pub struct AuthTokens {
    oauth_access_token: Option<String>,
}

impl AuthTokens {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_oauth_access_token(&mut self, token: String) {
        self.oauth_access_token = Some(token);
    }

    pub fn get_oauth_access_token(&self) -> Option<String> {
        self.oauth_access_token.clone()
    }

    pub fn clear(&mut self) {
        self.oauth_access_token = None;
    }
}

#[derive(Default, Clone)]
pub struct UserData {
    pub id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
}

impl UserData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_from_profile(&mut self, profile: UserProfile) {
        self.id = Some(profile.id);
        self.email = profile.email;
        self.name = profile.name;
        self.username = profile.username;
    }

    pub fn clear(&mut self) {
        self.id = None;
        self.email = None;
        self.name = None;
        self.username = None;
    }
}

#[derive(Clone)]
pub struct ApplicationState {
    pub ws_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub ws_handler: Arc<Mutex<Option<WebsocketHandler>>>,
    pub auth_tokens: Arc<Mutex<AuthTokens>>,
    pub user_data: Arc<Mutex<UserData>>,
    pub remote_sessions: Arc<Mutex<Vec<RemoteSession>>>,
    pub current_env: Arc<RwLock<Option<EnvConfig>>>,
    pub hw_manager: Arc<RwLock<Option<Arc<HardwareWalletManager>>>>,
    /// Active OAuth flow state. Rebuilt on each `cmd_authenticate` so PKCE
    /// and CSRF tokens are not reused, and so the auth URL reflects the
    /// currently selected environment.
    pub oauth_flow: Arc<RwLock<Option<OAuthState>>>,
}

impl ApplicationState {
    pub fn new() -> Self {
        Self {
            ws_thread: Arc::new(Mutex::new(None)),
            ws_handler: Arc::new(Mutex::new(None)),
            auth_tokens: Arc::new(Mutex::new(AuthTokens::new())),
            user_data: Arc::new(Mutex::new(UserData::new())),
            remote_sessions: Arc::new(Mutex::new(Vec::new())),
            current_env: Arc::new(RwLock::new(None)),
            hw_manager: Arc::new(RwLock::new(None)),
            oauth_flow: Arc::new(RwLock::new(None)),
        }
    }

    /// Apply a selected environment: stores it and (re)builds the hardware
    /// wallet manager bound to that network.
    pub async fn set_environment(&self, env: EnvConfig) {
        let network = parse_network_str(&env.network).unwrap_or(bitcoin::Network::Regtest);
        *self.hw_manager.write().await = Some(Arc::new(HardwareWalletManager::new(network)));
        *self.current_env.write().await = Some(env);
    }

    pub async fn clear_environment(&self) {
        *self.current_env.write().await = None;
        *self.hw_manager.write().await = None;
    }

    pub async fn require_env(&self) -> Result<EnvConfig, String> {
        self.current_env
            .read()
            .await
            .clone()
            .ok_or_else(|| "No environment selected".to_string())
    }

    pub async fn require_api_base_url(&self) -> Result<String, String> {
        Ok(self.require_env().await?.api_base_url)
    }

    pub async fn require_hw_manager(&self) -> Result<Arc<HardwareWalletManager>, String> {
        self.hw_manager
            .read()
            .await
            .clone()
            .ok_or_else(|| "Hardware wallet manager not initialized (no environment selected)".to_string())
    }
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self::new()
    }
}
