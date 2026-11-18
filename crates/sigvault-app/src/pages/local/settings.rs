//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct LocalSettingsPage {
}

impl LocalSettingsPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {  }
    }
}

impl Render for LocalSettingsPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("LocalSettingsPage", 720.)
                .child(title("Settings"))
                .child(muted("Not ported yet.")),
        )
    }
}
