//! TODO(keys agent)

use gpui::{prelude::*, Context, Window};

use crate::ui::{muted, page_frame};

pub struct KeyDetailPage {
    key_id: String,
}

impl KeyDetailPage {
    pub fn new(key_id: String, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { key_id }
    }
}

impl Render for KeyDetailPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        page_frame("key-detail").title("Key").child(muted(self.key_id.clone()))
    }
}
