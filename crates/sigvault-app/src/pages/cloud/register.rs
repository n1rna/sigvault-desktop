//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct MachineRegistrationPage {
}

impl MachineRegistrationPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {  }
    }
}

impl Render for MachineRegistrationPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("MachineRegistrationPage", 720.)
                .child(title("Machine registration"))
                .child(muted("Not ported yet.")),
        )
    }
}
