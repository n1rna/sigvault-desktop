// Event system for backend-to-frontend communication

use serde::{Deserialize, Serialize};

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

    pub fn authenticated(mut self, authenticated: bool) -> Self {
        self.authenticated = Some(authenticated);
        self
    }

    pub fn build(self) -> StateUpdateEvent {
        StateUpdateEvent {
            authenticated: self.authenticated,
            route: self.route,
            active_session: Some(self.active_session),
            remote_sessions: None,
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

/// Unified event type for all backend-to-frontend events
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotificationEvent {
    pub title: String,
    pub message: String,
    pub level: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    StateUpdate { data: StateUpdateEvent },
    Session { data: SessionEvent },
    Notification { data: NotificationEvent },
}

impl AppEvent {
    pub fn state_update(event: StateUpdateEvent) -> Self {
        Self::StateUpdate { data: event }
    }

    pub fn session(event: SessionEvent) -> Self {
        Self::Session { data: event }
    }

    pub fn notification(title: &str, message: &str, level: &str) -> Self {
        Self::Notification {
            data: NotificationEvent {
                title: title.to_string(),
                message: message.to_string(),
                level: level.to_string(),
            },
        }
    }
}
