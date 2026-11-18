//! `pages/local/WalletDashboard.tsx` — per-wallet dashboard (QBL-227).
//!
//! Renders balance, transaction history, and sync status for a single
//! unlocked wallet. Auto-refreshes balance + history when the sync status
//! reports completion. Receive / Send route to their own pages.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, ClipboardItem, Context, SharedString, Subscription, Window,
};
use sigvault_core::local_wallet::commands::{
    cmd_local_get_balance, cmd_local_get_history, cmd_local_list_wallets, cmd_local_lock_wallet,
    cmd_local_sync, LocalBalance, LocalTxRecord,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::sync_status::{initial_sync_state, watch_sync, LocalSyncState, LocalSyncStatus};
use super::widgets::{back_button, btc_amount, group_thousands, header_bar, sep_dot, tooltip};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, icon, spinner_with, Tone};

fn format_relative(unix: Option<u64>) -> String {
    let Some(unix) = unix.filter(|u| *u > 0) else {
        return "—".into();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.);
    let diff = now - unix as f64;
    if diff < 60. {
        return "just now".into();
    }
    if diff < 3600. {
        return format!("{}m ago", (diff / 60.).floor());
    }
    if diff < 86400. {
        return format!("{}h ago", (diff / 3600.).floor());
    }
    if diff < 86400. * 7. {
        return format!("{}d ago", (diff / 86400.).floor());
    }
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix as i64, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%b %-d, %Y").to_string(),
        _ => "—".into(),
    }
}

fn short_txid(txid: &str) -> String {
    if txid.len() <= 16 {
        return txid.to_string();
    }
    format!("{}…{}", &txid[..8], &txid[txid.len() - 6..])
}

pub struct WalletDashboardPage {
    wallet_id: String,
    wallet: Option<WalletSummary>,
    balance: Option<LocalBalance>,
    history: Vec<LocalTxRecord>,
    loading: bool,
    error: Option<SharedString>,
    syncing: bool,
    copied_txid: Option<String>,
    copied_epoch: usize,
    sync: LocalSyncState,
    _sync_sub: Subscription,
}

impl WalletDashboardPage {
    pub fn new(wallet_id: String, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sync = initial_sync_state(&wallet_id, cx);
        let sub = watch_sync(wallet_id.clone(), cx, |this: &mut Self, state, cx| {
            // Refresh balance + history when a sync cycle completes (auto-
            // sync after unlock, or a manual trigger).
            let became_complete = state.status == LocalSyncStatus::Complete
                && this.sync.status != LocalSyncStatus::Complete;
            this.sync = state;
            if became_complete {
                this.load_all(cx);
            }
            cx.notify();
        });
        let mut this = Self {
            wallet_id,
            wallet: None,
            balance: None,
            history: Vec::new(),
            loading: true,
            error: None,
            syncing: false,
            copied_txid: None,
            copied_epoch: 0,
            sync,
            _sync_sub: sub,
        };
        this.load_all(cx);
        this
    }

    fn load_all(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let state = State::new(&st);
            let (wallets, bal, hist) = futures::join!(
                cmd_local_list_wallets(app.clone(), state),
                cmd_local_get_balance(app.clone(), state, id.clone()),
                cmd_local_get_history(app, state, id.clone()),
            );
            let wallets = wallets?;
            let found = wallets.into_iter().find(|w| w.id.as_str() == id);
            Ok((found, bal?, hist?))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((wallet, bal, hist)) => {
                        this.wallet = wallet;
                        this.balance = Some(bal);
                        this.history = hist;
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

    fn trigger_sync(&mut self, cx: &mut Context<Self>) {
        if self.syncing {
            return;
        }
        self.syncing = true;
        self.error = None;
        cx.notify();
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_sync(app, State::new(&st), id).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.error = Some(e.into());
                }
                this.syncing = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn lock_and_exit(&mut self, cx: &mut Context<Self>) {
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_lock_wallet(app, State::new(&st), id).await
        });
        cx.spawn(async move |_, cx| {
            let _ = task.await;
            cx.update(|cx| navigate(Page::LocalWallets, cx)).ok();
        })
        .detach();
    }

    fn copy_txid(&mut self, txid: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(txid.clone()));
        self.copied_txid = Some(txid);
        self.copied_epoch += 1;
        let epoch = self.copied_epoch;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                if this.copied_epoch == epoch {
                    this.copied_txid = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    // ── rendering ──

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let wallet_name = self
            .wallet
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Wallet".into());
        header_bar()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_w_0()
                    .child(back_button("back", "Back to wallets", |_, _, cx| {
                        navigate(Page::LocalWallets, cx)
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child(wallet_name),
                            )
                            .children(self.wallet.as_ref().map(|w| {
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(badge(w.network.to_uppercase(), Tone::Outline))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(t.muted_foreground)
                                            .child(w.policy_type.clone()),
                                    )
                            })),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(self.render_sync_indicator(cx))
                    .child(
                        button("lock", "Lock")
                            .outline()
                            .small()
                            .icon("lock")
                            .on_click(cx.listener(|this, _, _, cx| this.lock_and_exit(cx))),
                    ),
            )
    }

    fn render_sync_indicator(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let sync = &self.sync;
        let active = self.syncing || sync.status == LocalSyncStatus::Active;
        let label: SharedString = if active {
            if sync.message.is_empty() {
                "Syncing…".into()
            } else {
                sync.message.clone().into()
            }
        } else if sync.status == LocalSyncStatus::Complete {
            "Synced".into()
        } else {
            "Sync".into()
        };
        let title: SharedString = if active {
            format!("{} ({}%)", sync.phase.clone().unwrap_or_default(), sync.percent).into()
        } else {
            "Run an Electrum sync".into()
        };
        let fg = t.muted_foreground;
        let leading = if active {
            spinner_with(px(14.), fg).into_any_element()
        } else {
            icon("refresh-ccw").size(px(14.)).text_color(fg).into_any_element()
        };
        let mut el = div()
            .id("sync-indicator")
            .h(px(32.))
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .rounded_md()
            .text_size(px(13.))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(fg)
            .whitespace_nowrap()
            .tooltip(tooltip(title))
            .child(leading)
            .child(label);
        if active {
            el = el.opacity(0.5);
        } else {
            el = el
                .cursor_pointer()
                .hover(move |s| s.bg(alpha(t.muted, 0.6)).text_color(t.foreground))
                .on_click(cx.listener(|this, _, _, cx| this.trigger_sync(cx)));
        }
        el
    }

    fn render_balance(&self) -> impl IntoElement {
        let t = theme();
        let confirmed = self.balance.as_ref().map(|b| b.confirmed_sat).unwrap_or(0);
        let pending = self.balance.as_ref().map(|b| b.unconfirmed_sat).unwrap_or(0);
        div()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .px_6()
            .py_6()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .child("Balance"),
            )
            .child(
                div()
                    .mt_2()
                    .flex()
                    .flex_row()
                    .items_end()
                    .gap(px(10.))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(36.))
                            .line_height(px(36.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(btc_amount(confirmed as i64)),
                    )
                    .child(
                        div()
                            .pb(px(3.))
                            .text_size(px(13.))
                            .text_color(t.muted_foreground)
                            .child("BTC"),
                    ),
            )
            .child(
                div()
                    .mt_2()
                    .font_family(FONT_MONO)
                    .text_size(px(11.))
                    .text_color(t.muted_foreground)
                    .child(format!("{} sat", group_thousands(confirmed))),
            )
            .when(pending > 0, |d| {
                d.child(
                    div()
                        .mt_3()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(t.warning)
                        .child(div().size(px(6.)).rounded_full().bg(t.warning))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .child("+")
                                .child(
                                    div()
                                        .font_family(FONT_MONO)
                                        .child(btc_amount(pending as i64)),
                                )
                                .child(" BTC pending"),
                        ),
                )
            })
    }

    fn render_recovery_only(&self) -> impl IntoElement {
        let t = theme();
        div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(10.))
            .rounded_md()
            .border_1()
            .border_color(alpha(t.warning, 0.3))
            .bg(alpha(t.warning, 0.05))
            .px(px(14.))
            .py_3()
            .text_size(px(12.))
            .line_height(px(17.))
            .text_color(t.foreground)
            .child(icon("alert-circle").size(px(14.)).mt(px(1.5)).text_color(t.warning))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Recovery-only wallet"))
                    .child(
                        div().mt(px(2.)).text_color(t.muted_foreground).child(
                            "The primary path is unspendable. Funds can only be moved via the recovery key after the timelock elapses.",
                        ),
                    ),
            )
    }

    fn render_actions(&self) -> impl IntoElement {
        let watch_only = self
            .wallet
            .as_ref()
            .map(|w| w.policy_type == "watch_only")
            .unwrap_or(false);
        let receive_id = self.wallet_id.clone();
        let send_id = self.wallet_id.clone();
        div()
            .flex()
            .flex_row()
            .gap_3()
            .child(
                div().flex_1().flex_basis(px(0.)).child(
                    button("receive", "Receive")
                        .outline()
                        .large()
                        .full_width()
                        .icon("arrow-left")
                        .on_click(move |_, _, cx| navigate(Page::LocalReceive(receive_id.clone()), cx)),
                ),
            )
            .when(!watch_only, |d| {
                d.child(
                    div().flex_1().flex_basis(px(0.)).child(
                        button("send", "Send")
                            .outline()
                            .large()
                            .full_width()
                            .icon("arrow-right")
                            .on_click(move |_, _, cx| navigate(Page::LocalSend(send_id.clone()), cx)),
                    ),
                )
            })
    }

    fn render_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let list = if self.history.is_empty() {
            div()
                .rounded_lg()
                .border_1()
                .border_dashed()
                .border_color(t.border)
                .bg(alpha(t.card, 0.4))
                .px_5()
                .py_10()
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t.foreground)
                        .child("No transactions yet"),
                )
                .child(
                    div()
                        .mt_1()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Use Receive to fund this wallet, then run a sync."),
                )
                .into_any_element()
        } else {
            let rows: Vec<_> = self
                .history
                .iter()
                .enumerate()
                .map(|(ix, tx)| self.render_tx_row(ix, tx, cx).into_any_element())
                .collect();
            div()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .bg(t.card)
                .children(rows)
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child("Transactions"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(format!("{} total", self.history.len())),
                    ),
            )
            .child(list)
    }

    fn render_tx_row(&self, ix: usize, tx: &LocalTxRecord, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let incoming = tx.net_sat >= 0;
        let copied = self.copied_txid.as_deref() == Some(tx.txid.as_str());
        let color = if incoming { t.success } else { t.destructive };

        let mut meta = div()
            .mt(px(2.))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .text_size(px(12.))
            .text_color(t.muted_foreground)
            .child(format_relative(tx.block_time))
            .child(sep_dot());
        meta = if tx.confirmed {
            meta.child(format!(
                "block {}",
                tx.block_height.map(|h| h.to_string()).unwrap_or_default()
            ))
        } else {
            meta.child(div().text_color(t.warning).child("pending"))
        };
        if tx.fee_sat > 0 {
            meta = meta
                .child(sep_dot())
                .child(format!("fee {} sat", group_thousands(tx.fee_sat)));
        }

        let txid = tx.txid.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .px_5()
            .py_3()
            .when(ix > 0, |d| d.border_t_1().border_color(t.border))
            .child(
                div()
                    .size(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(alpha(color, 0.10))
                    .child(
                        icon(if incoming { "tx-in" } else { "tx-out" })
                            .size(px(14.))
                            .text_color(color),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .id(("txid", ix))
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.))
                                    .text_color(t.foreground)
                                    .cursor_pointer()
                                    .hover(move |s| s.text_color(t.primary))
                                    .tooltip(tooltip(tx.txid.clone()))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.copy_txid(txid.clone(), cx)
                                    }))
                                    .child(if copied {
                                        "✓ Copied".to_string()
                                    } else {
                                        short_txid(&tx.txid)
                                    }),
                            )
                            .child(meta),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .items_end()
                            .font_family(FONT_MONO)
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if incoming { t.success } else { t.foreground })
                            .child(format!(
                                "{}{}",
                                if incoming { "+" } else { "" },
                                btc_amount(tx.net_sat)
                            ))
                            .child(
                                div()
                                    .mt(px(2.))
                                    .text_size(px(10.))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(alpha(t.muted_foreground, 0.7))
                                    .child("BTC"),
                            ),
                    ),
            )
    }
}

impl Render for WalletDashboardPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        if self.loading {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t.muted_foreground)
                        .child("Loading wallet…"),
                )
                .into_any_element();
        }

        let recovery_only = self.wallet.as_ref().map(|w| w.recovery_only).unwrap_or(false);
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_header(cx))
            .child(
                div()
                    .id("dashboard-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div().w_full().flex().flex_col().items_center().px_6().py(px(28.)).child(
                            div()
                                .w_full()
                                .max_w(px(768.))
                                .flex()
                                .flex_col()
                                .gap_6()
                                .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                                .child(self.render_balance())
                                .when(recovery_only, |d| d.child(self.render_recovery_only()))
                                .child(self.render_actions())
                                .child(self.render_history(cx)),
                        ),
                    ),
            )
            .into_any_element()
    }
}
