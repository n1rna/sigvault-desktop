//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct ReceivePage {
    wallet_id: String,
}

impl ReceivePage {
    pub fn new(wallet_id: String, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { wallet_id }
    }
}

impl Render for ReceivePage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("ReceivePage", 720.)
                .child(title("Receive"))
                .child(muted("Not ported yet.")),
        )
    }
}
