//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct RecoveryFlowPage {
}

impl RecoveryFlowPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {  }
    }
}

impl Render for RecoveryFlowPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("RecoveryFlowPage", 720.)
                .child(title("Recover wallet"))
                .child(muted("Not ported yet.")),
        )
    }
}
