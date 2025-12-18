// Message queue for window events

use std::collections::VecDeque;
use uuid::Uuid;

use super::types::{QueuedWindowMessage, WindowEventMessage};

pub struct MessageQueue {
    queue: VecDeque<QueuedWindowMessage>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn push(&mut self, message: WindowEventMessage) -> String {
        let id = Uuid::new_v4().to_string();
        let queued_message = QueuedWindowMessage {
            id: id.clone(),
            message,
        };
        self.queue.push_back(queued_message);
        id
    }

    pub fn pop(&mut self) -> Option<QueuedWindowMessage> {
        self.queue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}
