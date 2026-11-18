//! TODO(keys agent)

use gpui::{prelude::*, Context, Window};

use crate::ui::{muted, page_frame};

pub struct AddKeyPage;

impl AddKeyPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for AddKeyPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        page_frame("add").title("Add key").child(muted("Not implemented yet."))
    }
}
