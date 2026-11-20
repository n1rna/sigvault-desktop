//! Global UI state: the GPUI equivalent of `useAppEvents` +
//! `AppStateContext`. Core commands emit `HostEvent`s; [`AppStore::apply`]
//! folds them into this entity exactly like the React reducer did, and
//! re-emits typed [`StoreEvent`]s for views that need to react (route
//! changes, wallet sync progress, ...).

use std::collections::HashMap;

use gpui::{App, AppContext, Context, Entity, EventEmitter, Global};
use serde::Deserialize;
use serde_json::Value;
use sigvault_core::app_mode::AppMode;
use sigvault_core::window::WindowApplicationRoute;
use sigvault_core::HostEvent;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Notification {
    pub title: String,
    pub message: String,
    #[serde(deserialize_with = "level_lenient")]
    pub level: NotificationLevel,
}

fn level_lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<NotificationLevel, D::Error> {
    let s = String::deserialize(d)?;
    Ok(match s.as_str() {
        "success" => NotificationLevel::Success,
        "warning" => NotificationLevel::Warning,
        "error" => NotificationLevel::Error,
        _ => NotificationLevel::Info,
    })
}

/// Mirrors `local_wallet::sync::SyncProgress` (`local_wallet_sync_progress`).
/// `phase` is one of `connecting`, `fetching_history`, `persisting`,
/// `complete`.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncProgress {
    pub wallet_id: String,
    pub phase: String,
    pub percent: u32,
    #[allow(dead_code)] // part of the wire format; pages render phase/percent
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteSession {
    pub id: String,
    pub status: String,
    pub session_type: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Mirrors `activeSession.sessionState` in the TS app.
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    pub session_type: Option<String>,
    pub step: Option<u32>,
    pub requirements: Option<Value>,
    pub data: Option<Value>,
    pub finished: Option<bool>,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveSession {
    pub is_connected: bool,
    pub session_id: Option<String>,
    pub session_state: SessionState,
}

#[derive(Debug, Deserialize)]
struct WireActiveSession {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    is_connected: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WireStateUpdate {
    #[serde(default)]
    authenticated: Option<bool>,
    #[serde(default)]
    route: Option<WindowApplicationRoute>,
    #[serde(default)]
    active_session: Option<WireActiveSession>,
    #[serde(default)]
    remote_sessions: Option<Vec<RemoteSession>>,
    #[serde(default)]
    app_mode: Option<AppMode>,
}

#[derive(Debug, Deserialize)]
struct WireSession {
    session_type: String,
    step: u32,
    requirements: Value,
    #[serde(default)]
    data: Option<Value>,
    finished: bool,
    success: bool,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireAppEvent {
    StateUpdate { data: WireStateUpdate },
    Session { data: WireSession },
    Notification { data: Notification },
    Command { data: Value },
}

#[derive(Debug, Clone)]
pub enum StoreEvent {
    RouteChanged,
    AuthChanged,
    ModeChanged,
    SessionUpdated,
    #[allow(dead_code)] // payload kept for subscribers (toasts, logging)
    Notification(Notification),
    SyncProgress(SyncProgress),
    /// Local wallets were created / deleted / renamed / locked / unlocked.
    /// Emitted by pages via [`local_changed`]; the sidebar refreshes on it.
    LocalDataChanged,
}

pub struct AppStore {
    pub authenticated: bool,
    pub route: WindowApplicationRoute,
    pub app_mode: Option<AppMode>,
    pub notification: Option<Notification>,
    pub activity_log: Vec<Notification>,
    pub active_session: ActiveSession,
    pub remote_sessions: Vec<RemoteSession>,
    /// Last sync progress per local wallet id.
    pub sync_progress: HashMap<String, SyncProgress>,
}

impl EventEmitter<StoreEvent> for AppStore {}

impl AppStore {
    pub fn new() -> Self {
        Self {
            authenticated: false,
            route: WindowApplicationRoute::Loading,
            app_mode: None,
            notification: None,
            activity_log: Vec::new(),
            active_session: ActiveSession::default(),
            remote_sessions: Vec::new(),
            sync_progress: HashMap::new(),
        }
    }

    pub fn clear_activity_log(&mut self, cx: &mut Context<Self>) {
        self.activity_log.clear();
        cx.notify();
    }

    /// Reset the per-session workflow state (used when entering a session).
    pub fn reset_session_state(&mut self, cx: &mut Context<Self>) {
        self.active_session.session_state = SessionState::default();
        cx.notify();
    }

    pub fn apply(&mut self, ev: HostEvent, cx: &mut Context<Self>) {
        match ev.event.as_str() {
            "app_event" => match serde_json::from_value::<WireAppEvent>(ev.payload) {
                Ok(e) => self.apply_app_event(e, cx),
                Err(e) => log::warn!("unparseable app_event: {e}"),
            },
            "local_wallet_sync_progress" => {
                if let Ok(p) = serde_json::from_value::<SyncProgress>(ev.payload) {
                    self.sync_progress.insert(p.wallet_id.clone(), p.clone());
                    cx.emit(StoreEvent::SyncProgress(p));
                    cx.notify();
                }
            }
            other => log::debug!("unhandled host event {other}"),
        }
    }

    fn apply_app_event(&mut self, e: WireAppEvent, cx: &mut Context<Self>) {
        match e {
            WireAppEvent::StateUpdate { data } => {
                if let Some(a) = data.authenticated {
                    if a != self.authenticated {
                        self.authenticated = a;
                        cx.emit(StoreEvent::AuthChanged);
                    }
                }
                if let Some(mode) = data.app_mode {
                    if Some(mode) != self.app_mode {
                        self.app_mode = Some(mode);
                        cx.emit(StoreEvent::ModeChanged);
                    }
                }
                if let Some(s) = data.active_session {
                    if let Some(c) = s.is_connected {
                        self.active_session.is_connected = c;
                    }
                    if let Some(id) = s.session_id {
                        self.active_session.session_id = Some(id);
                    }
                }
                if let Some(rs) = data.remote_sessions {
                    self.remote_sessions = rs;
                }
                if let Some(route) = data.route {
                    self.route = route;
                    // Emit even when unchanged: the React effect re-asserted
                    // the route on every state update too.
                    cx.emit(StoreEvent::RouteChanged);
                }
            }
            WireAppEvent::Session { data } => {
                let st = &mut self.active_session.session_state;
                if data.success {
                    st.session_type = Some(data.session_type);
                    st.step = Some(data.step);
                    st.requirements = Some(data.requirements);
                    st.data = data.data;
                    st.finished = Some(data.finished);
                    st.message = data.message;
                } else {
                    st.error = Some(data.message.unwrap_or_else(|| "Unknown error".into()));
                }
                cx.emit(StoreEvent::SessionUpdated);
            }
            WireAppEvent::Notification { data } => {
                self.notification = Some(data.clone());
                self.activity_log.push(data.clone());
                cx.emit(StoreEvent::Notification(data));
            }
            WireAppEvent::Command { data } => {
                if data.get("command").and_then(Value::as_str) == Some("update_remote_sessions") {
                    if let Some(sessions) = data
                        .get("payload")
                        .and_then(|p| p.get("sessions"))
                        .and_then(|s| serde_json::from_value(s.clone()).ok())
                    {
                        self.remote_sessions = sessions;
                    }
                }
            }
        }
        cx.notify();
    }
}

pub struct StoreHandle(pub Entity<AppStore>);
impl Global for StoreHandle {}

pub fn init(cx: &mut App) -> Entity<AppStore> {
    let store = cx.new(|_| AppStore::new());
    cx.set_global(StoreHandle(store.clone()));
    store
}

/// Tell local-mode views (sidebar, wallet list) that wallet data changed.
pub fn local_changed(cx: &mut App) {
    store(cx).update(cx, |_, cx| cx.emit(StoreEvent::LocalDataChanged));
}

/// The global [`AppStore`] entity.
pub fn store(cx: &App) -> Entity<AppStore> {
    cx.global::<StoreHandle>().0.clone()
}
