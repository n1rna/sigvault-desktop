//! `pages/SelectEnv.tsx` — pick a SigVault deployment (regtest / signet /
//! testnet / mainnet) from the remote manifest.

use gpui::{div, prelude::*, px, Context, SharedString, Window};
use sigvault_core::commands::{cmd_list_environments, cmd_set_environment};
use sigvault_core::env_config::EnvConfig;
use sigvault_core::State;

use crate::runtime::backend;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, icon, logo, Tone};

pub struct SelectEnvPage {
    environments: Vec<EnvConfig>,
    selected_id: Option<String>,
    loading: bool,
    submitting: bool,
    error: Option<SharedString>,
}

impl SelectEnvPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            environments: vec![],
            selected_id: None,
            loading: true,
            submitting: false,
            error: None,
        };
        this.load(cx);
        this
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        cx.notify();
        let task = backend(cx).call(|app, st| async move {
            cmd_list_environments(app, State::new(&st)).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(resp) => {
                        let first = resp.environments.iter().find(|e| !e.coming_soon).map(|e| e.id.clone());
                        this.selected_id = resp.selected_id.or(first);
                        this.environments = resp.environments;
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

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(env_id) = self.selected_id.clone() else {
            return;
        };
        self.submitting = true;
        self.error = None;
        cx.notify();
        let task = backend(cx).call(move |app, st| async move {
            cmd_set_environment(app, State::new(&st), env_id).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if !r.success => this.error = Some(r.message.into()),
                    Err(e) => this.error = Some(e.into()),
                    _ => {}
                }
                this.submitting = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for SelectEnvPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let list = if self.loading {
            div()
                .rounded_md()
                .border_1()
                .border_color(t.border)
                .bg(t.card)
                .px_4()
                .py_3()
                .text_size(px(13.))
                .text_color(t.muted_foreground)
                .child("Loading environments…")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .children(self.environments.iter().map(|env| {
                    let disabled = env.coming_soon;
                    let selected = self.selected_id.as_deref() == Some(env.id.as_str());
                    let id = env.id.clone();
                    let host = env
                        .api_base_url
                        .trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .to_string();
                    div()
                        .id(SharedString::from(env.id.clone()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .w_full()
                        .rounded_md()
                        .border_1()
                        .px_4()
                        .py_3()
                        .map(|d| {
                            if disabled {
                                d.border_color(alpha(t.border, 0.5)).bg(alpha(t.muted, 0.2)).opacity(0.6)
                            } else if selected {
                                d.border_color(t.primary).bg(alpha(t.primary, 0.06)).cursor_pointer()
                            } else {
                                d.border_color(t.border)
                                    .bg(t.card)
                                    .cursor_pointer()
                                    .hover(|s| s.border_color(alpha(t.primary, 0.5)))
                            }
                        })
                        .when(!disabled, |d| {
                            d.on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_id = Some(id.clone());
                                cx.notify();
                            }))
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(div().text_size(px(14.)).font_weight(gpui::FontWeight::MEDIUM).child(env.name.clone()))
                                .child(
                                    div()
                                        .mt(px(2.))
                                        .flex()
                                        .flex_row()
                                        .gap_1()
                                        .text_size(px(12.))
                                        .text_color(t.muted_foreground)
                                        .child(format!("{} ·", env.network))
                                        .child(div().font_family(FONT_MONO).child(host)),
                                ),
                        )
                        .map(|d| {
                            if disabled {
                                d.child(badge("Coming soon", Tone::Secondary))
                            } else if selected {
                                d.child(icon("check").text_color(t.primary))
                            } else {
                                d
                            }
                        })
                }))
                .into_any_element()
        };

        div().id("select-env").size_full().overflow_y_scroll().child(
            div().min_h_full().flex().items_center().justify_center().child(
                div()
                    .w_full()
                    .max_w(px(460.))
                    .px_10()
                    .py_12()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .child(logo(px(26.)))
                            .child(div().text_size(px(15.)).font_weight(gpui::FontWeight::SEMIBOLD).child("SigVault")),
                    )
                    .child(div().mt_8().text_size(px(26.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Choose a network"))
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(13.))
                            .line_height(px(21.))
                            .text_color(t.muted_foreground)
                            .child("Pick which SigVault deployment to connect to. You can switch later by signing out."),
                    )
                    .children(self.error.clone().map(|e| div().mt_6().child(alert(Tone::Destructive, e))))
                    .child(div().mt_8().child(list))
                    .child(
                        div().mt_8().child(
                            button("env-continue", if self.submitting { "Applying…" } else { "Continue" })
                                .large()
                                .full_width()
                                .loading(self.submitting)
                                .icon_right("arrow-right")
                                .disabled(self.selected_id.is_none() || self.loading)
                                .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                        ),
                    )
                    .child(
                        div().mt_3().child(
                            button("env-refresh", "Refresh list")
                                .ghost()
                                .small()
                                .full_width()
                                .disabled(self.loading || self.submitting)
                                .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                        ),
                    ),
            ),
        )
    }
}
