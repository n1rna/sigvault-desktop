//! `pages/local/ReceiveScreen.tsx` — receive screen (QBL-228).
//!
//! Renders the wallet's external keychain index 0 address as both QR and
//! monospace text. v1 always shows the same address — next-unused / per-
//! index pickers land in a later UX pass.

use gpui::{
    div, prelude::*, px, ClipboardItem, Context, Entity, SharedString, Subscription, Window,
};
use sigvault_core::local_wallet::commands::{
    cmd_local_get_receive_address, cmd_local_list_wallets, ReceiveAddress,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::wallet_unlock::{WalletUnlock, WalletUnlocked};
use super::widgets::{loading_card, network_badge, plain_error};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::{navigate, toast};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, card, divider, page_frame, QrCode, Tone};

pub struct ReceivePage {
    wallet_id: String,
    address: Option<ReceiveAddress>,
    wallet: Option<WalletSummary>,
    error: Option<SharedString>,
    loading: bool,
    unlock: Entity<WalletUnlock>,
    _sub: Subscription,
}

impl ReceivePage {
    pub fn new(wallet_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let unlock = cx.new(|cx| {
            WalletUnlock::new(
                wallet_id.clone(),
                true,
                "show a receive address",
                window,
                cx,
            )
        });
        let sub = cx.subscribe(&unlock, |this, _, _: &WalletUnlocked, cx| this.load(cx));
        let mut this = Self {
            wallet_id,
            address: None,
            wallet: None,
            error: None,
            loading: true,
            unlock,
            _sub: sub,
        };
        this.load(cx);
        this
    }

    /// The wallet is locked: show the unlock card instead of an address.
    fn locked(&self) -> bool {
        self.address.is_none() && self.wallet.as_ref().is_some_and(|w| w.locked)
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        cx.notify();
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let state = State::new(&st);
            let wallets = cmd_local_list_wallets(app.clone(), state).await;
            let wallet = wallets
                .ok()
                .and_then(|ws| ws.into_iter().find(|w| w.id.as_str() == id));
            // A locked wallet can't derive addresses; the page offers the
            // unlock card instead of the core's "not unlocked" error.
            if wallet.as_ref().is_some_and(|w| w.locked) {
                return Ok((None, wallet));
            }
            let addr = cmd_local_get_receive_address(app, state, id.clone()).await;
            Ok((Some(addr), wallet))
        });
        cx.spawn(async move |this, cx| {
            let res: Result<_, String> = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((addr, wallet)) => {
                        let hot = wallet.as_ref().is_none_or(|w| w.has_hot_keys);
                        this.unlock.update(cx, |u, cx| u.set_hot(hot, cx));
                        this.wallet = wallet;
                        match addr {
                            Some(Ok(a)) => this.address = Some(a),
                            Some(Err(e)) if e.contains("is not unlocked") => {
                                // Locked between the two calls.
                                if let Some(w) = this.wallet.as_mut() {
                                    w.locked = true;
                                }
                            }
                            Some(Err(e)) => this.error = Some(plain_error(&e).into()),
                            None => {}
                        }
                    }
                    Err(e) => this.error = Some(plain_error(&e).into()),
                }
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn copy_address(&mut self, cx: &mut Context<Self>) {
        let Some(addr) = self.address.as_ref() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(addr.address.clone()));
        toast(cx, "Address copied");
    }

    fn render_address(&self, addr: &ReceiveAddress, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let qr = card()
            .flex_none()
            .p_4()
            .items_center()
            .justify_center()
            .child(QrCode::new(addr.address.clone(), px(248.)));

        let details = card()
            .flex_1()
            .min_w(px(300.))
            .child(
                div()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Bitcoin address"),
                            )
                            .child(badge(
                                if addr.keychain.eq_ignore_ascii_case("internal") {
                                    format!("Change address #{}", addr.index + 1)
                                } else {
                                    format!("Address #{}", addr.index + 1)
                                },
                                Tone::Outline,
                            )),
                    )
                    .child(
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(t.border)
                            .bg(alpha(t.background, 0.6))
                            .px_3()
                            .py(px(10.))
                            .font_family(FONT_MONO)
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .text_color(t.foreground)
                            .child(addr.address.clone()),
                    )
                    .child(
                        div().flex().flex_row().gap_2().child(
                            button("copy-address", "Copy address")
                                .icon("copy")
                                .on_click(cx.listener(|this, _, _, cx| this.copy_address(cx))),
                        ),
                    ),
            )
            .child(divider())
            .child(
                div()
                    .px_5()
                    .py_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(self.wallet.as_ref().map(|w| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("Network")
                            .child(network_badge(&w.network))
                    }))
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .text_color(t.muted_foreground)
                            .child(
                                "Only send bitcoin on this network to this address. Reusing an address is safe but makes your payments easier to link — per-payment addresses come in a later update.",
                            ),
                    ),
            );

        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap_4()
            .child(qr)
            .child(details)
    }
}

impl Render for ReceivePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self
            .wallet
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Wallet".into());
        let wallet_page = Page::LocalWallet(self.wallet_id.clone());
        let back = wallet_page.clone();
        page_frame("receive")
            .crumb("Wallets", Some(Page::LocalWallets))
            .crumb(name.clone(), Some(wallet_page))
            .crumb("Receive", None)
            .title("Receive")
            .subtitle(format!(
                "Share this address to receive bitcoin into {name}."
            ))
            .action(
                button("done", "Done")
                    .outline()
                    .small()
                    .on_click(move |_, _, cx| navigate(back.clone(), cx)),
            )
            .when(self.loading, |f| f.child(loading_card("Loading address…")))
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .when(!self.loading && self.locked(), |f| {
                f.child(self.unlock.clone())
            })
            .children(self.address.as_ref().map(|a| self.render_address(a, cx)))
    }
}
