// Application state management

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api::types::{RemoteSession, UserProfile};
use crate::websocket::WebsocketHandler;
use crate::window::{create_shared_window_state, SharedWindowState};

#[derive(Default, Clone)]
pub struct AuthTokens {
    auth_session: Option<String>,
    oauth_access_token: Option<String>,
    user_auth_token: Option<String>,
    machine_auth_token: Option<String>,
}

impl AuthTokens {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_auth_session(&mut self, token: String) {
        self.auth_session = Some(token);
    }

    pub fn set_oauth_access_token(&mut self, token: String) {
        self.oauth_access_token = Some(token);
    }

    pub fn set_user_token(&mut self, token: String) {
        self.user_auth_token = Some(token);
    }

    pub fn set_machine_token(&mut self, token: String) {
        self.machine_auth_token = Some(token);
    }

    pub fn get_auth_session(&self) -> Option<String> {
        self.auth_session.clone()
    }

    pub fn get_oauth_access_token(&self) -> Option<String> {
        self.oauth_access_token.clone()
    }

    pub fn get_user_token(&self) -> Option<String> {
        self.user_auth_token.clone()
    }

    pub fn get_machine_token(&self) -> Option<String> {
        self.machine_auth_token.clone()
    }

    pub fn clear(&mut self) {
        self.auth_session = None;
        self.oauth_access_token = None;
        self.user_auth_token = None;
        self.machine_auth_token = None;
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
    pub registration_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub auth_tokens: Arc<Mutex<AuthTokens>>,
    pub user_data: Arc<Mutex<UserData>>,
    pub remote_sessions: Arc<Mutex<Vec<RemoteSession>>>,
    pub window_state: SharedWindowState,
}

impl ApplicationState {
    pub fn new() -> Self {
        Self {
            ws_thread: Arc::new(Mutex::new(None)),
            ws_handler: Arc::new(Mutex::new(None)),
            registration_thread: Arc::new(Mutex::new(None)),
            auth_tokens: Arc::new(Mutex::new(AuthTokens::new())),
            user_data: Arc::new(Mutex::new(UserData::new())),
            remote_sessions: Arc::new(Mutex::new(Vec::new())),
            window_state: create_shared_window_state(),
        }
    }
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self::new()
    }
}
