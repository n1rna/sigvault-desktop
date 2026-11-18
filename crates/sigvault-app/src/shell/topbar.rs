//! Global top bar (`components/TopBar.tsx`): brand + version, Local ⇄ Cloud
//! context switcher, account menu. Window controls are native (traffic
//! lights on macOS, system decorations elsewhere); the bar doubles as the
//! window drag region.

use gpui::{
    anchored, deferred, div, prelude::*, px, Context, Corner, MouseButton, SharedString,
    Window, WindowControlArea,
};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{cmd_get_current_user, cmd_logout, cmd_set_app_mode};
use sigvault_core::State;

use crate::runtime::backend;
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme};
use crate::ui::{icon, logo};

pub struct TopBar {
    switcher_open: bool,
    account_open: bool,
    signing_out: bool,
    user_label: Option<SharedString>,
}

impl TopBar {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let st = store(cx);
        cx.observe(&st, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&st, |this, _, ev: &StoreEvent, cx| {
            if matches!(ev, StoreEvent::AuthChanged) {
                this.refresh_user(cx);
            }
        })
        .detach();
        Self {
            switcher_open: false,
            account_open: false,
            signing_out: false,
            user_label: None,
        }
    }

    fn refresh_user(&mut self, cx: &mut Context<Self>) {
        let authenticated = store(cx).read(cx).authenticated;
        if !authenticated {
            self.user_label = None;
            self.signing_out = false;
            cx.notify();
            return;
        }
        let task = backend(cx).call(|_app, st| async move {
            cmd_get_current_user(State::new(&st)).await
        });
        cx.spawn(async move |this, cx| {
            let user = task.await.ok();
            this.update(cx, |this, cx| {
                this.user_label = user.and_then(|u| u.email.or(u.name).or(u.username)).map(Into::into);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_mode(&mut self, mode: AppMode, cx: &mut Context<Self>) {
        self.switcher_open = false;
        cx.notify();
        if store(cx).read(cx).app_mode == Some(mode) {
            return;
        }
        let task = backend(cx).call(move |app, st| async move {
            cmd_set_app_mode(app, State::new(&st), mode).await
        });
        cx.spawn(async move |_, _| {
            if let Err(e) = task.await {
                log::error!("set app mode failed: {e}");
            }
        })
        .detach();
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
}

fn mode_meta(mode: AppMode) -> (&'static str, &'static str) {
    match mode {
        AppMode::Local => ("Local Wallets", "On-device · no account"),
        AppMode::Cloud => ("SigVault Cloud", "Team multisig · sessions"),
    }
}

impl Render for TopBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let s = store(cx).read(cx);
        let app_mode = s.app_mode;
        let authenticated = s.authenticated;
        let traffic_light_pad = if cfg!(target_os = "macos") { px(84.) } else { px(16.) };
        let _ = window;

        let mode_color = match app_mode {
            Some(AppMode::Cloud) => t.primary,
            Some(AppMode::Local) => t.accent,
            None => alpha(t.muted_foreground, 0.5),
        };
        let current = app_mode.map(|m| mode_meta(m).0).unwrap_or("Choose context");

        // ── Context switcher ──
        let switcher = div()
            .relative()
            .child(
                div()
                    .id("ctx-switcher")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(32.))
                    .w(px(176.))
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(alpha(t.card, 0.6))
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .cursor_pointer()
                    .hover(|s| s.border_color(alpha(t.primary, 0.4)).text_color(t.foreground))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.switcher_open = !this.switcher_open;
                        this.account_open = false;
                        cx.notify();
                    }))
                    .child(div().size(px(6.)).rounded_full().bg(mode_color))
                    .child(div().flex_1().truncate().child(current))
                    .child(icon(if self.switcher_open { "chevron-up" } else { "chevron-down" }).size(px(12.))),
            )
            .when(self.switcher_open, |d| {
                d.child(deferred(
                    anchored().anchor(Corner::TopRight).snap_to_window().child(
                        div()
                            .id("ctx-menu")
                            .occlude()
                            .mt(px(38.))
                            .w(px(256.))
                            .rounded_lg()
                            .border_1()
                            .border_color(t.border)
                            .bg(t.card)
                            .shadow_xl()
                            .overflow_hidden()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.switcher_open = false;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .border_b_1()
                                    .border_color(alpha(t.border, 0.6))
                                    .px_3()
                                    .py_2()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(alpha(t.muted_foreground, 0.7))
                                    .child("Switch context"),
                            )
                            .children([AppMode::Local, AppMode::Cloud].into_iter().map(|mode| {
                                let (title, hint) = mode_meta(mode);
                                let active = Some(mode) == app_mode;
                                let dot = if mode == AppMode::Cloud { t.primary } else { t.accent };
                                div()
                                    .id(title)
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .px_3()
                                    .py(px(10.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(alpha(t.muted, 0.6)))
                                    .when(active, |d| d.bg(alpha(t.primary, 0.06)))
                                    .on_click(cx.listener(move |this, _, _, cx| this.set_mode(mode, cx)))
                                    .child(div().size(px(6.)).rounded_full().bg(dot))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .child(div().text_size(px(12.5)).font_weight(gpui::FontWeight::MEDIUM).text_color(t.foreground).child(title))
                                            .child(div().text_size(px(11.)).text_color(alpha(t.muted_foreground, 0.7)).child(hint)),
                                    )
                                    .when(active, |d| d.child(icon("check").size(px(14.)).text_color(t.primary)))
                            })),
                    ),
                ))
            });

        // ── Account ──
        let account = if !authenticated {
            div().child(
                div()
                    .id("sign-in")
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(t.border)
                    .bg(alpha(t.card, 0.6))
                    .cursor_pointer()
                    .hover(|s| s.border_color(alpha(t.primary, 0.4)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.set_mode(AppMode::Cloud, cx)))
                    .child(icon("user").size(px(16.))),
            )
        } else {
            let label: SharedString = self.user_label.clone().unwrap_or_else(|| "Signed in".into());
            let initial: SharedString = label
                .trim()
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".into())
                .into();
            let avatar = |id: &'static str| {
                div()
                    .id(id)
                    .size(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(alpha(t.primary, 0.3))
                    .bg(alpha(t.primary, 0.12))
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.primary)
                    .child(initial.clone())
            };
            div()
                .relative()
                .child(
                    avatar("account")
                        .cursor_pointer()
                        .hover(|s| s.border_color(alpha(t.primary, 0.6)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.account_open = !this.account_open;
                            this.switcher_open = false;
                            cx.notify();
                        })),
                )
                .when(self.account_open, |d| {
                    d.child(deferred(
                        anchored().anchor(Corner::TopRight).snap_to_window().child(
                            div()
                                .id("account-menu")
                                .occlude()
                                .mt(px(38.))
                                .w(px(256.))
                                .rounded_lg()
                                .border_1()
                                .border_color(t.border)
                                .bg(t.card)
                                .shadow_xl()
                                .overflow_hidden()
                                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                    this.account_open = false;
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_3()
                                        .border_b_1()
                                        .border_color(alpha(t.border, 0.6))
                                        .px_3()
                                        .py_3()
                                        .child(avatar("account-avatar"))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .child(div().text_size(px(11.)).text_color(alpha(t.muted_foreground, 0.7)).child("Signed in as"))
                                                .child(div().truncate().text_size(px(12.5)).font_weight(gpui::FontWeight::MEDIUM).child(label.clone())),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("sign-out")
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(10.))
                                        .px_3()
                                        .py(px(10.))
                                        .text_size(px(13.))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.muted_foreground)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(alpha(t.muted, 0.6)).text_color(t.destructive))
                                        .on_click(cx.listener(|this, _, _, cx| this.sign_out(cx)))
                                        .child(icon("log-out").size(px(14.)))
                                        .child(if self.signing_out { "Signing out…" } else { "Sign out" }),
                                ),
                        ),
                    ))
                })
        };

        div()
            .id("topbar")
            .flex()
            .flex_row()
            .items_center()
            .h(px(56.))
            .flex_none()
            .border_b_1()
            .border_color(t.border)
            .bg(alpha(t.card, 0.8))
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |ev, window, _| {
                if ev.click_count == 1 {
                    window.start_window_move();
                }
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .pl(traffic_light_pad)
                    .pr_5()
                    .child(logo(px(18.)))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_baseline()
                            .gap_2()
                            .child(div().text_size(px(14.)).font_weight(gpui::FontWeight::SEMIBOLD).child("SigVault"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(alpha(t.muted_foreground, 0.6))
                                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                            ),
                    ),
            )
            .child(div().flex_1())
            .child(div().flex().items_center().px_3().child(switcher))
            .child(
                div()
                    .flex()
                    .items_center()
                    .border_l_1()
                    .border_color(alpha(t.border, 0.6))
                    .px_3()
                    .child(account),
            )
    }
}
