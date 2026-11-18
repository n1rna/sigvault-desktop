//! TODO(cloud agent): Cloud workspace settings — environment, account, about.

use gpui::{prelude::*, Context, Window};

use crate::ui::{muted, page_frame};

pub struct CloudSettingsPage;

impl CloudSettingsPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for CloudSettingsPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        page_frame("cloud-settings").title("Settings").child(muted("Not implemented yet."))
    }
}
