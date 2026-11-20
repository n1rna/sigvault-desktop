//! Inline "This wallet is locked" card: passphrase (for wallets with keys
//! on this computer) and an Unlock / Open wallet button.
//!
//! Shared by the wallet dashboard and the pages under it (Receive) so a
//! locked wallet always offers the same way in instead of an error.
//! Emits [`WalletUnlocked`] once the core reports the wallet unlocked.

use gpui::{
    div, prelude::*, px, Context, Entity, EventEmitter, SharedString, Subscription, Window,
};
use sigvault_core::local_wallet::commands::{cmd_local_unlock_wallet, UnlockWalletRequest};
use sigvault_core::State;

use super::widgets::{plain_error, row_icon};
use crate::runtime::backend;
use crate::state::local_changed;
use crate::theme::theme;
use crate::ui::{alert, button, card, field, InputEvent, TextInput, Tone, FORM_WIDTH};

/// The wallet is unlocked; reload whatever needed it.
pub struct WalletUnlocked;

pub struct WalletUnlock {
    wallet_id: String,
    /// The wallet holds keys on this computer (needs the passphrase).
    hot: bool,
    /// What unlocking is for, e.g. "load balances and activity".
    purpose: &'static str,
    passphrase: Entity<TextInput>,
    unlocking: bool,
    error: Option<SharedString>,
    _sub: Subscription,
}

impl EventEmitter<WalletUnlocked> for WalletUnlock {}

impl WalletUnlock {
    pub fn new(
        wallet_id: String,
        hot: bool,
        purpose: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let passphrase = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Wallet passphrase")
        });
        let sub = cx.subscribe(&passphrase, |this, _, ev: &InputEvent, cx| match ev {
            InputEvent::Changed => {
                this.error = None;
                cx.notify();
            }
            InputEvent::Submit => this.unlock(cx),
        });
        Self {
            wallet_id,
            hot,
            purpose,
            passphrase,
            unlocking: false,
            error: None,
            _sub: sub,
        }
    }

    pub fn set_hot(&mut self, hot: bool, cx: &mut Context<Self>) {
        if self.hot != hot {
            self.hot = hot;
            cx.notify();
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &Context<Self>) {
        if self.hot {
            self.passphrase.read(cx).focus(window);
        }
    }

    fn unlock(&mut self, cx: &mut Context<Self>) {
        let passphrase = self.passphrase.read(cx).text().to_string();
        if self.unlocking || (self.hot && passphrase.is_empty()) {
            return;
        }
        self.unlocking = true;
        self.error = None;
        cx.notify();
        let wallet_id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_unlock_wallet(
                app,
                State::new(&st),
                UnlockWalletRequest {
                    wallet_id,
                    passphrase,
                },
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.unlocking = false;
                match res {
                    Ok(()) => {
                        this.passphrase.update(cx, |p, cx| p.clear(cx));
                        local_changed(cx);
                        cx.emit(WalletUnlocked);
                    }
                    Err(e) => this.error = Some(plain_error(&e).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for WalletUnlock {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let hot = self.hot;
        let empty = self.passphrase.read(cx).text().is_empty();
        div().w_full().max_w(FORM_WIDTH).child(
            card()
                .p_5()
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_3()
                        .child(row_icon("lock", t.muted_foreground, false))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child("This wallet is locked"),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .line_height(px(19.))
                                        .text_color(t.muted_foreground)
                                        .child(if hot {
                                            format!("Enter the passphrase to {}. Keys stay decrypted in memory until you lock it again.", self.purpose)
                                        } else {
                                            format!("Open the wallet to {}. Signing happens on your hardware device.", self.purpose)
                                        }),
                                ),
                        ),
                )
                .when(hot, |d| {
                    d.child(field("Passphrase", self.passphrase.clone(), None))
                })
                .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                .child(
                    div().flex().flex_row().justify_end().child(
                        button("unlock", if hot { "Unlock" } else { "Open wallet" })
                            .icon("unlock")
                            .loading(self.unlocking)
                            .disabled(hot && empty)
                            .on_click(cx.listener(|this, _, _, cx| this.unlock(cx))),
                    ),
                ),
        )
    }
}
