//! App chrome, modelled on the web dashboard (`components/ui/navigation/`):
//!
//! - **Top bar** (full width, on canvas): `▣ sigvault / <workspace> [PILL] ⇕ /
//!   <wallet> ⇕ … network | version`. The workspace switcher is the web app's
//!   organization switcher, covering the desktop's two workspaces:
//!   "On this device" (local) and "SigVault Cloud". `⌘1` / `⌘2` switch directly.
//! - **Sidebar** (canvas, collapsible to an icon rail): mono section labels,
//!   workspace nav (or vault nav inside a wallet), "create" shortcuts,
//!   Documentation and the identity card at the bottom.
//!
//! One entity renders both so they share state; the root observes it.

use gpui::{
    anchored, deferred, div, prelude::*, px, AnyElement, App, Context, Corner, Hsla, MouseButton,
    SharedString, Window, WindowControlArea,
};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{
    cmd_get_current_user, cmd_list_environments, cmd_logout, cmd_set_app_mode,
};
use sigvault_core::local_wallet::commands::{cmd_local_get_settings, cmd_local_list_wallets};
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use crate::e2e::{E2e, E2eClick, Probe};
use crate::keychain::{keychain, KeychainEvent};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme, FONT_MONO, RADIUS_CARD, RADIUS_CHIP, RADIUS_CONTROL};
use crate::ui::{icon, logo, pill, spinner};

pub const SIDEBAR_WIDTH: f32 = 248.;
pub const SIDEBAR_WIDTH_COLLAPSED: f32 = 56.;
pub const TOPBAR_HEIGHT: f32 = 48.;
const DOCS_URL: &str = "https://sigvault.org/docs";

/// Display names for the two workspaces (product naming, not backend naming).
pub fn space_meta(mode: AppMode) -> (&'static str, &'static str, &'static str) {
    match mode {
        AppMode::Local => ("On this device", "personal · no account", "hard-drive"),
        AppMode::Cloud => ("SigVault Cloud", "team multisig · sessions", "cloud"),
    }
}

/// Switch workspace (no-op when already there). Shared by the switcher and
/// the `⌘1` / `⌘2` actions.
pub fn switch_space(mode: AppMode, cx: &mut App) {
    if store(cx).read(cx).app_mode == Some(mode) {
        return;
    }
    let task = backend(cx)
        .call(move |app, st| async move { cmd_set_app_mode(app, State::new(&st), mode).await });
    cx.spawn(async move |_| {
        if let Err(e) = task.await {
            log::error!("switch workspace failed: {e}");
        }
    })
    .detach();
}

#[derive(Clone)]
struct WalletItem {
    id: String,
    name: String,
    network: String,
    policy: String,
    locked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Workspace,
    Wallet,
    Account,
}

pub struct AppSidebar {
    current: Page,
    collapsed: bool,
    wallets: Vec<WalletItem>,
    wallets_loaded: bool,
    default_network: Option<SharedString>,
    menu: Option<Menu>,
    switching_to: Option<AppMode>,
    user_name: Option<SharedString>,
    user_email: Option<SharedString>,
    env_name: Option<SharedString>,
    env_network: Option<SharedString>,
    signing_out: bool,
}

impl AppSidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe(&store(cx), |_, _, cx| cx.notify()).detach();
        cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| match ev {
            StoreEvent::LocalDataChanged => this.refresh_local(cx),
            StoreEvent::ModeChanged => {
                this.switching_to = None;
                this.refresh_local(cx);
                this.refresh_cloud(cx);
            }
            StoreEvent::AuthChanged => this.refresh_cloud(cx),
            _ => {}
        })
        .detach();
        cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify())
            .detach();
        let mut this = Self {
            current: Page::Loading,
            collapsed: false,
            wallets: Vec::new(),
            wallets_loaded: false,
            default_network: None,
            menu: None,
            switching_to: None,
            user_name: None,
            user_email: None,
            env_name: None,
            env_network: None,
            signing_out: false,
        };
        this.refresh_local(cx);
        this.refresh_cloud(cx);
        this
    }

    pub fn set_current(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.current != page {
            let refresh =
                !self.wallets_loaded || matches!(page, Page::LocalWallets | Page::LocalHome);
            self.current = page;
            if refresh {
                self.refresh_local(cx);
            }
            self.switching_to = None;
            self.menu = None;
            cx.notify();
        }
    }

    fn refresh_local(&mut self, cx: &mut Context<Self>) {
        if store(cx).read(cx).app_mode != Some(AppMode::Local) {
            return;
        }
        let list = backend(cx)
            .call(|app, st| async move { cmd_local_list_wallets(app, State::new(&st)).await });
        let settings = backend(cx)
            .call(|app, st| async move { cmd_local_get_settings(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let list = list.await;
            let settings = settings.await.ok();
            this.update(cx, |this, cx| {
                if let Ok(list) = list {
                    let mut list: Vec<WalletSummary> = list;
                    list.sort_by_key(|a| a.name.to_lowercase());
                    this.wallets = list
                        .into_iter()
                        .map(|w| WalletItem {
                            id: w.id.as_str().to_string(),
                            name: w.name,
                            network: w.network,
                            policy: w.policy_type,
                            locked: w.locked,
                        })
                        .collect();
                    this.wallets_loaded = true;
                }
                if let Some(s) = settings {
                    this.default_network = Some(s.default_network.into());
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_cloud(&mut self, cx: &mut Context<Self>) {
        let authenticated = store(cx).read(cx).authenticated;
        if !authenticated {
            self.user_name = None;
            self.user_email = None;
            self.signing_out = false;
        }
        let user = authenticated.then(|| {
            backend(cx).call(|_app, st| async move { cmd_get_current_user(State::new(&st)).await })
        });
        let envs = backend(cx)
            .call(|app, st| async move { cmd_list_environments(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let user = match user {
                Some(t) => t.await.ok(),
                None => None,
            };
            let env = envs.await.ok().and_then(|r| {
                let id = r.selected_id?;
                r.environments.into_iter().find(|e| e.id == id)
            });
            this.update(cx, |this, cx| {
                if let Some(u) = user {
                    this.user_name = u.name.or(u.username.clone()).map(Into::into);
                    this.user_email = u.email.map(Into::into);
                }
                this.env_name = env.as_ref().map(|e| e.name.clone().into());
                this.env_network = env.map(|e| e.network.into());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose(&mut self, mode: AppMode, cx: &mut Context<Self>) {
        self.menu = None;
        if store(cx).read(cx).app_mode != Some(mode) {
            self.switching_to = Some(mode);
            switch_space(mode, cx);
        }
        cx.notify();
    }

    fn toggle_menu(&mut self, m: Menu, cx: &mut Context<Self>) {
        self.menu = if self.menu == Some(m) { None } else { Some(m) };
        cx.notify();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        self.signing_out = true;
        self.menu = None;
        cx.notify();
        let task =
            backend(cx).call(|app, st| async move { cmd_logout(app, State::new(&st)).await });
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

    fn active_wallet(&self) -> Option<&WalletItem> {
        let id = match &self.current {
            Page::LocalWallet(id) | Page::LocalReceive(id) | Page::LocalSend(id) => id,
            _ => return None,
        };
        self.wallets.iter().find(|w| &w.id == id)
    }

    fn mode(&self, cx: &App) -> AppMode {
        if self.current.is_cloud() {
            AppMode::Cloud
        } else if self.current.is_local() {
            AppMode::Local
        } else {
            store(cx).read(cx).app_mode.unwrap_or(AppMode::Local)
        }
    }

    // ───────────────────────────── top bar ─────────────────────────────

    /// Full-width top bar. Called from the root's render.
    pub fn render_topbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let mode = self.switching_to.unwrap_or_else(|| self.mode(cx));
        let (space_name, _, _) = space_meta(mode);
        let s = store(cx).read(cx);
        let space_pill: SharedString = match mode {
            AppMode::Local => "local".into(),
            AppMode::Cloud if s.authenticated => "signed in".into(),
            AppMode::Cloud => "signed out".into(),
        };
        let home = match mode {
            AppMode::Local => Page::LocalHome,
            AppMode::Cloud => Page::CloudHome,
        };
        let wallet = self.active_wallet().cloned();
        let network: Option<SharedString> = match mode {
            AppMode::Local => wallet
                .as_ref()
                .map(|w| w.network.clone().into())
                .or(self.default_network.clone()),
            AppMode::Cloud => self.env_network.clone(),
        };
        let traffic_pad = if cfg!(target_os = "macos") {
            px(80.)
        } else {
            px(16.)
        };

        let workspace_items = [AppMode::Local, AppMode::Cloud].map(|m| {
            let (name, meta, _) = space_meta(m);
            let shortcut = match (m, cfg!(target_os = "macos")) {
                (AppMode::Local, true) => "⌘1",
                (AppMode::Cloud, true) => "⌘2",
                (AppMode::Local, false) => "Ctrl+1",
                (AppMode::Cloud, false) => "Ctrl+2",
            };
            let active = m == self.mode(cx);
            let on = cx.listener(move |this, _, _, cx| this.choose(m, cx));
            menu_entry(name, Some(meta.into()), Some(shortcut), active, on).into_any_element()
        });

        let wallet_items: Vec<AnyElement> = self
            .wallets
            .iter()
            .map(|w| {
                let id = w.id.clone();
                let active = wallet.as_ref().is_some_and(|a| a.id == w.id);
                let meta: SharedString =
                    format!("{} · {}", w.network, policy_label(&w.policy)).into();
                let on = cx.listener(move |this, _, _, cx| {
                    this.menu = None;
                    navigate(Page::LocalWallet(id.clone()), cx);
                });
                menu_entry(w.name.clone(), Some(meta), None, active, on).into_any_element()
            })
            .collect();

        let workspace_menu = (self.menu == Some(Menu::Workspace)).then(|| {
            dropdown(
                "workspace-menu",
                "Workspaces",
                workspace_items.into_iter().collect(),
                Some(
                    div()
                        .px_3()
                        .py_2()
                        .text_size(px(11.5))
                        .line_height(px(16.))
                        .text_color(t.ink_3)
                        .child("Your keys never leave your devices in either workspace.")
                        .into_any_element(),
                ),
                cx,
            )
        });
        let wallet_menu = (self.menu == Some(Menu::Wallet)).then(|| {
            let new_wallet = div()
                .id("wallet-menu-new")
                .mx_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(7.))
                .rounded(RADIUS_CHIP)
                .text_size(px(13.))
                .text_color(t.ink_2)
                .cursor_pointer()
                .hover(|s| s.bg(t.hover_strong).text_color(t.ink))
                .clickable(
                    Probe::new("menuitem", "New wallet"),
                    cx.listener(|this, _, _, cx| {
                        this.menu = None;
                        navigate(Page::LocalWalletNew, cx);
                    }),
                )
                .child(icon("plus").size(px(14.)))
                .child("New wallet")
                .into_any_element();
            dropdown("wallet-menu", "Wallets", wallet_items, Some(new_wallet), cx)
        });

        div()
            .id("topbar")
            .flex_none()
            .h(px(TOPBAR_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl(traffic_pad)
            .pr_4()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |ev, window, _| {
                if ev.click_count == 1 {
                    window.start_window_move();
                } else if ev.click_count == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(
                div()
                    .id("brand")
                    .mr_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .rounded(RADIUS_CHIP)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| navigate(home.clone(), cx))
                    .child(logo(px(17.)))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.ink)
                            .child("sigvault"),
                    ),
            )
            .child(separator())
            .child(switcher_segment(
                "ws",
                space_name.into(),
                Some(pill(space_pill).into_any_element()),
                self.menu == Some(Menu::Workspace),
                cx.listener(move |_, _, _, cx| {
                    let home = match mode {
                        AppMode::Local => Page::LocalHome,
                        AppMode::Cloud => Page::CloudHome,
                    };
                    navigate(home, cx);
                }),
                cx.listener(|this, _, _, cx| this.toggle_menu(Menu::Workspace, cx)),
                workspace_menu,
            ))
            .children(wallet.clone().map(|w| {
                let id = w.id.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(separator())
                    .child(switcher_segment(
                        "wallet",
                        w.name.clone().into(),
                        Some(pill(if w.locked { "locked" } else { "unlocked" }).into_any_element()),
                        self.menu == Some(Menu::Wallet),
                        cx.listener(move |_, _, _, cx| navigate(Page::LocalWallet(id.clone()), cx)),
                        cx.listener(|this, _, _, cx| this.toggle_menu(Menu::Wallet, cx)),
                        wallet_menu,
                    ))
            }))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .font_family(FONT_MONO)
                    .text_size(px(11.))
                    .text_color(t.ink_3)
                    .children(network.map(|n| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .child(n)
                            .child(div().w(px(1.)).h(px(12.)).bg(t.line))
                    }))
                    .when(self.switching_to.is_some(), |d| d.child(spinner(px(12.))))
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .into_any_element()
    }

    // ───────────────────────────── sidebar ─────────────────────────────

    fn nav_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = self.collapsed;
        let cur = self.current.clone();
        let mut out: Vec<AnyElement> = Vec::new();
        match self.mode(cx) {
            AppMode::Local => {
                if let Some(w) = self.active_wallet().cloned() {
                    out.push(section_label("vault", c));
                    out.push(nav(
                        "sb-overview",
                        "wallet",
                        "Overview",
                        cur == Page::LocalWallet(w.id.clone()),
                        None,
                        Page::LocalWallet(w.id.clone()),
                        c,
                    ));
                    out.push(nav(
                        "sb-receive",
                        "arrow-down-left",
                        "Receive",
                        cur == Page::LocalReceive(w.id.clone()),
                        None,
                        Page::LocalReceive(w.id.clone()),
                        c,
                    ));
                    out.push(nav(
                        "sb-send",
                        "arrow-up-right",
                        "Send",
                        cur == Page::LocalSend(w.id.clone()),
                        None,
                        Page::LocalSend(w.id.clone()),
                        c,
                    ));
                    out.push(nav(
                        "sb-back",
                        "arrow-left",
                        "All wallets",
                        false,
                        None,
                        Page::LocalWallets,
                        c,
                    ));
                } else {
                    let keys = keychain(cx).read(cx).keys().len();
                    out.push(section_label("workspace", c));
                    out.push(nav(
                        "sb-home",
                        "home",
                        "Dashboard",
                        cur == Page::LocalHome,
                        None,
                        Page::LocalHome,
                        c,
                    ));
                    out.push(nav(
                        "sb-wallets",
                        "wallet",
                        "Wallets",
                        cur == Page::LocalWallets,
                        Some(self.wallets.len()),
                        Page::LocalWallets,
                        c,
                    ));
                    out.push(nav(
                        "sb-keys",
                        "key",
                        "Keys",
                        matches!(cur, Page::LocalKeys | Page::LocalKey(_)),
                        Some(keys),
                        Page::LocalKeys,
                        c,
                    ));
                    out.push(nav(
                        "sb-settings",
                        "settings",
                        "Settings",
                        cur == Page::LocalSettings,
                        None,
                        Page::LocalSettings,
                        c,
                    ));
                    out.push(spacer());
                    out.push(section_label("create", c));
                    out.push(nav(
                        "sb-new-wallet",
                        "wallet-plus",
                        "New wallet",
                        cur == Page::LocalWalletNew,
                        None,
                        Page::LocalWalletNew,
                        c,
                    ));
                    out.push(nav(
                        "sb-add-key",
                        "key-plus",
                        "Add key",
                        cur == Page::LocalKeyAdd,
                        None,
                        Page::LocalKeyAdd,
                        c,
                    ));
                    out.push(nav(
                        "sb-restore",
                        "history",
                        "Restore wallet",
                        cur == Page::LocalWalletRecover,
                        None,
                        Page::LocalWalletRecover,
                        c,
                    ));
                }
            }
            AppMode::Cloud => {
                let s = store(cx).read(cx);
                if !s.authenticated {
                    out.push(section_label("get started", c));
                    out.push(nav(
                        "sb-env",
                        "globe",
                        "Network",
                        cur == Page::SelectEnv,
                        None,
                        Page::SelectEnv,
                        c,
                    ));
                    out.push(nav(
                        "sb-login",
                        "user",
                        "Sign in",
                        cur == Page::Login,
                        None,
                        Page::Login,
                        c,
                    ));
                } else {
                    let total = s.remote_sessions.len();
                    let live = s.active_session.is_connected || cur == Page::SessionDetails;
                    out.push(section_label("workspace", c));
                    out.push(nav(
                        "sb-chome",
                        "home",
                        "Dashboard",
                        cur == Page::CloudHome,
                        None,
                        Page::CloudHome,
                        c,
                    ));
                    out.push(nav(
                        "sb-sessions",
                        "users",
                        "Sessions",
                        cur == Page::Sessions,
                        Some(total),
                        Page::Sessions,
                        c,
                    ));
                    if live {
                        out.push(nav(
                            "sb-live",
                            "radio",
                            "Live session",
                            cur == Page::SessionDetails,
                            None,
                            Page::SessionDetails,
                            c,
                        ));
                    }
                    out.push(nav(
                        "sb-csettings",
                        "settings",
                        "Settings",
                        cur == Page::CloudSettings,
                        None,
                        Page::CloudSettings,
                        c,
                    ));
                }
            }
        }
        out
    }

    fn identity_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let c = self.collapsed;
        let mode = self.mode(cx);
        let authenticated = store(cx).read(cx).authenticated;

        let (initials, line1, line2): (SharedString, SharedString, SharedString) = match mode {
            AppMode::Cloud if authenticated => {
                let name = self
                    .user_name
                    .clone()
                    .or(self.user_email.clone())
                    .unwrap_or_else(|| "Signed in".into());
                (
                    initials_of(&name),
                    name,
                    self.user_email
                        .clone()
                        .unwrap_or_else(|| self.env_name.clone().unwrap_or_default()),
                )
            }
            AppMode::Cloud => (
                "—".into(),
                "Not signed in".into(),
                self.env_name
                    .clone()
                    .unwrap_or_else(|| "SigVault Cloud".into()),
            ),
            AppMode::Local => {
                let kc = keychain(cx).read(cx);
                let status = if !kc.has_password() {
                    "keychain not set up"
                } else if kc.is_unlocked() {
                    "keychain unlocked"
                } else {
                    "keychain locked"
                };
                ("◆".into(), "This computer".into(), status.into())
            }
        };

        let menu = (self.menu == Some(Menu::Account)).then(|| {
            let mut items: Vec<AnyElement> = Vec::new();
            match mode {
                AppMode::Local => {
                    let kc = keychain(cx).read(cx);
                    if kc.is_unlocked() {
                        items.push(menu_action(
                            "acct-lock",
                            "lock",
                            "Lock keychain",
                            false,
                            cx.listener(|this, _, _, cx| {
                                this.menu = None;
                                keychain(cx).update(cx, |k, cx| k.lock(cx));
                            }),
                        ));
                    }
                    items.push(menu_action(
                        "acct-lsettings",
                        "settings",
                        "Settings",
                        false,
                        cx.listener(|this, _, _, cx| {
                            this.menu = None;
                            navigate(Page::LocalSettings, cx);
                        }),
                    ));
                }
                AppMode::Cloud => {
                    items.push(menu_action(
                        "acct-csettings",
                        "settings",
                        "Settings",
                        false,
                        cx.listener(|this, _, _, cx| {
                            this.menu = None;
                            navigate(Page::CloudSettings, cx);
                        }),
                    ));
                    if authenticated {
                        items.push(menu_action(
                            "acct-signout",
                            "log-out",
                            if self.signing_out {
                                "Signing out…"
                            } else {
                                "Sign out"
                            },
                            true,
                            cx.listener(|this, _, _, cx| this.sign_out(cx)),
                        ));
                    }
                }
            }
            deferred(
                anchored()
                    .anchor(Corner::BottomLeft)
                    .snap_to_window()
                    .child(
                        div()
                            .id("account-menu")
                            .e2e(Probe::new("overlay", "account menu"))
                            .occlude()
                            .mb(px(56.))
                            .w(px(SIDEBAR_WIDTH - 16.))
                            .py_1()
                            .rounded(RADIUS_CARD)
                            .border_1()
                            .border_color(t.line_tertiary)
                            .bg(t.surface_3)
                            .shadow_lg()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.menu = None;
                                cx.notify();
                            }))
                            .children(items),
                    ),
            )
            .with_priority(10)
        });

        div()
            .relative()
            .mx_2()
            .child(
                div()
                    .id("identity")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .p(px(6.))
                    .rounded(RADIUS_CONTROL)
                    .bg(t.inset)
                    .border_1()
                    .border_color(t.line)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover_strong))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .clickable(
                        Probe::new("button", "Account"),
                        cx.listener(|this, _, _, cx| this.toggle_menu(Menu::Account, cx)),
                    )
                    .child(
                        div()
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(RADIUS_CHIP)
                            .border_1()
                            .border_color(t.line_strong)
                            .bg(t.surface)
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(t.brand_ink)
                            .child(initials),
                    )
                    .when(!c, |d| {
                        d.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(13.5))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.ink)
                                        .child(line1),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(11.5))
                                        .text_color(t.ink_2)
                                        .child(line2),
                                ),
                        )
                        .child(icon("more-vertical").size(px(15.)).text_color(t.ink_3))
                    }),
            )
            .children(menu)
            .into_any_element()
    }
}

impl Render for AppSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let c = self.collapsed;
        let width = if c {
            SIDEBAR_WIDTH_COLLAPSED
        } else {
            SIDEBAR_WIDTH
        };
        let sections = self.nav_sections(cx);
        let identity = self.identity_card(cx);

        div()
            .id("app-sidebar")
            .flex_none()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .pb(px(10.))
            .child(
                div()
                    .flex_none()
                    .h(px(40.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .when(c, |d| d.justify_center())
                    .when(!c, |d| d.justify_end().pr_2())
                    .child(
                        div()
                            .id("collapse")
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(RADIUS_CONTROL)
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover_strong))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.collapsed = !this.collapsed;
                                cx.notify();
                            }))
                            .child(icon("panel-left").size(px(18.)).text_color(t.ink_3)),
                    ),
            )
            .child(
                div()
                    .id("sidebar-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(sections),
            )
            .child(
                div()
                    .flex_none()
                    .pt_3()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .id("sb-docs")
                            .child(nav_row(
                                "sb-docs-row",
                                icon_el("book-open", false),
                                "Documentation",
                                false,
                                None,
                                c,
                            ))
                            .clickable(Probe::new("nav", "Documentation"), |_, _, cx| {
                                cx.open_url(DOCS_URL)
                            }),
                    )
                    .child(div().mx_2().my(px(10.)).h(px(1.)).bg(t.line))
                    .child(identity),
            )
    }
}

// ───────────────────────────── pieces ─────────────────────────────

/// Plain draggable title bar for full-window pages (Loading, Welcome).
pub fn bare_titlebar() -> impl IntoElement {
    div()
        .id("bare-titlebar")
        .flex_none()
        .h(px(TOPBAR_HEIGHT))
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

fn initials_of(name: &str) -> SharedString {
    let mut parts = name
        .split(|c: char| c.is_whitespace() || c == '@' || c == '.')
        .filter(|p| !p.is_empty());
    let a = parts.next().and_then(|p| p.chars().next());
    let b = parts.next().and_then(|p| p.chars().next());
    match (a, b) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()).into(),
        (Some(a), None) => a.to_uppercase().to_string().into(),
        _ => "?".into(),
    }
}

fn policy_label(policy: &str) -> &str {
    match policy {
        "singlesig_hot" | "singlesig" | "singlesig_hw" => "single key",
        "multisig" => "multisig",
        "timelocked" => "timelocked",
        "watch_only" => "watch-only",
        other => other,
    }
}

fn separator() -> impl IntoElement {
    div()
        .px_1()
        .text_size(px(14.))
        .text_color(alpha(theme().ink_3, 0.6))
        .child("/")
}

/// Neon-style switcher segment: the name navigates, the ⇕ button opens the
/// picker (web `Switcher`).
fn switcher_segment(
    id: &'static str,
    label: SharedString,
    pill_el: Option<AnyElement>,
    open: bool,
    on_label: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    menu: Option<AnyElement>,
) -> impl IntoElement {
    let t = theme();
    div()
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .id(SharedString::from(format!("{id}-label")))
                .max_w(px(200.))
                .truncate()
                .px_1()
                .rounded(RADIUS_CHIP)
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.ink)
                .cursor_pointer()
                .hover(move |s| s.text_color(t.brand_ink))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .clickable(Probe::new("button", label.to_string()), on_label)
                .child(label),
        )
        .children(pill_el)
        .child(
            div()
                .id(SharedString::from(format!("{id}-toggle")))
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(RADIUS_CHIP)
                .cursor_pointer()
                .when(open, |d| d.bg(t.hover_strong))
                .hover(|s| s.bg(t.hover_strong))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .clickable(
                    Probe::new(
                        "button",
                        if id == "ws" {
                            "Workspace menu"
                        } else {
                            "Wallet menu"
                        },
                    ),
                    on_toggle,
                )
                .child(icon("chevrons-up-down").size(px(14.)).text_color(t.ink_3)),
        )
        .children(menu)
}

/// Dropdown panel anchored under a switcher segment (web `DropdownMenuContent`).
fn dropdown(
    id: &'static str,
    label: &'static str,
    items: Vec<AnyElement>,
    footer: Option<AnyElement>,
    cx: &mut Context<AppSidebar>,
) -> AnyElement {
    let t = theme();
    deferred(
        anchored().anchor(Corner::TopLeft).snap_to_window().child(
            div()
                .id(id)
                .e2e(Probe::new("overlay", id))
                .occlude()
                .mt(px(30.))
                .w(px(272.))
                .py_1()
                .rounded(RADIUS_CARD)
                .border_1()
                .border_color(t.line_tertiary)
                .bg(t.surface_3)
                .shadow_lg()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .px_3()
                        .pt(px(6.))
                        .pb_1()
                        .font_family(FONT_MONO)
                        .text_size(px(10.))
                        .text_color(t.ink_3)
                        .child(label.to_uppercase()),
                )
                .children(items)
                .children(footer.map(|f| {
                    div()
                        .mt_1()
                        .pt_1()
                        .border_t_1()
                        .border_color(t.line)
                        .child(f)
                })),
        ),
    )
    .with_priority(10)
    .into_any_element()
}

fn menu_entry(
    label: impl Into<SharedString>,
    meta: Option<SharedString>,
    shortcut: Option<&'static str>,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = theme();
    let label: SharedString = label.into();
    div()
        .id(label.clone())
        .mx_1()
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
        .px_2()
        .py(px(7.))
        .rounded(RADIUS_CHIP)
        .cursor_pointer()
        .hover(|s| s.bg(t.hover_strong))
        .clickable(
            Probe::new("menuitem", label.to_string()).selected(active),
            on_click,
        )
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
                        .text_size(px(14.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.ink)
                        .child(label),
                )
                .children(meta.map(|m| {
                    div()
                        .truncate()
                        .font_family(FONT_MONO)
                        .text_size(px(10.))
                        .text_color(t.ink_3)
                        .child(m.to_uppercase())
                })),
        )
        .child(if active {
            icon("check")
                .size(px(15.))
                .text_color(t.brand_ink)
                .mt(px(2.))
                .into_any_element()
        } else {
            div()
                .mt(px(2.))
                .font_family(FONT_MONO)
                .text_size(px(10.))
                .text_color(t.ink_4)
                .children(shortcut)
                .into_any_element()
        })
}

fn menu_action(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    danger: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
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
        .rounded(RADIUS_CHIP)
        .text_size(px(13.5))
        .text_color(t.ink_2)
        .cursor_pointer()
        .hover(move |s| {
            s.bg(t.hover_strong)
                .text_color(if danger { t.destructive } else { t.ink })
        })
        .clickable(Probe::new("menuitem", label), on_click)
        .child(icon(icon_name).size(px(15.)))
        .child(label)
        .into_any_element()
}

fn section_label(label: &'static str, collapsed: bool) -> AnyElement {
    let t = theme();
    if collapsed {
        return div().mx_3().my_2().h(px(1.)).bg(t.line).into_any_element();
    }
    div()
        .mx_2()
        .h(px(28.))
        .flex()
        .items_center()
        .px_2()
        .font_family(FONT_MONO)
        .text_size(px(9.5))
        .text_color(t.ink_3)
        .child(label.to_uppercase())
        .into_any_element()
}

fn spacer() -> AnyElement {
    div().h(px(10.)).into_any_element()
}

fn icon_el(name: &'static str, active: bool) -> AnyElement {
    let t = theme();
    icon(name)
        .size(px(18.))
        .text_color(if active { t.brand_ink } else { t.ink_3 })
        .into_any_element()
}

fn nav_row(
    id: impl Into<SharedString>,
    leading: AnyElement,
    label: impl Into<SharedString>,
    active: bool,
    trailing: Option<AnyElement>,
    collapsed: bool,
) -> gpui::Stateful<gpui::Div> {
    let t = theme();
    let id: SharedString = id.into();
    let label: SharedString = label.into();
    div()
        .id(id)
        .mx_2()
        .h(px(32.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .px_2()
        .rounded(RADIUS_CONTROL)
        .cursor_pointer()
        .text_size(px(14.))
        .text_color(if active { t.ink } else { t.ink_2 })
        .when(active, |d| {
            d.bg(t.hover_strong).font_weight(gpui::FontWeight::MEDIUM)
        })
        .when(!active, |d| {
            d.hover(|s| s.text_color(t.ink).bg(alpha(t.hover_strong, 0.6)))
        })
        .when(collapsed, |d| d.justify_center())
        .child(
            div()
                .size(px(20.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(leading),
        )
        .when(!collapsed, |d| {
            d.child(div().flex_1().min_w_0().truncate().child(label))
                .children(trailing)
        })
}

fn nav(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    active: bool,
    count: Option<usize>,
    page: Page,
    collapsed: bool,
) -> AnyElement {
    let t = theme();
    nav_row(
        id,
        icon_el(icon_name, active),
        label,
        active,
        count.map(|n| {
            div()
                .font_family(FONT_MONO)
                .text_size(px(11.))
                .text_color(t.ink_3)
                .child(n.to_string())
                .into_any_element()
        }),
        collapsed,
    )
    .clickable(
        Probe::new("nav", label).selected(active),
        move |_, _, cx| navigate(page.clone(), cx),
    )
    .into_any_element()
}

#[allow(dead_code)]
fn network_color(network: &str) -> Hsla {
    let t = theme();
    match network {
        "bitcoin" | "mainnet" => t.bitcoin,
        "testnet" | "testnet4" => t.success,
        "signet" => t.warning,
        _ => t.brand,
    }
}
