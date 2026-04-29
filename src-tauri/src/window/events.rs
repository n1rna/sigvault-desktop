// Event system for backend-to-frontend communication

use serde::{Deserialize, Serialize};

use super::types::WindowApplicationRoute;
use crate::api::types::RemoteSession;
use crate::app_mode::AppMode;

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
    /// Top-level mode. `Some(Cloud)` / `Some(Local)` carries an explicit
    /// update; `None` here just means "no change" — the frontend keeps
    /// whatever it had. To clear the mode (return to chooser), the backend
    /// sends a `route: ModeChooser` update; the frontend resets `appMode`
    /// to null when it sees that route.
    pub app_mode: Option<AppMode>,
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
    app_mode: Option<AppMode>,
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

    pub fn app_mode(mut self, mode: AppMode) -> Self {
        self.app_mode = Some(mode);
        self
    }

    pub fn build(self) -> StateUpdateEvent {
        StateUpdateEvent {
            authenticated: self.authenticated,
            route: self.route,
            active_session: Some(self.active_session),
            remote_sessions: None,
            app_mode: self.app_mode,
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

    pub fn build(self) -> Result<SessionEvent, &'static str> {
        Ok(SessionEvent {
            step: self.step.ok_or("step is required")?,
            requirements: self.requirements.ok_or("requirements are required")?,
            data: self.data,
            finished: self.finished.ok_or("finished is required")?,
            success: self.success.ok_or("success is required")?,
            message: self.message,
            session_type: self.session_type.ok_or("session_type is required")?,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_event_builder_success() {
        let event = SessionEvent::builder()
            .step(1)
            .requirements(serde_json::json!({"network": "testnet"}))
            .finished(false)
            .success(true)
            .session_type("DEVICE_REGISTRATION".to_string())
            .build();

        assert!(event.is_ok());
        let event = event.unwrap();
        assert_eq!(event.step, 1);
        assert!(!event.finished);
        assert!(event.success);
        assert_eq!(event.session_type, "DEVICE_REGISTRATION");
        assert!(event.data.is_none());
        assert!(event.message.is_none());
    }

    #[test]
    fn test_session_event_builder_missing_required_field() {
        let result = SessionEvent::builder()
            .step(1)
            .requirements(serde_json::json!({}))
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_session_event_builder_with_all_fields() {
        let event = SessionEvent::builder()
            .step(2)
            .requirements(serde_json::json!({"network": "mainnet"}))
            .data(Some(serde_json::json!({"tx": "abc123"})))
            .finished(true)
            .success(true)
            .session_type("TRANSACTION_SIGNING".to_string())
            .message("Done".to_string())
            .build()
            .unwrap();

        assert_eq!(event.step, 2);
        assert!(event.finished);
        assert_eq!(event.message, Some("Done".to_string()));
        assert!(event.data.is_some());
    }

    #[test]
    fn test_state_update_event_builder() {
        let event = StateUpdateEvent::builder()
            .route(WindowApplicationRoute::MainPage)
            .authenticated(true)
            .build();

        assert_eq!(event.route, Some(WindowApplicationRoute::MainPage));
        assert_eq!(event.authenticated, Some(true));
    }

    #[test]
    fn test_app_event_serialization() {
        let event = AppEvent::notification("Test", "Hello", "info");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"notification\""));
        assert!(json.contains("\"title\":\"Test\""));
    }
}
