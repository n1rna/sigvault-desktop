//! `pages/Loading.tsx` — shown until `cmd_initialize_app` routes onward.

use gpui::{div, prelude::*, px, Context, Window};

use crate::theme::theme;
use crate::ui::{logo, spinner};

pub struct LoadingPage;

impl LoadingPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for LoadingPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .relative()
                            .mb_6()
                            .size(px(56.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .border_1()
                            .border_color(t.border)
                            .bg(t.card)
                            .child(logo(px(30.))),
                    )
                    .child(spinner(px(18.)))
                    .child(
                        div()
                            .mt_3()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("SigVault"),
                    )
                    .child(
                        div()
                            .mt(px(6.))
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("Connecting to coordinator…"),
                    ),
            )
    }
}
