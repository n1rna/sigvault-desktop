//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct WalletDashboardPage {
    wallet_id: String,
}

impl WalletDashboardPage {
    pub fn new(wallet_id: String, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { wallet_id }
    }
}

impl Render for WalletDashboardPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("WalletDashboardPage", 720.)
                .child(title("Wallet"))
                .child(muted("Not ported yet.")),
        )
    }
}
