//! TODO(keys agent)

use gpui::{prelude::*, Context, Window};

use crate::ui::{muted, page_frame};

pub struct KeyListPage;

impl KeyListPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for KeyListPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        page_frame("list").title("Key library").child(muted("Not implemented yet."))
    }
}
