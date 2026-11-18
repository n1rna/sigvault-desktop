//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct SendPage {
    wallet_id: String,
}

impl SendPage {
    pub fn new(wallet_id: String, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { wallet_id }
    }
}

impl Render for SendPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("SendPage", 720.)
                .child(title("Send"))
                .child(muted("Not ported yet.")),
        )
    }
}
