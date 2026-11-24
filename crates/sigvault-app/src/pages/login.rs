//! `pages/Login.tsx` — OAuth (PKCE) sign-in against the selected env.

use gpui::{div, prelude::*, px, Context, FontWeight, SharedString, Window};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{cmd_authenticate, cmd_list_environments, cmd_navigate};
use sigvault_core::env_config::EnvConfig;
use sigvault_core::State;

use super::cloud::common::{icon_tile, network_badge_unless_named};
use crate::e2e::{E2eClick, Probe};
use crate::runtime::backend;
use crate::theme::{alpha, theme};
use crate::ui::{alert, button, card, divider, page_frame, Tone};

/// (icon, title, body) for the value proposition column.
const VALUE_POINTS: [(&str, &str, &str); 3] = [
    (
        "users",
        "Team multisig",
        "Hold bitcoin with co-signers. Each person keeps their own hardware wallet; spending needs a quorum.",
    ),
    (
        "file-key",
        "Signing sessions",
        "SigVault invites the right signers, collects each signature and tells everyone when a transaction is ready.",
    ),
    (
        "shield",
        "Keys stay on your devices",
        "Keys on your own devices never leave them, and signing happens on the device. If a vault uses a SigVault co-signer key, SigVault holds that one key.",
    ),
];

pub struct LoginPage {
    loading: bool,
    changing_env: bool,
    error: Option<SharedString>,
    current_env: Option<EnvConfig>,
}

impl LoginPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let task = backend(cx)
            .call(|app, st| async move { cmd_list_environments(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let env = task.await.ok().and_then(|resp| {
                let sel = resp.selected_id?;
                resp.environments.into_iter().find(|e| e.id == sel)
            });
            this.update(cx, |this, cx| {
                this.current_env = env;
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            loading: false,
            changing_env: false,
            error: None,
            current_env: None,
        }
    }

    fn change_env(&mut self, cx: &mut Context<Self>) {
        self.changing_env = true;
        self.error = None;
        cx.notify();
        // Navigate to the picker without clearing the persisted env, so
        // SelectEnv pre-selects the current one.
        let task = backend(cx).call(|app, st| async move {
            let window = app.get_webview_window("main").ok_or("no window")?;
            cmd_navigate(window, State::new(&st), "SelectEnv".into()).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.error = Some(e.into());
                }
                this.changing_env = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn login(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        cx.notify();
        let task =
            backend(cx).call(|app, st| async move { cmd_authenticate(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if !r.success => this.error = Some(r.message.into()),
                    Err(e) => this.error = Some(e.into()),
                    _ => {}
                }
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for LoginPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();

        let value = div()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .flex()
            .flex_col()
            .gap_5()
            .pt_1()
            .children(VALUE_POINTS.iter().map(|(icon_name, title, body)| {
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_3()
                    .child(icon_tile(icon_name, 32., Some(t.primary)))
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(*title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(19.))
                                    .text_color(t.muted_foreground)
                                    .child(*body),
                            ),
                    )
            }));

        let env = self.current_env.as_ref();
        let env_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .child(icon_tile("globe", 32., Some(t.muted_foreground)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                env.map(|e| e.name.clone())
                                    .unwrap_or_else(|| "Loading…".into()),
                            ),
                    )
                    .children(env.and_then(|e| network_badge_unless_named(&e.name, &e.network))),
            )
            .child(
                button("change-env", "Change")
                    .outline()
                    .small()
                    .loading(self.changing_env)
                    .disabled(self.changing_env || self.loading)
                    .on_click(cx.listener(|this, _, _, cx| this.change_env(cx))),
            );

        let sign_in = card()
            .w(px(400.))
            .flex_none()
            .overflow_hidden()
            .child(env_row)
            .child(divider())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                    .child(
                        button(
                            "login",
                            if self.loading {
                                "Waiting for browser…"
                            } else {
                                "Continue with SigVault"
                            },
                        )
                        .full_width()
                        .loading(self.loading)
                        .when(!self.loading, |b| b.icon_right("arrow-right"))
                        .on_click(cx.listener(|this, _, _, cx| this.login(cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("Opens your browser · single sign-on"),
                    ),
            );

        let local_link = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .text_size(px(12.))
            .text_color(t.muted_foreground)
            .child("Prefer no account?")
            .child(
                div()
                    .id("use-local")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_color(t.brand_ink)
                    .hover(|s| s.underline())
                    .clickable(
                        Probe::new("link", "Use wallets on this device"),
                        |_, _, cx| crate::shell::switch_space(AppMode::Local, cx),
                    )
                    .child("Use wallets on this device")
                    .child(
                        div()
                            .px(px(5.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(t.border)
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("⌘1"),
                    ),
            );

        page_frame("login")
            .title("Sign in to SigVault Cloud")
            .subtitle("Join signing sessions and coordinate multisig with your team.")
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_10()
                    .child(value)
                    .child(sign_in),
            )
            .child(
                div()
                    .pt_4()
                    .border_t_1()
                    .border_color(alpha(t.border, 0.7))
                    .child(local_link),
            )
    }
}
