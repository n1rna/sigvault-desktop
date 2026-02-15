// Event system for backend-to-frontend communication

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::WindowApplicationRoute;
use crate::api::types::RemoteSession;

/// State update event - updates the global application state
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ActiveSession {
    // Add fields as needed
    session_id: Option<String>,
    session_type: Option<String>,
    is_connected: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateUpdateEvent {
    pub authenticated: Option<bool>,
    pub route: Option<WindowApplicationRoute>,
    pub active_session: Option<ActiveSession>,
    pub remote_sessions: Option<Vec<RemoteSession>>,
}

impl StateUpdateEvent {
    pub fn builder() -> StateUpdateEventBuilder {
        StateUpdateEventBuilder::default()
    }
}

#[derive(Default)]
pub struct StateUpdateEventBuilder {
    authenticated: Option<bool>,
    route: Option<WindowApplicationRoute>,
    active_session: ActiveSession,
    remote_sessions: Option<Vec<RemoteSession>>,
}

impl StateUpdateEventBuilder {
    pub fn route(mut self, route: WindowApplicationRoute) -> Self {
        self.route = Some(route);
        self
    }

    pub fn socket_connected(mut self, connected: bool) -> Self {
        self.active_session.is_connected = connected;
        self
    }

    pub fn session_id(mut self, id: String) -> Self {
        self.active_session.session_id = Some(id);
        self
    }

    pub fn session_type(mut self, session_type: String) -> Self {
        self.active_session.session_type = Some(session_type);
        self
    }

    pub fn remote_sessions(mut self, sessions: Vec<RemoteSession>) -> Self {
        self.remote_sessions = Some(sessions);
        self
    }

    pub fn authenticated(mut self, authenticated: bool) -> Self {
        self.authenticated = Some(authenticated);
        self
    }

    pub fn build(self) -> StateUpdateEvent {
        StateUpdateEvent {
            authenticated: self.authenticated,
            route: self.route,
            active_session: Some(self.active_session),
            remote_sessions: self.remote_sessions,
        }
    }
}

/// Command event - triggers an action in the frontend
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandEvent {
    pub command: String,
    pub payload: Value,
}

impl CommandEvent {
    pub fn new(command: impl Into<String>, payload: Value) -> Self {
        Self {
            command: command.into(),
            payload,
        }
    }
}

/// Session event - handles session-specific messages
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionEvent {
    pub step: u32,
    pub requirements: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub finished: bool,
    pub success: bool,
    pub message: Option<String>,
    pub session_type: String,
}

impl SessionEvent {
    pub fn builder() -> SessionEventBuilder {
        SessionEventBuilder::default()
    }
}

#[derive(Default)]
pub struct SessionEventBuilder {
    session_type: Option<String>,
    step: Option<u32>,
    requirements: Option<serde_json::Value>,
    data: Option<serde_json::Value>,
    finished: Option<bool>,
    success: Option<bool>,
    message: Option<String>,
}

impl SessionEventBuilder {
    pub fn requirements(mut self, requirements: serde_json::Value) -> Self {
        // You can store requirements in active_session if needed
        self.requirements = Some(requirements);
        self
    }

    pub fn data(mut self, data: Option<serde_json::Value>) -> Self {
        self.data = data;
        self
    }

    pub fn step(mut self, step: u32) -> Self {
        self.step = Some(step);
        self
    }

    pub fn session_type(mut self, session_type: String) -> Self {
        self.session_type = Some(session_type);
        self
    }

    pub fn finished(mut self, finished: bool) -> Self {
        self.finished = Some(finished);
        self
    }

    pub fn success(mut self, success: bool) -> Self {
        self.success = Some(success);
        self
    }

    pub fn message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    pub fn build(self) -> SessionEvent {
        SessionEvent {
            step: self.step.expect("step is required"),
            requirements: self.requirements.expect("requirements are required"),
            data: self.data,
            finished: self.finished.expect("finished is required"),
            success: self.success.expect("success is required"),
            message: self.message,
            session_type: self.session_type.expect("session_type is required"),
        }
    }
}

/// Notification event - shows a notification to the user
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotificationEvent {
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationEvent {
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Info,
            title: title.into(),
            message: message.into(),
            duration_ms: Some(5000),
        }
    }

    pub fn success(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Success,
            title: title.into(),
            message: message.into(),
            duration_ms: Some(5000),
        }
    }

    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Warning,
            title: title.into(),
            message: message.into(),
            duration_ms: Some(7000),
        }
    }

    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Error,
            title: title.into(),
            message: message.into(),
            duration_ms: Some(10000),
        }
    }
}

/// Unified event type for all backend-to-frontend events
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    StateUpdate { data: StateUpdateEvent },
    Command { data: CommandEvent },
    Session { data: SessionEvent },
    Notification { data: NotificationEvent },
}

impl AppEvent {
    pub fn state_update(event: StateUpdateEvent) -> Self {
        Self::StateUpdate { data: event }
    }

    pub fn command(event: CommandEvent) -> Self {
        Self::Command { data: event }
    }

    pub fn session(event: SessionEvent) -> Self {
        Self::Session { data: event }
    }

    pub fn notification(event: NotificationEvent) -> Self {
        Self::Notification { data: event }
    }
}
