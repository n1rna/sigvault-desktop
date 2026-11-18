//! `pages/local/LocalWalletList.tsx` — local wallet list (QBL-222).
//!
//! Lists all on-disk wallets discovered by `cmd_local_list_wallets`,
//! supports unlock / lock / delete, and routes to the create wizard and
//! per-wallet dashboard. Once unlocked, opening a wallet auto-syncs in the
//! background; the dashboard renders the progress.

use gpui::{div, prelude::*, px, ClickEvent, Context, Entity, SharedString, Subscription, Window};
use sigvault_core::local_wallet::commands::{
    cmd_local_delete_wallet, cmd_local_list_wallets, cmd_local_lock_wallet,
    cmd_local_unlock_wallet, DeleteWalletRequest, UnlockWalletRequest,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::widgets::{sep_dot, status_box, tooltip};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, icon, icon_button, modal, InputEvent, TextInput, Tone};

fn format_network(n: &str) -> String {
    n.to_uppercase()
}

fn format_policy_type(t: &str) -> String {
    match t {
        "singlesig" | "singlesig_hot" => "Single key".into(),
        "singlesig_hardware" => "Single key · hardware".into(),
        "descriptor" => "Descriptor".into(),
        "multisig" => "Multisig".into(),
        "timelocked" => "Timelocked policy".into(),
        "watch_only" => "Watch-only".into(),
        other => other.into(),
    }
}

fn format_created_at(unix: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%b %-d, %Y").to_string(),
        _ => "—".into(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalKind {
    Unlock,
    Delete,
}

struct ModalState {
    kind: ModalKind,
    wallet: WalletSummary,
    submitting: bool,
    error: Option<SharedString>,
}

pub struct LocalWalletListPage {
    wallets: Vec<WalletSummary>,
    loading: bool,
    error: Option<SharedString>,
    modal: Option<ModalState>,
    unlock_pass: Entity<TextInput>,
    delete_name: Entity<TextInput>,
    delete_pass: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl LocalWalletListPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let unlock_pass = cx.new(|cx| TextInput::new(window, cx).masked());
        let delete_name = cx.new(|cx| TextInput::new(window, cx));
        let delete_pass = cx.new(|cx| TextInput::new(window, cx).masked());

        let mut subs = Vec::new();
        subs.push(cx.subscribe(&unlock_pass, |this, _, ev: &InputEvent, cx| match ev {
            InputEvent::Changed => cx.notify(),
            InputEvent::Submit => this.submit_unlock(cx),
        }));
        for input in [&delete_name, &delete_pass] {
            subs.push(cx.subscribe(input, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Changed => cx.notify(),
                InputEvent::Submit => this.submit_delete(cx),
            }));
        }

        let mut this = Self {
            wallets: Vec::new(),
            loading: true,
            error: None,
            modal: None,
            unlock_pass,
            delete_name,
            delete_pass,
            _subs: subs,
        };
        this.load_wallets(cx);
        this
    }

    fn load_wallets(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        cx.notify();
        let task = backend(cx).call(|app, st| async move {
            cmd_local_list_wallets(app, State::new(&st)).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(mut list) => {
                        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                        this.wallets = list;
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_wallet(&mut self, wallet: WalletSummary, window: &mut Window, cx: &mut Context<Self>) {
        let id = wallet.id.as_str().to_string();
        if wallet.locked && wallet.has_hot_keys {
            // Hot wallets need the passphrase to decrypt seed.enc.
            self.unlock_pass.update(cx, |i, cx| i.clear(cx));
            self.unlock_pass.read(cx).focus(window);
            self.modal = Some(ModalState {
                kind: ModalKind::Unlock,
                wallet,
                submitting: false,
                error: None,
            });
            cx.notify();
            return;
        }
        if !wallet.locked {
            navigate(Page::LocalWallet(id), cx);
            return;
        }
        // HW / watch-only wallets have no on-disk seed; unlock just loads
        // the BDK store + kicks off auto-sync. Empty passphrase.
        let wallet_id = id.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_unlock_wallet(
                app,
                State::new(&st),
                UnlockWalletRequest { wallet_id, passphrase: String::new() },
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(()) => navigate(Page::LocalWallet(id), cx),
                Err(e) => {
                    this.error = Some(e.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn lock_wallet(&mut self, wallet_id: String, cx: &mut Context<Self>) {
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_lock_wallet(app, State::new(&st), wallet_id).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(_) => this.load_wallets(cx),
                Err(e) => {
                    this.error = Some(e.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn open_delete(&mut self, wallet: WalletSummary, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_name.update(cx, |i, cx| {
            i.clear(cx);
            i.set_placeholder(wallet.name.clone(), cx);
        });
        self.delete_pass.update(cx, |i, cx| i.clear(cx));
        self.delete_name.read(cx).focus(window);
        self.modal = Some(ModalState {
            kind: ModalKind::Delete,
            wallet,
            submitting: false,
            error: None,
        });
        cx.notify();
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        cx.notify();
    }

    fn submit_unlock(&mut self, cx: &mut Context<Self>) {
        let passphrase = self.unlock_pass.read(cx).text().to_string();
        let Some(m) = self.modal.as_mut() else { return };
        if m.kind != ModalKind::Unlock || m.submitting || passphrase.is_empty() {
            return;
        }
        m.submitting = true;
        m.error = None;
        let id = m.wallet.id.as_str().to_string();
        cx.notify();
        let wallet_id = id.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_unlock_wallet(app, State::new(&st), UnlockWalletRequest { wallet_id, passphrase })
                .await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => {
                        this.modal = None;
                        this.load_wallets(cx);
                        navigate(Page::LocalWallet(id), cx);
                    }
                    Err(e) => {
                        if let Some(m) = this.modal.as_mut() {
                            m.error = Some(e.into());
                            m.submitting = false;
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn delete_ready(&self, cx: &Context<Self>) -> bool {
        let Some(m) = self.modal.as_ref() else { return false };
        let name_ok = self.delete_name.read(cx).text() == m.wallet.name;
        let pass_ok = !m.wallet.has_hot_keys || !self.delete_pass.read(cx).text().is_empty();
        name_ok && pass_ok
    }

    fn submit_delete(&mut self, cx: &mut Context<Self>) {
        if !self.delete_ready(cx) {
            return;
        }
        let passphrase = self.delete_pass.read(cx).text().to_string();
        let Some(m) = self.modal.as_mut() else { return };
        if m.kind != ModalKind::Delete || m.submitting {
            return;
        }
        m.submitting = true;
        m.error = None;
        let request = DeleteWalletRequest {
            wallet_id: m.wallet.id.as_str().to_string(),
            passphrase: m.wallet.has_hot_keys.then_some(passphrase),
        };
        cx.notify();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_delete_wallet(app, State::new(&st), request).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => {
                        this.modal = None;
                        this.load_wallets(cx);
                    }
                    Err(e) => {
                        if let Some(m) = this.modal.as_mut() {
                            m.error = Some(e.into());
                            m.submitting = false;
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── rendering ──

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(20.))
                            .line_height(px(26.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child("Local wallets"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(13.))
                            .text_color(t.muted_foreground)
                            .child("On-device wallets. Your seed never leaves this machine."),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div().id("settings-wrap").tooltip(tooltip("Settings")).child(
                            icon_button("settings", "settings")
                                .outline()
                                .on_click(|_, _, cx| navigate(Page::LocalSettings, cx)),
                        ),
                    )
                    .child(
                        button("refresh", "Refresh")
                            .outline()
                            .small()
                            .disabled(self.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.load_wallets(cx))),
                    )
                    .child(
                        button("recover", "Recover")
                            .outline()
                            .small()
                            .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx)),
                    )
                    .child(
                        button("new-wallet", "New wallet")
                            .small()
                            .icon("plus")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                    ),
            )
    }

    fn render_empty(&self) -> impl IntoElement {
        let t = theme();
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .rounded_lg()
            .border_1()
            .border_dashed()
            .border_color(t.border)
            .bg(alpha(t.card, 0.4))
            .px_10()
            .py_16()
            .child(
                div()
                    .size(px(48.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.background)
                    .child(icon("wallet-card").size(px(20.)).text_color(t.muted_foreground)),
            )
            .child(
                div()
                    .mt_5()
                    .text_size(px(16.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.foreground)
                    .child("No wallets yet"),
            )
            .child(
                div()
                    .mt_2()
                    .max_w(px(384.))
                    .text_center()
                    .text_size(px(13.))
                    .line_height(px(21.))
                    .text_color(t.muted_foreground)
                    .child(
                        "Create your first local wallet to start receiving and signing transactions on-device. Your seed never leaves this machine.",
                    ),
            )
            .child(
                div().mt_6().child(
                    button("create-wallet", "Create wallet")
                        .icon("plus")
                        .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                ),
            )
    }

    fn render_card(&self, ix: usize, w: &WalletSummary, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let group: SharedString = format!("wallet-card-{ix}").into();
        let fp = w.fingerprints.first().cloned();

        let lock_icon = if w.locked {
            icon("lock").size(px(14.)).text_color(t.muted_foreground)
        } else {
            icon("unlock").size(px(14.)).text_color(t.success)
        };

        let mut meta = div()
            .mt_1()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .text_size(px(12.))
            .text_color(t.muted_foreground)
            .child(format_policy_type(&w.policy_type));
        if let Some(fp) = fp {
            meta = meta.child(sep_dot()).child(div().font_family(FONT_MONO).child(fp));
        }
        meta = meta.child(sep_dot()).child(format_created_at(w.created_at));

        let open_wallet = w.clone();
        let row = div()
            .id(("wallet-open", ix))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_4()
            .py(px(14.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_wallet(open_wallet.clone(), window, cx)
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(14.))
                    .min_w_0()
                    .child(
                        div()
                            .size(px(36.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .border_1()
                            .border_color(t.border)
                            .bg(t.background)
                            .child(lock_icon),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(t.foreground)
                                            .child(w.name.clone()),
                                    )
                                    .child(badge(format_network(&w.network), Tone::Outline))
                                    .when(w.recovery_only, |d| {
                                        d.child(badge("Recovery-only", Tone::Warning))
                                    }),
                            )
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .group_hover(group.clone(), |s| s.ml(px(2.)))
                    .child(icon("chevron-right").size(px(16.)).text_color(t.muted_foreground)),
            );

        // Hover-revealed actions, painted over the row. Their click handlers
        // stop propagation so the row's open handler doesn't also fire.
        let mut actions = div().flex().flex_row().items_center().gap_1();
        if !w.locked {
            let id = w.id.as_str().to_string();
            actions = actions.child(
                action_icon(("wallet-lock", ix), "lock", "Lock wallet", false).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.lock_wallet(id.clone(), cx);
                    }),
                ),
            );
        }
        let del_wallet = w.clone();
        actions = actions.child(
            action_icon(("wallet-delete", ix), "trash", "Delete wallet", true).on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_delete(del_wallet.clone(), window, cx);
                }),
            ),
        );

        div()
            .id(("wallet-card", ix))
            .group(group.clone())
            .relative()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .hover(move |s| s.border_color(alpha(t.primary, 0.5)))
            .child(row)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(44.))
                    .flex()
                    .items_center()
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(1.))
                    .child(actions),
            )
    }

    fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let m = self.modal.as_ref()?;
        let t = theme();
        let (title, modal_id) = match m.kind {
            ModalKind::Unlock => ("Unlock wallet", "unlock-modal"),
            ModalKind::Delete => ("Delete wallet", "delete-modal"),
        };

        let header = div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(m.wallet.name.clone()),
                    ),
            )
            .child(
                div()
                    .id("modal-close")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.muted))
                    .on_click(cx.listener(|this, _, _, cx| this.close_modal(cx)))
                    .child(icon("x").size(px(14.)).text_color(t.muted_foreground)),
            );

        let error = m.error.clone().map(|e| {
            div()
                .rounded_md()
                .border_1()
                .border_color(alpha(t.destructive, 0.3))
                .bg(alpha(t.destructive, 0.06))
                .px_3()
                .py_2()
                .text_size(px(12.))
                .text_color(t.destructive)
                .child(e)
        });

        let body = match m.kind {
            ModalKind::Unlock => {
                let empty = self.unlock_pass.read(cx).text().is_empty();
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(input_label("Passphrase", self.unlock_pass.clone()))
                    .children(error)
                    .child(
                        button("unlock-submit", if m.submitting { "Unlocking…" } else { "Unlock" })
                            .large()
                            .full_width()
                            .disabled(m.submitting || empty)
                            .on_click(cx.listener(|this, _, _, cx| this.submit_unlock(cx))),
                    )
            }
            ModalKind::Delete => {
                let ready = self.delete_ready(cx);
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(19.))
                            .text_color(t.muted_foreground)
                            .child(
                                "This permanently removes the on-disk wallet, including the encrypted seed. Recovery requires the original BIP39 mnemonic. Type the wallet name to confirm.",
                            ),
                    )
                    .child(input_label("Confirm name", self.delete_name.clone()))
                    .when(m.wallet.has_hot_keys, |d| {
                        d.child(input_label("Passphrase", self.delete_pass.clone()))
                    })
                    .children(error)
                    .child(
                        button(
                            "delete-submit",
                            if m.submitting { "Deleting…" } else { "Delete wallet" },
                        )
                        .destructive()
                        .large()
                        .full_width()
                        .disabled(m.submitting || !ready)
                        .on_click(cx.listener(|this, _, _, cx| this.submit_delete(cx))),
                    )
            }
        };

        Some(
            modal(modal_id, 448.)
                .on_dismiss(cx.listener(|this, _, _, cx| this.close_modal(cx)))
                .child(div().flex().flex_col().gap_1().child(header).child(div().mt_4().child(body))),
        )
    }
}

fn input_label(text: &'static str, input: Entity<TextInput>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(12.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme().muted_foreground)
                .child(text),
        )
        .child(input)
}

fn action_icon(
    id: impl Into<gpui::ElementId>,
    icon_name: &'static str,
    title: &'static str,
    destructive: bool,
) -> gpui::Stateful<gpui::Div> {
    let t = theme();
    let hover_fg = if destructive { t.destructive } else { t.foreground };
    let hover_bg = if destructive { alpha(t.destructive, 0.1) } else { t.muted };
    let group: SharedString = format!("action-{icon_name}").into();
    div()
        .id(id)
        .group(group.clone())
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .tooltip(tooltip(title))
        .child(
            icon(icon_name)
                .size(px(14.))
                .text_color(t.muted_foreground)
                .group_hover(group, move |s| s.text_color(hover_fg)),
        )
}

impl Render for LocalWalletListPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.loading {
            status_box("Loading wallets…").rounded_lg().into_any_element()
        } else if self.wallets.is_empty() {
            self.render_empty().into_any_element()
        } else {
            let cards: Vec<_> = self
                .wallets
                .iter()
                .enumerate()
                .map(|(ix, w)| self.render_card(ix, w, cx).into_any_element())
                .collect();
            div().flex().flex_col().gap(px(10.)).children(cards).into_any_element()
        };

        div()
            .size_full()
            .relative()
            .child(
                div().id("local-wallets-scroll").size_full().overflow_y_scroll().child(
                    div().w_full().flex().flex_col().items_center().px_10().py(px(36.)).child(
                        div()
                            .w_full()
                            .max_w(px(768.))
                            .flex()
                            .flex_col()
                            .gap_6()
                            .child(self.render_header(cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .children(self.error.clone().map(|e| {
                                        div().mb_4().child(alert(Tone::Destructive, e))
                                    }))
                                    .child(body),
                            ),
                    ),
                ),
            )
            .children(self.render_modal(cx))
    }
}
