//! Local-mode sidebar: wallets, key library, settings. Always visible on
//! `/local/*` pages so local mode navigates like a desktop app rather than a
//! sequence of web pages.

use gpui::{div, prelude::*, px, Context, Hsla, SharedString, Window};
use sigvault_core::local_wallet::commands::cmd_local_list_wallets;
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use crate::keychain::{keychain, KeychainEvent};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme};
use crate::ui::icon;

pub const SIDEBAR_WIDTH: f32 = 236.;

#[derive(Clone)]
struct WalletItem {
    id: String,
    name: String,
    network: String,
    locked: bool,
}

pub struct LocalSidebar {
    current: Page,
    wallets: Vec<WalletItem>,
    loaded: bool,
}

impl LocalSidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| match ev {
            StoreEvent::LocalDataChanged | StoreEvent::ModeChanged => this.refresh(cx),
            StoreEvent::SyncProgress(_) => {}
            _ => {}
        })
        .detach();
        cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()).detach();
        let mut this = Self { current: Page::LocalWallets, wallets: Vec::new(), loaded: false };
        this.refresh(cx);
        this
    }

    pub fn set_current(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.current != page {
            // Wallet lock state may have changed on the page we're leaving.
            let refresh = !self.loaded || matches!(page, Page::LocalWallets);
            self.current = page;
            if refresh {
                self.refresh(cx);
            }
            cx.notify();
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
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
                this.loaded = true;
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
}

fn section_header(label: &'static str, add: Option<(&'static str, Page)>) -> impl IntoElement {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_3()
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

fn nav_item(
    id: impl Into<SharedString>,
    icon_name: &'static str,
    label: impl Into<SharedString>,
    active: bool,
    trailing: Option<gpui::AnyElement>,
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
        .child(
            icon(icon_name)
                .size(px(15.))
                .text_color(if active { t.primary } else { t.muted_foreground }),
        )
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .children(trailing)
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

impl Render for LocalSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let kc = keychain(cx).read(cx);
        let key_count = kc.keys().len();
        let kc_unlocked = kc.is_unlocked();
        let kc_has_pw = kc.has_password();
        let active_wallet = self.active_wallet().map(str::to_string);
        let cur = self.current.clone();

        let count_badge = |n: usize| {
            div()
                .text_size(px(11.))
                .text_color(alpha(t.muted_foreground, 0.8))
                .child(n.to_string())
                .into_any_element()
        };

        let wallets = div().flex().flex_col().gap(px(1.)).children(self.wallets.iter().map(|w| {
            let active = active_wallet.as_deref() == Some(w.id.as_str());
            let page = Page::LocalWallet(w.id.clone());
            let id = SharedString::from(format!("sb-w-{}", w.id));
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
                .child(
                    div()
                        .size(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(7.)).rounded_full().bg(network_color(&w.network))),
                )
                .child(div().flex_1().min_w_0().truncate().child(w.name.clone()))
                .child(
                    icon(if w.locked { "lock" } else { "unlock" })
                        .size(px(12.))
                        .text_color(if w.locked { alpha(t.muted_foreground, 0.6) } else { t.success }),
                )
        }));

        div()
            .id("local-sidebar")
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(alpha(t.border, 0.7))
            .bg(alpha(t.card, 0.55))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .pb_3()
                    .child(section_header("WALLETS", Some(("sb-new-wallet", Page::LocalWalletNew))))
                    .child(nav_item(
                        "sb-all-wallets",
                        "layers",
                        "All wallets",
                        cur == Page::LocalWallets,
                        Some(count_badge(self.wallets.len())),
                        Page::LocalWallets,
                    ))
                    .child(wallets)
                    .child(nav_item(
                        "sb-restore",
                        "history",
                        "Restore wallet",
                        cur == Page::LocalWalletRecover,
                        None,
                        Page::LocalWalletRecover,
                    ))
                    .child(section_header("KEYS", Some(("sb-add-key", Page::LocalKeyAdd))))
                    .child(nav_item(
                        "sb-keys",
                        "key",
                        "Key library",
                        matches!(cur, Page::LocalKeys | Page::LocalKey(_)),
                        Some(count_badge(key_count)),
                        Page::LocalKeys,
                    ))
                    .child(nav_item(
                        "sb-add-key-item",
                        "plus",
                        "Add key",
                        cur == Page::LocalKeyAdd,
                        None,
                        Page::LocalKeyAdd,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(alpha(t.border, 0.7))
                    .py_2()
                    .when(kc_has_pw, |d| {
                        d.child(
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
                                .when(kc_unlocked, |d| {
                                    d.cursor_pointer()
                                        .hover(|s| s.bg(alpha(t.muted, 0.9)))
                                        .on_click(|_, _, cx| {
                                            keychain(cx).update(cx, |k, cx| k.lock(cx));
                                        })
                                })
                                .child(
                                    icon(if kc_unlocked { "unlock" } else { "lock" })
                                        .size(px(13.))
                                        .text_color(if kc_unlocked { t.success } else { t.muted_foreground }),
                                )
                                .child(div().flex_1().child(if kc_unlocked {
                                    "Keychain unlocked"
                                } else {
                                    "Keychain locked"
                                }))
                                .when(kc_unlocked, |d| {
                                    d.child(div().text_size(px(11.)).text_color(t.primary).child("Lock"))
                                }),
                        )
                    })
                    .child(nav_item(
                        "sb-settings",
                        "settings",
                        "Settings",
                        cur == Page::LocalSettings,
                        None,
                        Page::LocalSettings,
                    )),
            )
    }
}
