//! TODO(port): placeholder until the page is ported.

use gpui::{div, prelude::*, Context, Window};

use crate::ui::{muted, page_scroll, title};

pub struct CreateWalletWizardPage {
}

impl CreateWalletWizardPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {  }
    }
}

impl Render for CreateWalletWizardPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            page_scroll("CreateWalletWizardPage", 720.)
                .child(title("Create wallet"))
                .child(muted("Not ported yet.")),
        )
    }
}
