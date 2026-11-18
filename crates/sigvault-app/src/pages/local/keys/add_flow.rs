//! Embeddable "add a key to the library" flow.
//!
//! INTERFACE (stable — the wallet wizard embeds this in a modal):
//! - `AddKeyFlow::new(network: Option<String>, window, cx)` — `Some(net)`
//!   preselects and locks the network (wizard knows the wallet's network);
//!   `None` lets the user choose.
//! - emits [`AddKeyEvent::Added(key_id)`] once the key is saved in the
//!   keychain, or [`AddKeyEvent::Cancelled`].
//! - renders only its body (no page frame); hosts supply the frame/modal.
//!
//! TODO(keys agent): implement the source chooser and per-source flows.

use gpui::{div, prelude::*, Context, EventEmitter, Window};

use crate::ui::muted;

#[derive(Debug, Clone)]
pub enum AddKeyEvent {
    Added(String),
    Cancelled,
}

pub struct AddKeyFlow {
    network: Option<String>,
}

impl EventEmitter<AddKeyEvent> for AddKeyFlow {}

impl AddKeyFlow {
    pub fn new(network: Option<String>, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { network }
    }
}

impl Render for AddKeyFlow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(muted(format!("Add key flow (network: {:?}) — not implemented yet", self.network)))
    }
}
