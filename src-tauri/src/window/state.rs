// Window state management

use std::sync::Arc;
use tokio::sync::Mutex;

use super::message_queue::MessageQueue;
use super::types::WindowApplicationRoute;

pub struct WindowState {
    pub message_queue: MessageQueue,
    pub is_processing: bool,
    pub current_route: WindowApplicationRoute,
}

impl WindowState {
    pub fn new() -> Self {
        Self {
            message_queue: MessageQueue::new(),
            is_processing: false,
            current_route: WindowApplicationRoute::Loading,
        }
    }

    pub fn set_route(&mut self, route: WindowApplicationRoute) {
        self.current_route = route;
    }

    pub fn get_route(&self) -> &WindowApplicationRoute {
        &self.current_route
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedWindowState = Arc<Mutex<WindowState>>;

pub fn create_shared_window_state() -> SharedWindowState {
    Arc::new(Mutex::new(WindowState::new()))
}
