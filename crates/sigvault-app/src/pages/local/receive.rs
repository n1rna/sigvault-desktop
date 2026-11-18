//! `pages/local/ReceiveScreen.tsx` — receive screen (QBL-228).
//!
//! Renders the wallet's external keychain index 0 address as both QR and
//! monospace text. v1 always shows the same address — next-unused / per-
//! index pickers land in a later UX pass.

use std::time::Duration;

use gpui::{div, prelude::*, px, ClipboardItem, Context, SharedString, Window};
use sigvault_core::local_wallet::commands::{
    cmd_local_get_receive_address, cmd_local_list_wallets, ReceiveAddress,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::widgets::{back_button, header_bar, status_box, tooltip};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, QrCode, Tone};

pub struct ReceivePage {
    wallet_id: String,
    address: Option<ReceiveAddress>,
    wallet: Option<WalletSummary>,
    error: Option<SharedString>,
    loading: bool,
    copied: bool,
    copied_epoch: usize,
}

impl ReceivePage {
    pub fn new(wallet_id: String, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let id = wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let state = State::new(&st);
            let (addr, wallets) = futures::join!(
                cmd_local_get_receive_address(app.clone(), state, id.clone()),
                cmd_local_list_wallets(app, state),
            );
            let addr = addr?;
            let wallets = wallets?;
            Ok((addr, wallets.into_iter().find(|w| w.id.as_str() == id)))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((addr, wallet)) => {
                        this.address = Some(addr);
                        this.wallet = wallet;
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            wallet_id,
            address: None,
            wallet: None,
            error: None,
            loading: true,
            copied: false,
            copied_epoch: 0,
        }
    }

    fn copy_address(&mut self, cx: &mut Context<Self>) {
        let Some(addr) = self.address.as_ref() else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(addr.address.clone()));
        self.copied = true;
        self.copied_epoch += 1;
        let epoch = self.copied_epoch;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1800)).await;
            this.update(cx, |this, cx| {
                if this.copied_epoch == epoch {
                    this.copied = false;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn render_address(&self, addr: &ReceiveAddress, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.card)
                    .p_6()
                    .child(QrCode::new(addr.address.clone(), px(256.))),
            )
            .child(
                div()
                    .id("copy-address")
                    .group("copy-address")
                    .w_full()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.card)
                    .p_4()
                    .cursor_pointer()
                    .hover(move |s| s.border_color(alpha(t.primary, 0.5)))
                    .tooltip(tooltip("Click to copy"))
                    .on_click(cx.listener(|this, _, _, cx| this.copy_address(cx)))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .items_start()
                                    .child(badge(
                                        format!("{} · index {}", addr.keychain, addr.index),
                                        Tone::Outline,
                                    ))
                                    .child(
                                        div()
                                            .mt_2()
                                            .w_full()
                                            .font_family(FONT_MONO)
                                            .text_size(px(12.))
                                            .line_height(px(20.))
                                            .text_color(t.foreground)
                                            .child(addr.address.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .group_hover("copy-address", move |s| s.text_color(t.primary))
                                    .child(if self.copied { "✓ Copied" } else { "Copy" }),
                            ),
                    ),
            )
            .child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(alpha(t.card, 0.4))
                    .px_4()
                    .py_3()
                    .child(
                        div()
                            .text_size(px(11.))
                            .line_height(px(18.))
                            .text_color(t.muted_foreground)
                            .child(
                                "This address can be reused safely but each reuse weakens your privacy. Per-index and next-unused address pickers ship in a later UX pass.",
                            ),
                    ),
            )
    }
}

impl Render for ReceivePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let back_id = self.wallet_id.clone();
        let header = header_bar().justify_start().child(back_button(
            "back",
            "Back to wallet",
            move |_, _, cx| navigate(Page::LocalWallet(back_id.clone()), cx),
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.foreground)
                        .child("Receive"),
                )
                .children(self.wallet.as_ref().map(|w| {
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(format!("{} · {}", w.name, w.network))
                })),
        );

        let address = self.address.as_ref().map(|a| self.render_address(a, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("receive-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div().w_full().flex().flex_col().items_center().px_6().py_8().child(
                            div()
                                .w_full()
                                .max_w(px(448.))
                                .flex()
                                .flex_col()
                                .when(self.loading, |d| d.child(status_box("Loading address…")))
                                .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                                .children(address),
                        ),
                    ),
            )
    }
}
