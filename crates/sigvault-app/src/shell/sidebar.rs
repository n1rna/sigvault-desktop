//! App sidebar with a workspace switcher.
//!
//! Two spaces, one at a time (Slack-style):
//! - **On this device** — local wallets, key library, keychain, settings.
//! - **SigVault Cloud** — signing sessions with your team, account, settings.
//!
//! The switcher at the top replaces the old top-bar dropdown. Switching calls
//! `cmd_set_app_mode`, exactly like the dropdown did, so the backend's mode
//! gating and routing are unchanged. `⌘1` / `⌘2` switch directly.

use gpui::{
    anchored, deferred, div, prelude::*, px, AnyElement, App, Context, Corner, Hsla, MouseButton,
    SharedString, Window, WindowControlArea,
};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{cmd_get_current_user, cmd_list_environments, cmd_logout, cmd_set_app_mode};
use sigvault_core::local_wallet::commands::cmd_local_list_wallets;
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use crate::keychain::{keychain, KeychainEvent};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme};
use crate::ui::{icon, spinner};

pub const SIDEBAR_WIDTH: f32 = 240.;
/// Height of the window-chrome strip (traffic lights on macOS).
pub const CHROME_HEIGHT: f32 = 40.;

/// Display names for the two spaces (product naming, not backend naming).
pub fn space_meta(mode: AppMode) -> (&'static str, &'static str, &'static str) {
    match mode {
        AppMode::Local => ("On this device", "Personal wallets · no account", "hard-drive"),
        AppMode::Cloud => ("SigVault Cloud", "Team multisig · signing sessions", "cloud"),
    }
}

pub fn space_color(mode: AppMode) -> Hsla {
    match mode {
        AppMode::Local => theme().accent,
        AppMode::Cloud => theme().primary,
    }
}

/// Switch space (no-op when already there). Shared by the switcher and the
/// `⌘1` / `⌘2` actions.
pub fn switch_space(mode: AppMode, cx: &mut App) {
    if store(cx).read(cx).app_mode == Some(mode) {
        return;
    }
    let task = backend(cx).call(move |app, st| async move {
        cmd_set_app_mode(app, State::new(&st), mode).await
    });
    cx.spawn(async move |_| {
        if let Err(e) = task.await {
            log::error!("switch space failed: {e}");
        }
    })
    .detach();
}

#[derive(Clone)]
struct WalletItem {
    id: String,
    name: String,
    network: String,
    locked: bool,
}

pub struct AppSidebar {
    current: Page,
    wallets: Vec<WalletItem>,
    wallets_loaded: bool,
    switcher_open: bool,
    account_open: bool,
    switching_to: Option<AppMode>,
    user_label: Option<SharedString>,
    env_label: Option<SharedString>,
    signing_out: bool,
}

impl AppSidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe(&store(cx), |_, _, cx| cx.notify()).detach();
        cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| match ev {
            StoreEvent::LocalDataChanged => this.refresh_wallets(cx),
            StoreEvent::ModeChanged => {
                this.switching_to = None;
                this.refresh_wallets(cx);
                this.refresh_cloud(cx);
            }
            StoreEvent::AuthChanged => this.refresh_cloud(cx),
            _ => {}
        })
        .detach();
        cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()).detach();
        let mut this = Self {
            current: Page::Loading,
            wallets: Vec::new(),
            wallets_loaded: false,
            switcher_open: false,
            account_open: false,
            switching_to: None,
            user_label: None,
            env_label: None,
            signing_out: false,
        };
        this.refresh_wallets(cx);
        this
    }

    pub fn set_current(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.current != page {
            let refresh = !self.wallets_loaded || matches!(page, Page::LocalWallets);
            if page.is_cloud() != self.current.is_cloud() && page.is_cloud() {
                self.refresh_cloud(cx);
            }
            self.current = page;
            if refresh {
                self.refresh_wallets(cx);
            }
            // A page change finishes any pending switch.
            self.switching_to = None;
            cx.notify();
        }
    }

    fn refresh_wallets(&mut self, cx: &mut Context<Self>) {
        if store(cx).read(cx).app_mode != Some(AppMode::Local) {
            return;
        }
        let task = backend(cx).call(|app, st| async move {
            cmd_local_list_wallets(app, State::new(&st)).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(list) = task.await else { return };
            this.update(cx, |this, cx| {
                let mut list: Vec<WalletSummary> = list;
                list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                this.wallets = list
                    .into_iter()
                    .map(|w| WalletItem {
                        id: w.id.as_str().to_string(),
                        name: w.name,
                        network: w.network,
                        locked: w.locked,
                    })
                    .collect();
                this.wallets_loaded = true;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Account label + environment name for the cloud footer.
    fn refresh_cloud(&mut self, cx: &mut Context<Self>) {
        let authenticated = store(cx).read(cx).authenticated;
        if !authenticated {
            self.user_label = None;
            self.signing_out = false;
        }
        let user = authenticated.then(|| {
            backend(cx).call(|_app, st| async move { cmd_get_current_user(State::new(&st)).await })
        });
        let envs = backend(cx).call(|app, st| async move {
            cmd_list_environments(app, State::new(&st)).await
        });
        cx.spawn(async move |this, cx| {
            let label = match user {
                Some(t) => t.await.ok().and_then(|u| u.email.or(u.name).or(u.username)),
                None => None,
            };
            let env = envs.await.ok().and_then(|r| {
                let id = r.selected_id?;
                r.environments.into_iter().find(|e| e.id == id).map(|e| e.name)
            });
            this.update(cx, |this, cx| {
                if label.is_some() {
                    this.user_label = label.map(Into::into);
                }
                this.env_label = env.map(Into::into);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose(&mut self, mode: AppMode, cx: &mut Context<Self>) {
        self.switcher_open = false;
        if store(cx).read(cx).app_mode != Some(mode) {
            self.switching_to = Some(mode);
            switch_space(mode, cx);
        }
        cx.notify();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        self.signing_out = true;
        self.account_open = false;
        cx.notify();
        let task = backend(cx).call(|app, st| async move { cmd_logout(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if res.is_err() {
                    this.signing_out = false;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn active_wallet(&self) -> Option<&str> {
        match &self.current {
            Page::LocalWallet(id) | Page::LocalReceive(id) | Page::LocalSend(id) => Some(id),
            _ => None,
        }
    }

    // ── pieces ──

    fn render_switcher(&self, mode: Option<AppMode>, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let shown = self.switching_to.or(mode).unwrap_or(AppMode::Local);
        let (name, _, icon_name) = space_meta(shown);
        let color = space_color(shown);
        let subtitle: SharedString = match shown {
            AppMode::Local => match self.wallets.len() {
                1 => "1 wallet".into(),
                n => format!("{n} wallets").into(),
            },
            AppMode::Cloud => match (&self.env_label, store(cx).read(cx).authenticated) {
                (Some(env), true) => format!("{env} · signed in").into(),
                (Some(env), false) => format!("{env} · signed out").into(),
                (None, _) => "Not connected".into(),
            },
        };

        let menu_row = |m: AppMode, cx: &mut Context<Self>| {
            let (name, hint, icon_name) = space_meta(m);
            let active = Some(m) == mode;
            let c = space_color(m);
            let shortcut = if cfg!(target_os = "macos") {
                if m == AppMode::Local { "⌘1" } else { "⌘2" }
            } else if m == AppMode::Local {
                "Ctrl+1"
            } else {
                "Ctrl+2"
            };
            div()
                .id(name)
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .mx_1()
                .px_2()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(alpha(t.muted, 0.9)))
                .when(active, |d| d.bg(alpha(c, 0.08)))
                .on_click(cx.listener(move |this, _, _, cx| this.choose(m, cx)))
                .child(space_tile(icon_name, c, 30.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM).text_color(t.foreground).child(name))
                        .child(div().text_size(px(11.5)).text_color(t.muted_foreground).child(hint)),
                )
                .child(if active {
                    icon("check").size(px(14.)).text_color(c).into_any_element()
                } else {
                    div().text_size(px(11.)).text_color(alpha(t.muted_foreground, 0.7)).child(shortcut).into_any_element()
                })
        };

        div()
            .relative()
            .px_2()
            .child(
                div()
                    .id("space-switcher")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .p(px(6.))
                    .rounded_lg()
                    .border_1()
                    .border_color(if self.switcher_open { alpha(color, 0.5) } else { alpha(t.border, 0.8) })
                    .bg(alpha(t.background, 0.5))
                    .cursor_pointer()
                    .hover(|s| s.border_color(alpha(t.primary, 0.35)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.switcher_open = !this.switcher_open;
                        this.account_open = false;
                        cx.notify();
                    }))
                    .child(space_tile(icon_name, color, 32.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(t.muted_foreground)
                                    .child(subtitle),
                            ),
                    )
                    .child(if self.switching_to.is_some() {
                        spinner(px(14.)).into_any_element()
                    } else {
                        icon("chevrons-up-down").size(px(14.)).into_any_element()
                    }),
            )
            .when(self.switcher_open, |d| {
                d.child(deferred(
                    anchored().anchor(Corner::TopLeft).snap_to_window().child(
                        div()
                            .id("space-menu")
                            .occlude()
                            .mt(px(52.))
                            .w(px(SIDEBAR_WIDTH + 40.))
                            .py_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(t.border)
                            .bg(t.popover)
                            .shadow_xl()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.switcher_open = false;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .px_3()
                                    .pt_1()
                                    .pb_1()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(alpha(t.muted_foreground, 0.75))
                                    .child("Switch workspace"),
                            )
                            .child(menu_row(AppMode::Local, cx))
                            .child(menu_row(AppMode::Cloud, cx))
                            .child(
                                div()
                                    .mt_1()
                                    .mx_3()
                                    .pt_2()
                                    .pb_1()
                                    .border_t_1()
                                    .border_color(alpha(t.border, 0.7))
                                    .text_size(px(11.))
                                    .line_height(px(15.))
                                    .text_color(alpha(t.muted_foreground, 0.8))
                                    .child("Your keys never leave your devices in either workspace."),
                            ),
                    ),
                ))
            })
    }

    fn local_nav(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = theme();
        let kc = keychain(cx).read(cx);
        let key_count = kc.keys().len();
        let cur = self.current.clone();
        let active_wallet = self.active_wallet().map(str::to_string);

        let wallets = self.wallets.iter().map(|w| {
            let active = active_wallet.as_deref() == Some(w.id.as_str());
            row(
                SharedString::from(format!("sb-w-{}", w.id)),
                div()
                    .size(px(15.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(7.)).rounded_full().bg(network_color(&w.network)))
                    .into_any_element(),
                w.name.clone(),
                active,
                Some(
                    icon(if w.locked { "lock" } else { "unlock" })
                        .size(px(12.))
                        .text_color(if w.locked { alpha(t.muted_foreground, 0.6) } else { t.success })
                        .into_any_element(),
                ),
                Page::LocalWallet(w.id.clone()),
            )
            .into_any_element()
        });

        let mut out = vec![
            section_header("WALLETS", Some(("sb-new-wallet", Page::LocalWalletNew))).into_any_element(),
            nav_item("sb-all-wallets", "layers", "All wallets", cur == Page::LocalWallets, Some(count(self.wallets.len())), Page::LocalWallets).into_any_element(),
        ];
        out.extend(wallets);
        out.push(nav_item("sb-restore", "history", "Restore wallet", cur == Page::LocalWalletRecover, None, Page::LocalWalletRecover).into_any_element());
        out.push(section_header("KEYS", Some(("sb-add-key", Page::LocalKeyAdd))).into_any_element());
        out.push(nav_item("sb-keys", "key", "Key library", matches!(cur, Page::LocalKeys | Page::LocalKey(_)), Some(count(key_count)), Page::LocalKeys).into_any_element());
        out.push(nav_item("sb-add-key-item", "plus", "Add key", cur == Page::LocalKeyAdd, None, Page::LocalKeyAdd).into_any_element());
        out
    }

    fn cloud_nav(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = theme();
        let s = store(cx).read(cx);
        let cur = self.current.clone();
        if !s.authenticated {
            return vec![
                section_header("GET STARTED", None).into_any_element(),
                nav_item("sb-env", "globe", "Network", cur == Page::SelectEnv, None, Page::SelectEnv).into_any_element(),
                nav_item("sb-login", "user", "Sign in", cur == Page::Login, None, Page::Login).into_any_element(),
                div()
                    .mx_3()
                    .mt_4()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(alpha(t.primary, 0.25))
                    .bg(alpha(t.primary, 0.05))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(12.5)).font_weight(gpui::FontWeight::SEMIBOLD).child("Multisig with your team"))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(16.))
                            .text_color(t.muted_foreground)
                            .child("Join signing sessions from any device. SigVault coordinates — it never holds your keys."),
                    )
                    .into_any_element(),
            ];
        }
        let active = s.remote_sessions.iter().filter(|r| r.status.eq_ignore_ascii_case("active")).count();
        let live = s.active_session.is_connected;
        let session_id = s.active_session.session_id.clone();
        let mut out = vec![
            section_header("SIGNING", None).into_any_element(),
            nav_item(
                "sb-sessions",
                "users",
                "Sessions",
                cur == Page::Sessions,
                Some(if active > 0 { pill(active) } else { count(s.remote_sessions.len()) }),
                Page::Sessions,
            )
            .into_any_element(),
        ];
        if live || cur == Page::SessionDetails {
            let label = session_id
                .map(|id| format!("Session {}", id.chars().take(8).collect::<String>()))
                .unwrap_or_else(|| "Current session".into());
            out.push(
                row(
                    "sb-live",
                    div()
                        .size(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(7.)).rounded_full().bg(if live { t.success } else { t.muted_foreground }))
                        .into_any_element(),
                    label,
                    cur == Page::SessionDetails,
                    Some(
                        div()
                            .text_size(px(10.5))
                            .text_color(if live { t.success } else { t.muted_foreground })
                            .child(if live { "LIVE" } else { "…" })
                            .into_any_element(),
                    ),
                    Page::SessionDetails,
                )
                .into_any_element(),
            );
        }
        out
    }

    fn local_footer(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = theme();
        let kc = keychain(cx).read(cx);
        let unlocked = kc.is_unlocked();
        let mut out = Vec::new();
        if kc.has_password() {
            out.push(
                div()
                    .id("sb-keychain")
                    .mx_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(9.))
                    .h(px(28.))
                    .px_2()
                    .rounded_md()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .when(unlocked, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(alpha(t.muted, 0.9)))
                            .on_click(|_, _, cx| keychain(cx).update(cx, |k, cx| k.lock(cx)))
                    })
                    .child(icon(if unlocked { "unlock" } else { "lock" }).size(px(13.)).text_color(if unlocked { t.success } else { t.muted_foreground }))
                    .child(div().flex_1().child(if unlocked { "Keychain unlocked" } else { "Keychain locked" }))
                    .when(unlocked, |d| d.child(div().text_size(px(11.)).text_color(t.primary).child("Lock")))
                    .into_any_element(),
            );
        }
        out.push(nav_item("sb-settings", "settings", "Settings", self.current == Page::LocalSettings, None, Page::LocalSettings).into_any_element());
        out
    }

    fn cloud_footer(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = theme();
        let authenticated = store(cx).read(cx).authenticated;
        let mut out = Vec::new();
        if authenticated {
            let label: SharedString = self.user_label.clone().unwrap_or_else(|| "Signed in".into());
            let initial: SharedString = label.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()).into();
            out.push(
                div()
                    .relative()
                    .child(
                        div()
                            .id("sb-account")
                            .mx_2()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(9.))
                            .px_2()
                            .py(px(6.))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(alpha(t.muted, 0.9)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.account_open = !this.account_open;
                                this.switcher_open = false;
                                cx.notify();
                            }))
                            .child(avatar(initial.clone(), 26.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().truncate().text_size(px(12.5)).text_color(t.foreground).child(label.clone()))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(t.muted_foreground)
                                            .child(self.env_label.clone().unwrap_or_else(|| "SigVault Cloud".into())),
                                    ),
                            )
                            .child(icon("chevrons-up-down").size(px(13.))),
                    )
                    .when(self.account_open, |d| {
                        d.child(deferred(
                            anchored().anchor(Corner::BottomLeft).snap_to_window().child(
                                div()
                                    .id("account-menu")
                                    .occlude()
                                    .mb(px(48.))
                                    .ml_2()
                                    .w(px(SIDEBAR_WIDTH - 16.))
                                    .py_1()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(t.border)
                                    .bg(t.popover)
                                    .shadow_xl()
                                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                        this.account_open = false;
                                        cx.notify();
                                    }))
                                    .child(menu_item("acct-settings", "settings", "Cloud settings", false, cx.listener(|this, _, _, cx| {
                                        this.account_open = false;
                                        navigate(Page::CloudSettings, cx);
                                    })))
                                    .child(menu_item("acct-signout", "log-out", if self.signing_out { "Signing out…" } else { "Sign out" }, true, cx.listener(|this, _, _, cx| this.sign_out(cx)))),
                            ),
                        ))
                    })
                    .into_any_element(),
            );
            out.push(nav_item("sb-cloud-settings", "settings", "Settings", self.current == Page::CloudSettings, None, Page::CloudSettings).into_any_element());
        }
        out
    }
}

impl Render for AppSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let mode = store(cx).read(cx).app_mode;
        let cloud = self.current.is_cloud();
        let nav = if cloud { self.cloud_nav(cx) } else { self.local_nav(cx) };
        let footer = if cloud { self.cloud_footer(cx) } else { self.local_footer(cx) };

        div()
            .id("app-sidebar")
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(alpha(t.border, 0.7))
            .bg(alpha(t.card, 0.6))
            // Window chrome strip: traffic lights live here; drag to move.
            .child(chrome_strip("sb-chrome"))
            .child(self.render_switcher(mode, cx))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .pt_1()
                    .pb_3()
                    .children(nav),
            )
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(alpha(t.border, 0.7))
                    .py_2()
                    .children(footer)
                    .child(
                        div()
                            .px_4()
                            .pt_1()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_size(px(10.5))
                            .text_color(alpha(t.muted_foreground, 0.55))
                            .child(crate::ui::logo(px(10.)))
                            .child(format!("SigVault v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
    }
}

/// Empty, draggable strip at the top of a column (sidebar or content) that
/// stands in for a title bar.
pub fn chrome_strip(id: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .h(px(CHROME_HEIGHT))
        .w_full()
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |ev, window, _| {
            if ev.click_count == 1 {
                window.start_window_move();
            } else if ev.click_count == 2 {
                window.titlebar_double_click();
            }
        })
}

fn space_tile(icon_name: &'static str, color: Hsla, size: f32) -> impl IntoElement {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(alpha(color, 0.14))
        .border_1()
        .border_color(alpha(color, 0.3))
        .child(icon(icon_name).size(px(size * 0.5)).text_color(color))
}

fn avatar(initial: SharedString, size: f32) -> impl IntoElement {
    let t = theme();
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(alpha(t.primary, 0.3))
        .bg(alpha(t.primary, 0.12))
        .text_size(px(11.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.primary)
        .child(initial)
}

fn count(n: usize) -> AnyElement {
    div()
        .text_size(px(11.))
        .text_color(alpha(theme().muted_foreground, 0.8))
        .child(n.to_string())
        .into_any_element()
}

fn pill(n: usize) -> AnyElement {
    let t = theme();
    div()
        .px(px(6.))
        .rounded_full()
        .bg(alpha(t.primary, 0.18))
        .text_size(px(10.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.primary)
        .child(n.to_string())
        .into_any_element()
}

fn section_header(label: &'static str, add: Option<(&'static str, Page)>) -> impl IntoElement {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .pt_4()
        .pb_1()
        .child(
            div()
                .text_size(px(10.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(alpha(t.muted_foreground, 0.75))
                .child(label),
        )
        .children(add.map(|(id, page)| {
            div()
                .id(id)
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(t.muted))
                .on_click(move |_, _, cx| navigate(page.clone(), cx))
                .child(icon("plus").size(px(13.)))
        }))
}

fn row(
    id: impl Into<SharedString>,
    leading: AnyElement,
    label: impl Into<SharedString>,
    active: bool,
    trailing: Option<AnyElement>,
    page: Page,
) -> impl IntoElement {
    let t = theme();
    let id: SharedString = id.into();
    div()
        .id(id)
        .mx_2()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.))
        .h(px(30.))
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(if active { t.foreground } else { t.muted_foreground })
        .when(active, |d| d.bg(alpha(t.primary, 0.12)))
        .when(!active, |d| d.hover(|s| s.bg(alpha(t.muted, 0.9)).text_color(t.foreground)))
        .on_click(move |_, _, cx| navigate(page.clone(), cx))
        .child(leading)
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .children(trailing)
}

fn nav_item(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    active: bool,
    trailing: Option<AnyElement>,
    page: Page,
) -> impl IntoElement {
    let t = theme();
    row(
        id,
        icon(icon_name)
            .size(px(15.))
            .text_color(if active { t.primary } else { t.muted_foreground })
            .into_any_element(),
        label,
        active,
        trailing,
        page,
    )
}

fn menu_item(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    danger: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = theme();
    div()
        .id(id)
        .mx_1()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.))
        .px_2()
        .py(px(7.))
        .rounded_md()
        .text_size(px(13.))
        .text_color(t.muted_foreground)
        .cursor_pointer()
        .hover(move |s| s.bg(alpha(t.muted, 0.9)).text_color(if danger { t.destructive } else { t.foreground }))
        .on_click(on_click)
        .child(icon(icon_name).size(px(14.)))
        .child(label)
}

fn network_color(network: &str) -> Hsla {
    let t = theme();
    match network {
        "bitcoin" | "mainnet" => t.bitcoin,
        "testnet" | "testnet4" => t.success,
        "signet" => t.warning,
        _ => t.accent,
    }
}
