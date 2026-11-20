//! Workspace dashboards. One page, two flavours:
//!
//! - "On this device": on first run the three setup steps (add a key →
//!   create a wallet → back it up); afterwards the wallets with balance,
//!   sync state and, for time-locked wallets, when the recovery key can
//!   spend.
//! - "SigVault Cloud": sessions that are open for this user first (each
//!   with Join), then recent sessions.
//!
//! The Bitcoin price chart stays at the bottom of both.

use gpui::{div, prelude::*, px, AnyElement, AppContext, Context, Entity, SharedString, Window};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{cmd_get_remote_sessions, cmd_update_remote_sessions};
use sigvault_core::local_wallet::commands::{
    cmd_local_get_balance, cmd_local_list_spending_paths, cmd_local_list_wallets,
};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use crate::e2e::{E2eClick, Probe};
use crate::keychain::{keychain, KeychainEvent};
use crate::pages::cloud::common::{
    format_relative, icon_tile, join_session, main_window, parse_time, session_icon, session_title,
    short_id,
};
use crate::pages::local::setup::policy::describe_timelock;
use crate::pages::local::widgets::{btc_amount, network_badge, policy_label, row_icon, sep_dot};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{store, RemoteSession, StoreEvent};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::price_chart::PriceChart;
use crate::ui::{alert, badge, button, card, divider, icon, page_frame, section, Tone};

/// What the local home knows about one wallet.
struct WalletRow {
    summary: WalletSummary,
    balance_sat: Option<u64>,
    /// Shortest recovery timelock, for unlocked time-locked wallets.
    recovery_blocks: Option<u32>,
}

pub struct HomePage {
    mode: AppMode,
    /// `None` until the first load finishes.
    wallets: Option<Vec<WalletRow>>,
    /// Cloud: the latest session list (`None` until fetched).
    sessions: Option<Vec<RemoteSession>>,
    joining: Option<String>,
    error: Option<SharedString>,
    chart: Entity<PriceChart>,
}

impl HomePage {
    pub fn new(mode: AppMode, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify())
            .detach();
        cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| {
            if matches!(ev, StoreEvent::LocalDataChanged) {
                this.load(cx);
            }
        })
        .detach();
        cx.observe(&store(cx), |_, _, cx| cx.notify()).detach();
        let mut this = Self {
            mode,
            wallets: None,
            sessions: None,
            joining: None,
            error: None,
            chart: cx.new(PriceChart::new),
        };
        this.load(cx);
        this
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        match self.mode {
            AppMode::Local => self.load_wallets(cx),
            AppMode::Cloud => self.load_sessions(cx),
        }
    }

    fn load_wallets(&mut self, cx: &mut Context<Self>) {
        let task = backend(cx).call(|app, st| async move {
            let mut list = cmd_local_list_wallets(app.clone(), State::new(&st)).await?;
            list.sort_by_key(|w| w.name.to_lowercase());
            let mut rows = Vec::with_capacity(list.len());
            for w in list {
                let id = w.id.as_str().to_string();
                let (mut balance_sat, mut recovery_blocks) = (None, None);
                // Both are in-memory reads that need an unlocked wallet.
                if !w.locked {
                    balance_sat = cmd_local_get_balance(app.clone(), State::new(&st), id.clone())
                        .await
                        .ok()
                        .map(|b| b.confirmed_sat + b.unconfirmed_sat);
                    if w.policy_type == "timelocked" {
                        recovery_blocks =
                            cmd_local_list_spending_paths(app.clone(), State::new(&st), id)
                                .await
                                .ok()
                                .and_then(|paths| {
                                    paths.iter().filter_map(|p| p.timelock_blocks).min()
                                });
                    }
                }
                rows.push(WalletRow {
                    summary: w,
                    balance_sat,
                    recovery_blocks,
                });
            }
            Ok(rows)
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(rows) => {
                        this.wallets = Some(rows);
                        this.error = None;
                    }
                    Err(_) => {
                        this.wallets.get_or_insert_with(Vec::new);
                        this.error = Some("Couldn't read the wallets on this computer.".into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Refresh the session list (errors keep what the store already has).
    fn load_sessions(&mut self, cx: &mut Context<Self>) {
        let task = backend(cx).call(|app, st| async move {
            let window = main_window(&app)?;
            cmd_update_remote_sessions(window, State::new(&st)).await?;
            Ok(cmd_get_remote_sessions(State::new(&st)).await.ok())
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Ok(Some(fresh)) = res {
                    let known = store(cx).read(cx).remote_sessions.clone();
                    this.sessions = Some(
                        fresh
                            .into_iter()
                            .map(|s| RemoteSession {
                                created_at: known
                                    .iter()
                                    .find(|k| k.id == s.id)
                                    .and_then(|k| k.created_at.clone()),
                                id: s.id,
                                status: s.status,
                                session_type: s.session_type,
                            })
                            .collect(),
                    );
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn join(&mut self, id: String, cx: &mut Context<Self>) {
        self.joining = Some(id.clone());
        self.error = None;
        cx.notify();
        let task = join_session(id, cx);
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.error = Some(e);
                }
                this.joining = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

// ── shared bits ──

fn row_title(text: impl Into<SharedString>) -> gpui::Div {
    let t = theme();
    div()
        .truncate()
        .text_size(px(14.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(t.foreground)
        .child(text.into())
}

fn row_meta() -> gpui::Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(t.muted_foreground)
}

fn list(rows: Vec<AnyElement>) -> gpui::Div {
    let mut c = card().overflow_hidden();
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            c = c.child(divider());
        }
        c = c.child(r);
    }
    c
}

fn quiet_note(text: impl Into<SharedString>) -> gpui::Div {
    let t = theme();
    card()
        .px_5()
        .py_4()
        .text_size(px(13.))
        .line_height(px(19.))
        .text_color(t.ink_2)
        .child(text.into())
}

// ── On this device ──

struct SetupStep {
    title: &'static str,
    body: &'static str,
    done: bool,
    action: Option<(&'static str, &'static str, Page)>,
}

fn setup_steps(steps: Vec<SetupStep>) -> impl IntoElement {
    let t = theme();
    // The first unfinished step carries the one primary button.
    let next = steps.iter().position(|s| !s.done);
    let rows = steps
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let is_next = next == Some(i);
            let marker = div()
                .size(px(28.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .map(|d| {
                    if s.done {
                        d.border_color(t.success)
                            .bg(alpha(t.success, 0.12))
                            .child(icon("check").size(px(14.)).text_color(Tone::Success.ink()))
                    } else {
                        d.border_color(if is_next { t.brand_ink } else { t.border })
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if is_next {
                                t.brand_ink
                            } else {
                                t.muted_foreground
                            })
                            .child(format!("{}", i + 1))
                    }
                });
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .px_5()
                .py_4()
                .child(marker)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(row_title(s.title))
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(19.))
                                .text_color(t.muted_foreground)
                                .child(s.body),
                        ),
                )
                .children(s.action.filter(|_| !s.done).map(|(id, label, page)| {
                    button(id, label)
                        .when(!is_next, |b| b.outline())
                        .on_click(move |_, _, cx| navigate(page.clone(), cx))
                }))
                .into_any_element()
        })
        .collect();
    list(rows)
}

impl HomePage {
    fn render_local(&self, cx: &mut Context<Self>) -> AnyElement {
        let keys = keychain(cx).read(cx).keys().len();
        let wallets = self.wallets.as_ref();
        let wallet_count = wallets.map(|w| w.len()).unwrap_or(0);
        let first_run = wallets.is_some() && wallet_count == 0;

        let (title, subtitle) = match wallets {
            None => ("On this device", "Your wallets and keys on this computer."),
            Some(_) if first_run && keys == 0 => (
                "Set up your first wallet",
                "Three steps, all on this computer. No account needed.",
            ),
            Some(_) if first_run => (
                "Welcome back.",
                "Your keys are in the library. Next, combine them into a wallet.",
            ),
            Some(_) => ("Welcome back.", "Your wallets on this computer."),
        };

        let mut frame = page_frame("home")
            .crumb("Dashboard", None)
            .title(title)
            .subtitle(subtitle);
        if !first_run {
            frame = frame
                .action(
                    button("home-restore", "Restore")
                        .outline()
                        .small()
                        .icon("history")
                        .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx)),
                )
                .action(
                    button("home-cta", "New wallet")
                        .icon("plus")
                        .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                );
        }
        let frame = frame.children(self.error.clone().map(|e| alert(Tone::Destructive, e)));

        let body: AnyElement = if first_run {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(setup_steps(vec![
                    SetupStep {
                        title: "Add a key",
                        body: "Generate one here, import a recovery phrase, or connect a hardware wallet.",
                        done: keys > 0,
                        action: Some(("home-step-key", "Add key", Page::LocalKeyAdd)),
                    },
                    SetupStep {
                        title: "Create a wallet",
                        body: "Combine your keys into a single-key, multisig or time-locked wallet.",
                        done: false,
                        action: Some(("home-step-wallet", "New wallet", Page::LocalWalletNew)),
                    },
                    SetupStep {
                        title: "Back it up",
                        body: "Save the wallet backup file when the wallet is created. Multi-key wallets can't be restored from recovery phrases alone.",
                        done: false,
                        action: None,
                    },
                ]))
                .child(
                    div().flex().flex_row().child(
                        button("home-restore", "Restore a wallet from a backup")
                            .ghost()
                            .small()
                            .icon("history")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx)),
                    ),
                )
                .into_any_element()
        } else {
            match wallets {
                None => quiet_note("Loading wallets…").into_any_element(),
                Some(rows) => section("Wallets")
                    .action(
                        button("home-all-wallets", "All wallets")
                            .ghost()
                            .small()
                            .icon_right("arrow-right")
                            .on_click(|_, _, cx| navigate(Page::LocalWallets, cx)),
                    )
                    .child(list(
                        rows.iter()
                            .enumerate()
                            .map(|(i, w)| self.wallet_row(i, w, cx))
                            .collect(),
                    ))
                    .into_any_element(),
            }
        };

        frame
            .child(body)
            .child(self.chart.clone())
            .into_any_element()
    }

    fn wallet_row(&self, ix: usize, w: &WalletRow, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let s = &w.summary;
        let id = s.id.as_str().to_string();
        let page = Page::LocalWallet(id.clone());
        let syncing = store(cx)
            .read(cx)
            .sync_progress
            .get(&id)
            .filter(|p| p.phase != "complete")
            .map(|p| p.percent.min(100));

        let status: SharedString = if s.locked {
            "Locked".into()
        } else if let Some(pct) = syncing {
            format!("Syncing… {pct}%").into()
        } else {
            "Unlocked".into()
        };

        let mut meta = row_meta()
            .child(policy_label(&s.policy_type))
            .child(sep_dot())
            .child(status);
        if s.recovery_only {
            meta = meta
                .child(sep_dot())
                .child(div().text_color(t.warning_ink).child("Recovery-only"));
        }

        // Relative timelock: counted from when each coin last moved. The
        // tip height isn't known here, so say the rule, not a date.
        let recovery = w.recovery_blocks.map(|b| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(t.ink_2)
                .child(icon("clock").size(px(12.)))
                .child(format!(
                    "Recovery key can spend {} after coins last move",
                    describe_timelock(b).trim_start_matches('~')
                ))
        });

        let balance: AnyElement = match (s.locked, w.balance_sat) {
            (true, _) => div()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child("Unlock to see balance")
                .into_any_element(),
            (false, Some(sat)) => div()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_1()
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(14.))
                        .text_color(t.foreground)
                        .child(btc_amount(sat as i64)),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("BTC"),
                )
                .into_any_element(),
            (false, None) => div()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child("—")
                .into_any_element(),
        };

        div()
            .id(("home-wallet", ix))
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .px_5()
            .py_4()
            .cursor_pointer()
            .hover(move |d| d.bg(alpha(t.muted, 0.5)))
            .clickable(Probe::new("row", s.name.clone()), move |_, _, cx| {
                navigate(page.clone(), cx)
            })
            .child(if s.locked {
                row_icon("lock", t.muted_foreground, false)
            } else {
                row_icon("wallet", t.brand_ink, true)
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
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(row_title(s.name.clone()))
                            .child(network_badge(&s.network)),
                    )
                    .child(meta)
                    .children(recovery),
            )
            .child(div().flex_none().child(balance))
            .into_any_element()
    }
}

// ── SigVault Cloud ──

impl HomePage {
    fn render_cloud(&self, cx: &mut Context<Self>) -> AnyElement {
        let sessions = self
            .sessions
            .clone()
            .unwrap_or_else(|| store(cx).read(cx).remote_sessions.clone());
        let mut sorted: Vec<RemoteSession> = sessions;
        sorted.sort_by(|a, b| {
            let ta = a.created_at.as_deref().and_then(parse_time);
            let tb = b.created_at.as_deref().and_then(parse_time);
            tb.cmp(&ta)
        });
        let (open, done): (Vec<_>, Vec<_>) = sorted
            .into_iter()
            .partition(|s| s.status.eq_ignore_ascii_case("active"));

        let open_el: AnyElement = if open.is_empty() {
            quiet_note(
                "Nothing needs you right now. When someone starts a signing or device-registration session that includes you, it shows up here.",
            )
            .into_any_element()
        } else {
            list(
                open.iter()
                    .enumerate()
                    .map(|(i, s)| self.session_row(i, s, true, cx))
                    .collect(),
            )
            .into_any_element()
        };

        let recent: Vec<AnyElement> = done
            .iter()
            .take(5)
            .enumerate()
            .map(|(i, s)| self.session_row(i, s, false, cx))
            .collect();

        page_frame("home")
            .crumb("Dashboard", None)
            .title("Welcome back.")
            .subtitle("Signing sessions for your team's vaults. Keys on your own devices never leave them.")
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .child(
                section("Waiting for action")
                    .description("Open sessions you're part of. Join one to register a device or sign.")
                    .child(open_el),
            )
            .when(!recent.is_empty(), |f| {
                f.child(
                    section("Recent sessions")
                        .action(
                            button("home-all-sessions", "All sessions")
                                .ghost()
                                .small()
                                .icon_right("arrow-right")
                                .on_click(|_, _, cx| navigate(Page::Sessions, cx)),
                        )
                        .child(list(recent)),
                )
            })
            .child(self.chart.clone())
            .into_any_element()
    }

    fn session_row(
        &self,
        ix: usize,
        s: &RemoteSession,
        open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = theme();
        let joining = self.joining.as_deref() == Some(s.id.as_str());
        let busy = self.joining.is_some();
        let status = {
            let mut c = s.status.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => "Unknown".into(),
            }
        };
        let id = s.id.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .px_5()
            .py_3()
            .min_h(px(60.))
            .child(icon_tile(
                session_icon(&s.session_type),
                32.,
                Some(if open { t.primary } else { t.muted_foreground }),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(row_title(session_title(&s.session_type)))
                    .child(
                        row_meta()
                            .child(div().font_family(FONT_MONO).child(short_id(&s.id)))
                            .when_some(s.created_at.clone(), |d, c| {
                                d.child(sep_dot())
                                    .child(format!("Started {}", format_relative(Some(&c))))
                            }),
                    ),
            )
            .child(if open {
                button(("home-join", ix), if joining { "Joining…" } else { "Join" })
                    .loading(joining)
                    .disabled(busy && !joining)
                    .on_click(cx.listener(move |this, _, _, cx| this.join(id.clone(), cx)))
                    .into_any_element()
            } else {
                badge(status, Tone::Secondary).into_any_element()
            })
            .into_any_element()
    }
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.mode {
            AppMode::Local => self.render_local(cx),
            AppMode::Cloud => self.render_cloud(cx),
        }
    }
}
