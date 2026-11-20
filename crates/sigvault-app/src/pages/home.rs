//! Workspace dashboards (web `app/(dashboard)/page.tsx`): "Welcome back.",
//! stat strip, Bitcoin price, getting-started tiles. One page, two flavours:
//! "On this device" (wallets / keys / keychain) and "SigVault Cloud"
//! (sessions).

use gpui::{div, prelude::*, px, AppContext, Context, Entity, SharedString, Window};
use sigvault_core::app_mode::AppMode;
use sigvault_core::local_wallet::commands::cmd_local_list_wallets;
use sigvault_core::State;

use crate::keychain::{keychain, KeychainEvent};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{store, StoreEvent};
use crate::theme::{theme, FONT_MONO, RADIUS_CARD};
use crate::ui::price_chart::PriceChart;
use crate::ui::{button, icon, page_frame, section_head, stat_grid};

pub struct HomePage {
    mode: AppMode,
    wallets: Option<(usize, usize)>, // (total, unlocked)
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
            chart: cx.new(PriceChart::new),
        };
        this.load(cx);
        this
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        if self.mode != AppMode::Local {
            return;
        }
        let task = backend(cx)
            .call(|app, st| async move { cmd_local_list_wallets(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            if let Ok(list) = task.await {
                let unlocked = list.iter().filter(|w| !w.locked).count();
                this.update(cx, |this, cx| {
                    this.wallets = Some((list.len(), unlocked));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

struct Tip {
    n: &'static str,
    icon: &'static str,
    title: &'static str,
    body: &'static str,
    page: Page,
}

fn tiles(tips: Vec<Tip>) -> impl IntoElement {
    let t = theme();
    let n = tips.len();
    div()
        .flex()
        .flex_row()
        .rounded(RADIUS_CARD)
        .border_1()
        .border_color(t.line)
        .overflow_hidden()
        .children(tips.into_iter().enumerate().map(move |(i, tip)| {
            let page = tip.page.clone();
            div()
                .id(tip.n)
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .flex()
                .flex_col()
                .justify_between()
                .p_6()
                .bg(t.page)
                .cursor_pointer()
                .hover(|s| s.bg(t.surface_2))
                .when(i + 1 < n, |d| d.border_r_1().border_color(t.line))
                .on_click(move |_, _, cx| navigate(page.clone(), cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .font_family(FONT_MONO)
                                .text_size(px(10.))
                                .text_color(t.ink_2)
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .child(icon(tip.icon).size(px(14.)).text_color(t.brand_ink))
                                        .child("STEP"),
                                )
                                .child(tip.n),
                        )
                        .child(
                            div()
                                .mt_5()
                                .text_size(px(17.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.ink)
                                .child(tip.title),
                        )
                        .child(
                            div()
                                .mt_2()
                                .text_size(px(13.5))
                                .line_height(px(20.))
                                .text_color(t.ink_2)
                                .child(tip.body),
                        ),
                )
                .child(
                    div()
                        .mt_6()
                        .pt_4()
                        .border_t_1()
                        .border_color(t.line)
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .font_family(FONT_MONO)
                        .text_size(px(10.))
                        .text_color(t.ink_2)
                        .child("OPEN")
                        .child(icon("arrow-up-right").size(px(14.))),
                )
        }))
}

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = |v: String| -> SharedString { v.into() };
        let (subtitle, stats, tips, cta) = match self.mode {
            AppMode::Local => {
                let kc = keychain(cx).read(cx);
                let keys = kc.keys().len();
                let hot = kc.hot_count();
                let (total, unlocked) = self.wallets.unwrap_or((0, 0));
                let dash = |v: usize| {
                    if self.wallets.is_some() {
                        v.to_string()
                    } else {
                        "…".into()
                    }
                };
                (
                    "Your wallets, your keys, your signatures — on this computer.",
                    vec![
                        (s("Wallets".into()), s(dash(total)), s("on this device".into())),
                        (s("Keys".into()), s(keys.to_string()), s(format!("{hot} private · {} watch-only", keys - hot))),
                        (s("Unlocked".into()), s(dash(unlocked)), s("ready to sign".into())),
                    ],
                    vec![
                        Tip { n: "01", icon: "key", title: "Add your keys", body: "Generate a key, import a recovery phrase, or connect a hardware wallet.", page: Page::LocalKeyAdd },
                        Tip { n: "02", icon: "shield", title: "Create a vault", body: "Combine keys into single-key, multisig or time-locked wallets.", page: Page::LocalWalletNew },
                        Tip { n: "03", icon: "clock", title: "Restore a wallet", body: "Bring back a wallet from its backup file or descriptor.", page: Page::LocalWalletRecover },
                    ],
                    ("New wallet", Page::LocalWalletNew),
                )
            }
            AppMode::Cloud => {
                let st = store(cx).read(cx);
                let total = st.remote_sessions.len();
                let active = st
                    .remote_sessions
                    .iter()
                    .filter(|r| r.status.eq_ignore_ascii_case("active"))
                    .count();
                let live = if st.active_session.is_connected { 1 } else { 0 };
                (
                    "Your team's vaults, coordinated — SigVault never holds your keys.",
                    vec![
                        (s("Sessions".into()), s(total.to_string()), s("across your workspace".into())),
                        (s("Active".into()), s(active.to_string()), s("awaiting signers".into())),
                        (s("Connected".into()), s(live.to_string()), s("live session".into())),
                    ],
                    vec![
                        Tip { n: "01", icon: "users", title: "Join a session", body: "Pick up a signing or device-registration session your team started.", page: Page::Sessions },
                        Tip { n: "02", icon: "fingerprint", title: "Hardware signing", body: "Ledger, BitBox02, Jade, Trezor and Coldcard sign right from this app.", page: Page::Sessions },
                        Tip { n: "03", icon: "hard-drive", title: "Wallets on this device", body: "Keep personal wallets locally too — no account needed (⌘1).", page: Page::LocalHome },
                    ],
                    ("View sessions", Page::Sessions),
                )
            }
        };
        let (cta_label, cta_page) = cta;

        page_frame("home")
            .crumb("Dashboard", None)
            .title("Welcome back.")
            .subtitle(subtitle)
            .action(
                button("home-cta", cta_label)
                    .icon(if self.mode == AppMode::Local {
                        "plus"
                    } else {
                        "arrow-right"
                    })
                    .on_click(move |_, _, cx| navigate(cta_page.clone(), cx)),
            )
            .child(stat_grid(stats))
            .child(self.chart.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(section_head("Getting started", None))
                    .child(tiles(tips)),
            )
    }
}
