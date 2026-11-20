//! `pages/local/WalletDashboard.tsx` — per-wallet dashboard (QBL-227).
//!
//! Balance, activity, keys and descriptor details for one wallet.
//! Auto-refreshes balance + history when the sync status reports
//! completion. Locked wallets (reached from the sidebar) unlock inline.
//! Receive / Send route to their own pages.

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, SharedString, Subscription, Window};
use sigvault_core::local_wallet::commands::{
    cmd_local_export_wallet_backup, cmd_local_get_balance, cmd_local_get_history,
    cmd_local_get_wallet_details, cmd_local_list_wallets, cmd_local_lock_wallet, cmd_local_sync,
    ExportWalletBackupRequest, LocalBalance, LocalTxRecord, WalletDetails,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::sync_status::{initial_sync_state, watch_sync, LocalSyncState, LocalSyncStatus};
use super::wallet_unlock::{WalletUnlock, WalletUnlocked};
use super::widgets::{
    btc_amount, copy_button, default_save_dir, format_date, format_relative, group_thousands,
    hover_actions, list_card, list_row, loading_card, network_badge, policy_badge, row_icon,
    sep_dot, tooltip,
};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::{navigate, toast};
use crate::state::{local_changed, store};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, badge, button, card, divider, empty_state, page_frame, section, truncate_middle, Tone,
};

pub struct WalletDashboardPage {
    wallet_id: String,
    wallet: Option<WalletSummary>,
    details: Option<WalletDetails>,
    balance: Option<LocalBalance>,
    history: Vec<LocalTxRecord>,
    loading: bool,
    error: Option<SharedString>,
    syncing: bool,
    /// A sync finished since the wallet was unlocked.
    synced: bool,
    sync: LocalSyncState,
    unlock: Entity<WalletUnlock>,
    exporting: bool,
    _subs: Vec<Subscription>,
}

impl WalletDashboardPage {
    pub fn new(wallet_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sync = initial_sync_state(&wallet_id, cx);
        let synced = store(cx)
            .read(cx)
            .sync_progress
            .get(&wallet_id)
            .is_some_and(|p| p.phase == "complete");
        let sync_sub = watch_sync(wallet_id.clone(), cx, |this: &mut Self, state, cx| {
            // Refresh balance + history when a sync cycle completes (auto-
            // sync after unlock, or a manual trigger).
            let became_complete = state.status == LocalSyncStatus::Complete
                && this.sync.status != LocalSyncStatus::Complete;
            this.sync = state;
            if became_complete {
                this.synced = true;
                this.load_all(cx);
            }
            cx.notify();
        });
        let unlock = cx.new(|cx| {
            WalletUnlock::new(
                wallet_id.clone(),
                true,
                "load balances and activity",
                window,
                cx,
            )
        });
        let unlock_sub = cx.subscribe(&unlock, |this, _, _: &WalletUnlocked, cx| {
            this.load_all(cx);
        });
        let mut this = Self {
            wallet_id,
            wallet: None,
            details: None,
            balance: None,
            history: Vec::new(),
            loading: true,
            error: None,
            syncing: false,
            synced,
            sync,
            unlock,
            exporting: false,
            _subs: vec![sync_sub, unlock_sub],
        };
        this.load_all(cx);
        this
    }

    fn locked(&self) -> bool {
        self.wallet.as_ref().is_some_and(|w| w.locked)
    }

    fn load_all(&mut self, cx: &mut Context<Self>) {
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let state = State::new(&st);
            let wallets = cmd_local_list_wallets(app.clone(), state).await?;
            let wallet = wallets.into_iter().find(|w| w.id.as_str() == id);
            // Details are metadata only — available while locked too.
            let details = cmd_local_get_wallet_details(app.clone(), state, id.clone())
                .await
                .ok();
            if wallet.as_ref().is_none_or(|w| w.locked) {
                return Ok((wallet, details, None, Vec::new()));
            }
            let (bal, hist) = futures::join!(
                cmd_local_get_balance(app.clone(), state, id.clone()),
                cmd_local_get_history(app, state, id.clone()),
            );
            Ok((wallet, details, Some(bal?), hist?))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((wallet, details, bal, hist)) => {
                        if wallet.is_none() {
                            this.error = Some("This wallet no longer exists.".into());
                        } else {
                            this.error = None;
                        }
                        this.wallet = wallet;
                        this.details = details;
                        this.balance = bal;
                        this.history = hist;
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

    fn trigger_sync(&mut self, cx: &mut Context<Self>) {
        if self.syncing || self.sync.status == LocalSyncStatus::Active {
            return;
        }
        self.syncing = true;
        self.error = None;
        cx.notify();
        let id = self.wallet_id.clone();
        let task = backend(cx)
            .call(move |app, st| async move { cmd_local_sync(app, State::new(&st), id).await });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.error = Some(super::widgets::plain_error(&e).into());
                }
                this.syncing = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn lock(&mut self, cx: &mut Context<Self>) {
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_lock_wallet(app, State::new(&st), id).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(_) => {
                        local_changed(cx);
                        this.synced = false;
                        this.load_all(cx);
                    }
                    Err(e) => this.error = Some(super::widgets::plain_error(&e).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn export_backup(&mut self, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.exporting = true;
        cx.notify();
        let wallet_id = self.wallet_id.clone();
        let name = self
            .wallet
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "wallet".into());
        let file_name = format!("{}.wallet-backup.json", file_safe(&name));
        let export = backend(cx).call(move |app, st| async move {
            cmd_local_export_wallet_backup(
                app,
                State::new(&st),
                ExportWalletBackupRequest { wallet_id },
            )
            .await
        });
        let rx = cx.prompt_for_new_path(&default_save_dir(), Some(&file_name));
        cx.spawn(async move |this, cx| {
            let res: Result<Option<String>, String> = async {
                let contents = export.await?;
                let path = match rx.await {
                    Ok(Ok(Some(p))) => p,
                    Ok(Err(e)) => return Err(e.to_string()),
                    _ => return Ok(None),
                };
                cx.background_executor()
                    .spawn(async move {
                        std::fs::write(&path, contents.as_bytes())
                            .map(|_| {
                                path.file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            })
                            .map_err(|e| e.to_string())
                    })
                    .await
                    .map(Some)
            }
            .await;
            this.update(cx, |this, cx| {
                this.exporting = false;
                match res {
                    Ok(Some(f)) => toast(cx, format!("Saved {f}")),
                    Ok(None) => {}
                    Err(e) => this.error = Some(format!("Couldn't save backup: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── rendering ──

    fn sync_active(&self) -> bool {
        self.syncing || self.sync.status == LocalSyncStatus::Active
    }

    fn render_actions(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(w) = self.wallet.as_ref() else {
            return Vec::new();
        };
        if w.locked {
            return Vec::new();
        }
        let active = self.sync_active();
        let sync_label: SharedString = if active {
            if self.sync.percent > 0 {
                format!("Syncing… {}%", self.sync.percent.min(100)).into()
            } else {
                "Syncing…".into()
            }
        } else {
            "Sync".into()
        };
        let sync_tip: SharedString = if active {
            phase_label(self.sync.phase.as_deref()).into()
        } else {
            "Fetch the latest transactions from your Electrum server".into()
        };
        let watch_only = w.policy_type == "watch_only";
        let receive_id = self.wallet_id.clone();
        let send_id = self.wallet_id.clone();
        let mut out = vec![
            div()
                .id("sync-wrap")
                .tooltip(tooltip(sync_tip))
                .child(
                    button("sync", sync_label)
                        .outline()
                        .small()
                        .icon("refresh-ccw")
                        .loading(active)
                        .on_click(cx.listener(|this, _, _, cx| this.trigger_sync(cx))),
                )
                .into_any_element(),
            button("lock", "Lock")
                .outline()
                .small()
                .icon("lock")
                .on_click(cx.listener(|this, _, _, cx| this.lock(cx)))
                .into_any_element(),
            button("receive", "Receive")
                .outline()
                .small()
                .icon("arrow-down-left")
                .on_click(move |_, _, cx| navigate(Page::LocalReceive(receive_id.clone()), cx))
                .into_any_element(),
        ];
        if !watch_only {
            out.push(
                button("send", "Send")
                    .icon("arrow-up-right")
                    .on_click(move |_, _, cx| navigate(Page::LocalSend(send_id.clone()), cx))
                    .into_any_element(),
            );
        }
        out
    }

    fn render_balance(&self) -> impl IntoElement {
        let t = theme();
        let confirmed = self.balance.as_ref().map(|b| b.confirmed_sat).unwrap_or(0);
        let pending = self
            .balance
            .as_ref()
            .map(|b| b.unconfirmed_sat)
            .unwrap_or(0);
        let total = confirmed + pending;
        let active = self.sync_active();

        let stat = |label: &'static str, sat: u64, color| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(label),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_baseline()
                        .gap_1()
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(13.))
                                .text_color(color)
                                .child(btc_amount(sat as i64)),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(t.muted_foreground)
                                .child("BTC"),
                        ),
                )
        };

        let status: AnyElement = if active {
            let pct = self.sync.percent.min(100) as f32 / 100.;
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .w_full()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(format!(
                            "Syncing… {}% · {}",
                            self.sync.percent.min(100),
                            phase_label(self.sync.phase.as_deref())
                        )),
                )
                .child(
                    div()
                        .h(px(3.))
                        .w_full()
                        .rounded_full()
                        .bg(alpha(t.muted_foreground, 0.15))
                        .child(
                            div()
                                .h_full()
                                .rounded_full()
                                .bg(t.primary)
                                .w(gpui::relative(pct.max(0.02))),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(div().size(px(6.)).rounded_full().bg(if self.synced {
                    t.success
                } else {
                    alpha(t.muted_foreground, 0.5)
                }))
                .child(if self.synced {
                    "Up to date with your Electrum server"
                } else {
                    "Showing the last synced state — press Sync to refresh"
                })
                .into_any_element()
        };

        card()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .p_5()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child("Total balance"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_baseline()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(28.))
                                            .line_height(px(34.))
                                            .text_color(t.foreground)
                                            .child(btc_amount(total as i64)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(t.muted_foreground)
                                            .child("BTC"),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(format!("{} sat", group_thousands(total))),
                            ),
                    )
                    .child(div().w(px(1.)).bg(alpha(t.border, 0.6)))
                    .child(
                        div()
                            .w(px(220.))
                            .flex_none()
                            .p_5()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap_3()
                            .child(stat("Confirmed", confirmed, t.foreground))
                            .child(stat(
                                "Pending",
                                pending,
                                if pending > 0 {
                                    t.warning_ink
                                } else {
                                    t.muted_foreground
                                },
                            )),
                    ),
            )
            .child(divider())
            .child(div().px_5().py_3().child(status))
    }

    fn render_activity(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.history.len();
        let body = if self.history.is_empty() {
            let rid = self.wallet_id.clone();
            card()
                .child(
                    empty_state(
                        "history",
                        "No transactions yet",
                        "Share a receive address to fund this wallet. Incoming payments show up here after the next sync.",
                    )
                    .child(
                        div().mt_4().child(
                            button("empty-receive", "Receive bitcoin")
                                .outline()
                                .small()
                                .icon("arrow-down-left")
                                .on_click(move |_, _, cx| navigate(Page::LocalReceive(rid.clone()), cx)),
                        ),
                    ),
                )
                .into_any_element()
        } else {
            let mut list = list_card();
            for (ix, tx) in self.history.iter().enumerate() {
                if ix > 0 {
                    list = list.child(divider());
                }
                list = list.child(self.render_tx_row(ix, tx, window, cx));
            }
            list.into_any_element()
        };
        section("Activity")
            .description(if n == 0 {
                "Transactions sent from and received by this wallet.".to_string()
            } else {
                format!("{n} transaction{}", if n == 1 { "" } else { "s" })
            })
            .child(body)
    }

    fn render_tx_row(
        &self,
        ix: usize,
        tx: &LocalTxRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let incoming = tx.net_sat >= 0;
        let group: SharedString = format!("tx-{ix}").into();

        let mut meta = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(t.muted_foreground);
        meta = if tx.confirmed {
            meta.child(format!(
                "Confirmed{}",
                tx.block_height
                    .map(|h| format!(" in block {}", group_thousands(h as u64)))
                    .unwrap_or_default()
            ))
        } else {
            meta.child(div().text_color(t.warning_ink).child("Pending"))
        };
        if tx.fee_sat > 0 {
            meta = meta
                .child(sep_dot())
                .child(format!("fee {} sat", group_thousands(tx.fee_sat)));
        }

        let when = tx
            .block_time
            .filter(|u| *u > 0)
            .map(|u| (format_relative(u), format_date(u as i64)));

        list_row(("tx-row", ix))
            .group(group.clone())
            .child(if incoming {
                row_icon("arrow-down-left", t.success, true)
            } else {
                row_icon("arrow-up-right", t.muted_foreground, true)
            })
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
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(if incoming { "Received" } else { "Sent" }),
                    )
                    .child(meta),
            )
            .child(
                div()
                    .w(px(200.))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(("txid", ix))
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .tooltip(tooltip(tx.txid.clone()))
                            .child(truncate_middle(&tx.txid, 8)),
                    )
                    .child(
                        hover_actions(group, ("txid-actions", ix), window, cx).child(copy_button(
                            ("copy-txid", ix),
                            tx.txid.clone(),
                            "Copy transaction ID",
                        )),
                    ),
            )
            .child(
                div()
                    .id(("tx-when", ix))
                    .w(px(96.))
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .map(|d| match when {
                        Some((rel, abs)) => d.tooltip(tooltip(abs)).child(rel),
                        None => d.child("—"),
                    }),
            )
            .child(
                div()
                    .w(px(160.))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .items_baseline()
                    .gap_1()
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(13.))
                            .text_color(if incoming { t.success } else { t.foreground })
                            .child(format!(
                                "{}{}",
                                if incoming { "+" } else { "" },
                                btc_amount(tx.net_sat)
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("BTC"),
                    ),
            )
    }

    fn render_keys(&self, d: &WalletDetails) -> impl IntoElement {
        let t = theme();
        let mut list = list_card();
        for (ix, slot) in d.slots.iter().enumerate() {
            if ix > 0 {
                list = list.child(divider());
            }
            let hw = slot.source == "hardware";
            let mut meta = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(source_label(&slot.source));
            if let Some(fp) = slot.fingerprint.clone() {
                meta = meta
                    .child(sep_dot())
                    .child(div().font_family(FONT_MONO).child(fp));
            }
            if let Some(p) = slot.derivation_path.clone() {
                meta = meta
                    .child(sep_dot())
                    .child(div().font_family(FONT_MONO).child(p));
            }
            list = list.child(
                list_row(("slot", ix))
                    .child(row_icon(
                        if hw { "hw-device" } else { "key" },
                        t.muted_foreground,
                        false,
                    ))
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
                                    .child(if slot.label.is_empty() {
                                        format!("Key {}", ix + 1)
                                    } else {
                                        slot.label.clone()
                                    }),
                            )
                            .child(meta),
                    )
                    .child(if slot.hot {
                        badge("On this computer", Tone::Success)
                    } else if hw {
                        badge("Hardware", Tone::Outline)
                    } else {
                        badge("Public key only", Tone::Outline)
                    })
                    .child(copy_button(
                        ("copy-xpub", ix),
                        slot.xpub.clone(),
                        "Copy xpub",
                    )),
            );
        }
        section("Keys")
            .description(format!(
                "{} key{} can sign for this wallet.",
                d.slots.len(),
                if d.slots.len() == 1 { "" } else { "s" }
            ))
            .child(list)
    }

    fn render_details(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let w = self.wallet.as_ref();
        let d = self.details.as_ref();
        let row = |k: &'static str, v: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .min_h(px(44.))
                .px_4()
                .py_2()
                .child(
                    div()
                        .w(px(140.))
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(k),
                )
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_size(px(13.))
                        .child(v),
                )
        };
        let mono_row = |id: &'static str, value: String| {
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id(SharedString::from(format!("{id}-text")))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .text_color(t.foreground)
                        .tooltip(tooltip(value.clone()))
                        .child(value.clone()),
                )
                .child(copy_button(id, value, "Copy"))
                .into_any_element()
        };

        let mut c = card();
        if let Some(w) = w {
            c = c
                .child(row(
                    "Created",
                    div().child(format_date(w.created_at)).into_any_element(),
                ))
                .child(divider())
                .child(row(
                    "Wallet ID",
                    mono_row("copy-id", w.id.as_str().to_string()),
                ));
        }
        if let Some(d) = d {
            c = c
                .child(divider())
                .child(row(
                    "Receive descriptor",
                    mono_row("copy-ext", d.external_descriptor.clone()),
                ))
                .child(divider())
                .child(row(
                    "Change descriptor",
                    mono_row("copy-int", d.internal_descriptor.clone()),
                ));
        }
        section("Details")
            .description("Descriptors describe how this wallet's addresses are built. The backup file contains no private keys.")
            .action(
                button("export-backup", "Export backup")
                    .outline()
                    .small()
                    .icon("download")
                    .loading(self.exporting)
                    .on_click(cx.listener(|this, _, _, cx| this.export_backup(cx))),
            )
            .child(c)
    }
}

fn phase_label(phase: Option<&str>) -> &'static str {
    match phase {
        Some("connecting") => "Connecting to Electrum",
        Some("fetching_history") => "Fetching history",
        Some("persisting") => "Saving wallet data",
        Some("complete") => "Done",
        _ => "Working",
    }
}

fn source_label(source: &str) -> &'static str {
    match source {
        "generated" => "Generated on this computer",
        "seed" => "Recovery phrase",
        "xprv" => "Extended private key",
        "hardware" => "Hardware device",
        "xpub" => "Extended public key",
        "key_file" => "Key backup file",
        _ => "Key",
    }
}

fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "wallet".into()
    } else {
        s
    }
}

impl Render for WalletDashboardPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self
            .wallet
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Wallet".into());
        let mut frame = page_frame("wallet-dashboard")
            .crumb("Wallets", Some(Page::LocalWallets))
            .crumb(name.clone(), None)
            .title(name);
        if let Some(w) = self.wallet.as_ref() {
            frame = frame.title_adornment(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .ml_1()
                    .child(network_badge(&w.network))
                    .child(policy_badge(&w.policy_type))
                    .when(w.recovery_only, |d| {
                        d.child(badge("Recovery-only", Tone::Warning))
                    })
                    .when(w.locked, |d| d.child(badge("Locked", Tone::Outline))),
            );
        }
        for a in self.render_actions(cx) {
            frame = frame.action(a);
        }

        if self.loading {
            return frame
                .child(loading_card("Loading wallet…"))
                .into_any_element();
        }
        frame = frame.children(self.error.clone().map(|e| alert(Tone::Destructive, e)));
        if self.wallet.is_none() {
            return frame.into_any_element();
        }

        let recovery_only = self.wallet.as_ref().is_some_and(|w| w.recovery_only);
        if self.locked() {
            let hot = self.wallet.as_ref().is_some_and(|w| w.has_hot_keys);
            self.unlock.update(cx, |u, cx| u.set_hot(hot, cx));
            frame = frame.child(self.unlock.clone());
        } else {
            frame = frame
                .child(self.render_balance())
                .when(recovery_only, |f| {
                    f.child(alert(
                        Tone::Warning,
                        "Recovery-only wallet: the primary spending path is unspendable. Funds can only be moved with the recovery key after the timelock elapses.",
                    ))
                })
                .child(self.render_activity(window, cx));
        }
        if let Some(d) = self.details.as_ref() {
            if !d.slots.is_empty() {
                frame = frame.child(self.render_keys(d));
            }
        }
        frame.child(self.render_details(cx)).into_any_element()
    }
}
