//! `pages/local/LocalWalletList.tsx` — "All wallets" (QBL-222).
//!
//! Lists all on-disk wallets discovered by `cmd_local_list_wallets` in one
//! table-like card, supports unlock / lock / delete, and routes to the
//! create wizard, restore flow and per-wallet dashboard. Refreshes itself
//! when wallet data changes elsewhere (sidebar, dashboard, sync).

use std::collections::HashMap;

use gpui::{div, prelude::*, px, ClickEvent, Context, Entity, SharedString, Subscription, Window};
use sigvault_core::local_wallet::commands::{
    cmd_local_delete_wallet, cmd_local_get_balance, cmd_local_list_wallets, cmd_local_lock_wallet,
    cmd_local_unlock_wallet, DeleteWalletRequest, UnlockWalletRequest,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::widgets::{
    btc_amount, dialog_actions, dialog_title, format_date, hover_action, hover_actions, list_card,
    list_header, list_row, loading_card, network_badge, policy_label, row_icon, sep_dot, tooltip,
};
use crate::e2e::{E2eClick, Probe};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{local_changed, store, StoreEvent};
use crate::theme::{theme, FONT_MONO};
use crate::ui::{
    alert, button, card, divider, empty_state, field, modal, page_frame, InputEvent, TextInput,
    Tone,
};

const COL_NETWORK: f32 = 112.;
const COL_CREATED: f32 = 104.;
const COL_BALANCE: f32 = 150.;
const COL_ACTIONS: f32 = 84.;

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
    /// Confirmed + pending sats for unlocked wallets.
    balances: HashMap<String, u64>,
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
        let unlock_pass = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Wallet passphrase")
        });
        let delete_name =
            cx.new(|cx| TextInput::new(window, cx).label("Type the wallet name to confirm"));
        let delete_pass = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Wallet passphrase")
        });

        let mut subs = Vec::new();
        subs.push(
            cx.subscribe(&unlock_pass, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Changed => cx.notify(),
                InputEvent::Submit => this.submit_unlock(cx),
            }),
        );
        for input in [&delete_name, &delete_pass] {
            subs.push(
                cx.subscribe(input, |this, _, ev: &InputEvent, cx| match ev {
                    InputEvent::Changed => cx.notify(),
                    InputEvent::Submit => this.submit_delete(cx),
                }),
            );
        }
        // Auto refresh: wallet data changed elsewhere, or a sync finished.
        subs.push(
            cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| match ev {
                StoreEvent::LocalDataChanged => this.load_wallets(cx),
                StoreEvent::SyncProgress(p) if p.phase == "complete" => this.load_wallets(cx),
                _ => {}
            }),
        );

        let mut this = Self {
            wallets: Vec::new(),
            balances: HashMap::new(),
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
        let task = backend(cx).call(|app, st| async move {
            let mut list = cmd_local_list_wallets(app.clone(), State::new(&st)).await?;
            list.sort_by_key(|a| a.name.to_lowercase());
            // Balances are an in-memory read on unlocked wallets — cheap.
            let mut balances = HashMap::new();
            for w in list.iter().filter(|w| !w.locked) {
                let id = w.id.as_str().to_string();
                if let Ok(b) = cmd_local_get_balance(app.clone(), State::new(&st), id.clone()).await
                {
                    balances.insert(id, b.confirmed_sat + b.unconfirmed_sat);
                }
            }
            Ok((list, balances))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((list, balances)) => {
                        this.wallets = list;
                        this.balances = balances;
                        this.error = None;
                    }
                    Err(e) => this.error = Some(super::widgets::plain_error(&e).into()),
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
                UnlockWalletRequest {
                    wallet_id,
                    passphrase: String::new(),
                },
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(()) => {
                    local_changed(cx);
                    navigate(Page::LocalWallet(id), cx);
                }
                Err(e) => {
                    this.error = Some(super::widgets::plain_error(&e).into());
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
                // `local_changed` reloads this page through the store subscription.
                Ok(_) => local_changed(cx),
                Err(e) => {
                    this.error = Some(super::widgets::plain_error(&e).into());
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
        if self.modal.as_ref().is_some_and(|m| m.submitting) {
            return;
        }
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
                match res {
                    Ok(()) => {
                        this.modal = None;
                        local_changed(cx);
                        navigate(Page::LocalWallet(id), cx);
                    }
                    Err(e) => {
                        if let Some(m) = this.modal.as_mut() {
                            m.error = Some(super::widgets::plain_error(&e).into());
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
        let Some(m) = self.modal.as_ref() else {
            return false;
        };
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
        let name = m.wallet.name.clone();
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
                        local_changed(cx);
                        crate::shell::toast(cx, format!("Deleted “{name}”"));
                    }
                    Err(e) => {
                        if let Some(m) = this.modal.as_mut() {
                            m.error = Some(super::widgets::plain_error(&e).into());
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

    fn render_empty(&self) -> impl IntoElement {
        card().child(
            empty_state(
                "wallet",
                "No wallets yet",
                "Create a wallet from keys on this computer or a hardware device, or restore one from a recovery phrase or backup file.",
            )
            .child(
                div()
                    .mt_4()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        button("empty-restore", "Restore wallet")
                            .outline()
                            .icon("history")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx)),
                    )
                    .child(
                        button("empty-create", "Create wallet")
                            .icon("plus")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                    ),
            )
            .child(
                div().mt_2().child(
                    button("empty-keys", "Manage keys in the Key library")
                        .link()
                        .small()
                        .on_click(|_, _, cx| navigate(Page::LocalKeys, cx)),
                ),
            ),
        )
    }

    fn render_list(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let col = |w: f32, text: &'static str, right: bool| {
            div()
                .w(px(w))
                .flex_none()
                .when(right, |d| d.flex().justify_end())
                .child(text)
        };
        let header = list_header(vec![
            div().w(px(32.)).flex_none().into_any_element(),
            div().flex_1().child("Name").into_any_element(),
            col(COL_NETWORK, "Network", false).into_any_element(),
            col(COL_CREATED, "Created", false).into_any_element(),
            col(COL_BALANCE, "Balance", true).into_any_element(),
            div().w(px(COL_ACTIONS)).flex_none().into_any_element(),
        ]);
        let mut list = list_card().child(header).child(divider());
        for (ix, w) in self.wallets.iter().enumerate() {
            if ix > 0 {
                list = list.child(divider());
            }
            list = list.child(self.render_row(ix, w, window, cx));
        }
        let unlocked = self.wallets.iter().filter(|w| !w.locked).count();
        div().flex().flex_col().gap_2().child(list).child(
            div()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(format!(
                    "{} wallet{} · {} unlocked",
                    self.wallets.len(),
                    if self.wallets.len() == 1 { "" } else { "s" },
                    unlocked
                )),
        )
    }

    fn render_row(
        &self,
        ix: usize,
        w: &WalletSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let group: SharedString = format!("wallet-row-{ix}").into();
        let id = w.id.as_str().to_string();

        let lead = if w.locked {
            row_icon("lock", t.muted_foreground, false)
        } else {
            row_icon("unlock", t.success, true)
        };
        let lead = div()
            .id(("lock-state", ix))
            .tooltip(tooltip(if w.locked { "Locked" } else { "Unlocked" }))
            .child(lead);

        let mut meta = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(t.muted_foreground)
            .child(policy_label(&w.policy_type));
        if !w.fingerprints.is_empty() {
            let shown: Vec<&str> = w.fingerprints.iter().take(2).map(String::as_str).collect();
            let more = w.fingerprints.len().saturating_sub(2);
            meta = meta
                .child(sep_dot())
                .child(div().font_family(FONT_MONO).child(shown.join(" ")));
            if more > 0 {
                meta = meta.child(format!("+{more}"));
            }
        }
        if w.recovery_only {
            meta = meta
                .child(sep_dot())
                .child(div().text_color(t.warning_ink).child("Recovery-only"));
        }

        let balance = if w.locked {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .text_size(px(12.))
                .text_color(t.ink_3)
                .child("Locked")
                .into_any_element()
        } else {
            match self.balances.get(&id) {
                Some(sat) => div()
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .gap_1()
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.foreground)
                            .child(btc_amount(*sat as i64)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("BTC"),
                    )
                    .into_any_element(),
                None => div()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .child("—")
                    .into_any_element(),
            }
        };

        let mut actions = hover_actions(group.clone(), ("wallet-actions", ix), window, cx)
            .flex()
            .flex_row()
            .items_center()
            .gap_1();
        if !w.locked {
            let lock_id = id.clone();
            actions = actions.child(
                hover_action(("wallet-lock", ix), "lock", "Lock wallet", false).clickable(
                    Probe::new("button", format!("Lock {}", w.name)),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.lock_wallet(lock_id.clone(), cx);
                    }),
                ),
            );
        }
        let del_wallet = w.clone();
        actions = actions.child(
            hover_action(("wallet-delete", ix), "trash", "Delete wallet…", true).clickable(
                Probe::new("button", format!("Delete {}…", w.name)),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_delete(del_wallet.clone(), window, cx);
                }),
            ),
        );

        let open_wallet = w.clone();
        list_row(("wallet-row", ix))
            .group(group)
            .cursor_pointer()
            .clickable(
                Probe::new("row", w.name.clone()),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_wallet(open_wallet.clone(), window, cx)
                }),
            )
            .child(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(w.name.clone()),
                    )
                    .child(meta),
            )
            .child(
                div()
                    .w(px(COL_NETWORK))
                    .flex_none()
                    .flex()
                    .child(network_badge(&w.network)),
            )
            .child(
                div()
                    .w(px(COL_CREATED))
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .child(format_date(w.created_at)),
            )
            .child(
                div()
                    .w(px(COL_BALANCE))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(balance),
            )
            .child(
                div()
                    .w(px(COL_ACTIONS))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .child(actions)
                    .child(icon_chevron()),
            )
    }

    fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let m = self.modal.as_ref()?;
        let error = m.error.clone().map(|e| alert(Tone::Destructive, e));
        let cancel = button("modal-cancel", "Cancel")
            .ghost()
            .disabled(m.submitting)
            .on_click(cx.listener(|this, _, _, cx| this.close_modal(cx)))
            .into_any_element();

        let panel = match m.kind {
            ModalKind::Unlock => {
                let empty = self.unlock_pass.read(cx).text().is_empty();
                modal("unlock-modal", 420.)
                    .child(dialog_title(
                        format!("Unlock “{}”", m.wallet.name),
                        Some(
                            "Enter the passphrase you set when creating this wallet. Keys stay decrypted in memory until you lock it."
                                .into(),
                        ),
                    ))
                    .child(field("Passphrase", self.unlock_pass.clone(), None))
                    .children(error)
                    .child(dialog_actions(vec![
                        cancel,
                        button("unlock-submit", "Unlock")
                            .icon("unlock")
                            .loading(m.submitting)
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, _, cx| this.submit_unlock(cx)))
                            .into_any_element(),
                    ]))
            }
            ModalKind::Delete => {
                let ready = self.delete_ready(cx);
                modal("delete-modal", 460.)
                    .child(dialog_title(
                        format!("Delete “{}”?", m.wallet.name),
                        Some(
                            "This permanently removes the wallet from this computer, including its encrypted seed. You can only get it back with the original recovery phrase or a backup."
                                .into(),
                        ),
                    ))
                    .child(field(
                        format!("Type “{}” to confirm", m.wallet.name),
                        self.delete_name.clone(),
                        None,
                    ))
                    .when(m.wallet.has_hot_keys, |d| {
                        d.child(field("Passphrase", self.delete_pass.clone(), None))
                    })
                    .children(error)
                    .child(dialog_actions(vec![
                        cancel,
                        button("delete-submit", "Delete wallet")
                            .destructive()
                            .icon("trash")
                            .loading(m.submitting)
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.submit_delete(cx)))
                            .into_any_element(),
                    ]))
            }
        };
        Some(panel.on_dismiss(cx.listener(|this, _, _, cx| this.close_modal(cx))))
    }
}

fn icon_chevron() -> impl IntoElement {
    crate::ui::icon("chevron-right")
        .size(px(14.))
        .text_color(theme().ink_3)
}

impl Render for LocalWalletListPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.loading {
            loading_card("Loading wallets…").into_any_element()
        } else if self.wallets.is_empty() {
            self.render_empty().into_any_element()
        } else {
            self.render_list(window, cx).into_any_element()
        };

        div()
            .size_full()
            .relative()
            .child(
                page_frame("local-wallets")
                    .title("Wallets")
                    .subtitle(
                        "Wallets stored on this computer. Nothing leaves it unless you export it.",
                    )
                    .action(
                        button("restore", "Restore")
                            .outline()
                            .small()
                            .icon("history")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx)),
                    )
                    .action(
                        button("new-wallet", "New wallet")
                            .icon("plus")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                    )
                    .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                    .child(body),
            )
            .children(self.render_modal(cx))
    }
}
