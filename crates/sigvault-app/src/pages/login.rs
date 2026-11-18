//! `pages/Login.tsx` — OAuth (PKCE) sign-in against the selected env.

use gpui::{div, prelude::*, px, Context, SharedString, Window};
use sigvault_core::commands::{cmd_authenticate, cmd_list_environments, cmd_navigate};
use sigvault_core::env_config::EnvConfig;
use sigvault_core::State;

use crate::runtime::backend;
use crate::theme::{alpha, theme};
use crate::ui::{alert, badge, button, icon, logo, Tone};

const VALUE_POINTS: [&str; 3] = [
    "Ledger, Trezor, BitBox, Coldcard",
    "Remote signing ceremonies",
    "Non-custodial by design",
];

pub struct LoginPage {
    loading: bool,
    changing_env: bool,
    error: Option<SharedString>,
    current_env: Option<EnvConfig>,
}

impl LoginPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let task = backend(cx).call(|app, st| async move {
            cmd_list_environments(app, State::new(&st)).await
        });
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
        let task = backend(cx).call(|app, st| async move { cmd_authenticate(app, State::new(&st)).await });
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
        let aside = div()
            .w(relative_44())
            .flex()
            .flex_col()
            .justify_between()
            .border_r_1()
            .border_color(t.border)
            .bg(alpha(t.muted, 0.2))
            .p_10()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .child(logo(px(26.)))
                    .child(div().text_size(px(15.)).font_weight(gpui::FontWeight::SEMIBOLD).child("SigVault")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(26.)).line_height(px(32.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Multisig Bitcoin, done right."))
                    .child(
                        div()
                            .mt_4()
                            .max_w(px(340.))
                            .text_size(px(13.))
                            .line_height(px(21.))
                            .text_color(t.muted_foreground)
                            .child("Coordinate hardware wallets across signers. Your keys never leave your devices — SigVault is the conductor, not the custodian."),
                    )
                    .child(div().mt_6().flex().flex_col().gap(px(10.)).children(VALUE_POINTS.iter().map(|p| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .text_size(px(13.))
                            .text_color(t.muted_foreground)
                            .child(icon("check").size(px(14.)).text_color(t.primary))
                            .child(*p)
                    }))),
            );

        let env_box = div()
            .mt_6()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .rounded_md()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .px(px(14.))
            .py(px(10.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(11.)).text_color(t.muted_foreground).child("Connecting to"))
                    .child(
                        div()
                            .mt(px(2.))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(self.current_env.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "—".into()))
                            .children(self.current_env.as_ref().map(|e| badge(e.network.clone(), Tone::Outline))),
                    ),
            )
            .child(
                button("change-env", if self.changing_env { "…" } else { "Change" })
                    .outline()
                    .small()
                    .disabled(self.changing_env || self.loading)
                    .on_click(cx.listener(|this, _, _, cx| this.change_env(cx))),
            );

        div()
            .size_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(aside)
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    div()
                        .w_full()
                        .max_w(px(400.))
                        .px_10()
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(26.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Welcome back"))
                        .child(
                            div()
                                .mt_2()
                                .text_size(px(13.))
                                .line_height(px(21.))
                                .text_color(t.muted_foreground)
                                .child("Sign in with your SigVault account to join sessions and coordinate with other signers."),
                        )
                        .child(env_box)
                        .children(self.error.clone().map(|e| div().mt_6().child(alert(Tone::Destructive, e))))
                        .child(
                            div().mt_8().child(
                                button("login", if self.loading { "Opening browser…" } else { "Continue with SigVault" })
                                    .large()
                                    .full_width()
                                    .loading(self.loading)
                                    .icon_right("arrow-right")
                                    .on_click(cx.listener(|this, _, _, cx| this.login(cx))),
                            ),
                        )
                        .child(
                            div()
                                .mt_4()
                                .text_center()
                                .text_size(px(12.))
                                .text_color(alpha(t.muted_foreground, 0.8))
                                .child("Opens your browser · single sign-on"),
                        ),
                ),
            )
    }
}

fn relative_44() -> gpui::DefiniteLength {
    gpui::relative(0.44)
}
