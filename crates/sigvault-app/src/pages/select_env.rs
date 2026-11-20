//! `pages/SelectEnv.tsx` — pick a SigVault deployment (regtest / signet /
//! testnet / mainnet) from the remote manifest.

use gpui::{div, prelude::*, px, Context, FontWeight, SharedString, Window};
use sigvault_core::commands::{cmd_list_environments, cmd_set_environment};
use sigvault_core::env_config::EnvConfig;
use sigvault_core::State;

use super::cloud::common::{icon_tile, list_row, network_badge_unless_named, skeleton_list};
use crate::e2e::{E2e, E2eClick, Probe};
use crate::runtime::backend;
use crate::state::store;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, badge, button, card, divider, footer_bar, icon, page_frame, section, Tone, FORM_WIDTH,
};

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
        let task = backend(cx)
            .call(|app, st| async move { cmd_list_environments(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(resp) => {
                        let first = resp
                            .environments
                            .iter()
                            .find(|e| !e.coming_soon)
                            .map(|e| e.id.clone());
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

fn host_of(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

impl SelectEnvPage {
    fn render_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme();
        if self.loading && self.environments.is_empty() {
            return skeleton_list("env-skeleton", 3).into_any_element();
        }
        if self.environments.is_empty() {
            return card()
                .items_center()
                .gap_1()
                .py_10()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child("No networks available"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Check your internet connection, then refresh the list."),
                )
                .into_any_element();
        }
        let mut list = card().overflow_hidden();
        for (ix, env) in self.environments.iter().enumerate() {
            if ix > 0 {
                list = list.child(divider());
            }
            let disabled = env.coming_soon;
            let selected = self.selected_id.as_deref() == Some(env.id.as_str());
            let id = env.id.clone();
            let radio = div()
                .size(px(16.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(if selected { t.primary } else { t.line_control })
                .when(selected, |d| {
                    d.child(div().size(px(8.)).rounded_full().bg(t.primary))
                });
            list = list.child(
                list_row(("env", ix))
                    .h(px(60.))
                    .gap_4()
                    .when(selected, |d| d.bg(alpha(t.primary, 0.06)))
                    .when(disabled, |d| {
                        d.bg(alpha(t.muted, 0.35))
                            .e2e(Probe::new("option", env.name.clone()).disabled(true))
                    })
                    .when(!disabled, |d| {
                        d.cursor_pointer().clickable(
                            Probe::new("option", env.name.clone()).selected(selected),
                            cx.listener(move |this, _, _, cx| {
                                this.selected_id = Some(id.clone());
                                cx.notify();
                            }),
                        )
                    })
                    .child(radio)
                    .child(icon_tile(
                        "globe",
                        32.,
                        Some(if selected {
                            t.primary
                        } else {
                            t.muted_foreground
                        }),
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
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child(env.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(host_of(&env.api_base_url)),
                            ),
                    )
                    .children(network_badge_unless_named(&env.name, &env.network))
                    .child(div().w(px(96.)).flex_none().flex().justify_end().map(|d| {
                        if disabled {
                            d.child(badge("Coming soon", Tone::Secondary))
                        } else if selected {
                            d.child(badge("Selected", Tone::Default))
                        } else {
                            d
                        }
                    })),
            );
        }
        list.into_any_element()
    }
}

impl Render for SelectEnvPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let authenticated = store(cx).read(cx).authenticated;
        let selected_name = self
            .environments
            .iter()
            .find(|e| Some(&e.id) == self.selected_id.as_ref())
            .map(|e| e.name.clone());
        let footer = footer_bar(
            vec![div()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(match &selected_name {
                    Some(n) => format!("You'll sign in to {n} next."),
                    None => "Choose a network to continue.".into(),
                })
                .into_any_element()],
            vec![button(
                "env-continue",
                if self.submitting {
                    "Applying…"
                } else {
                    "Continue"
                },
            )
            .loading(self.submitting)
            .when(!self.submitting, |b| b.icon_right("arrow-right"))
            .disabled(self.selected_id.is_none() || self.loading || authenticated)
            .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
            .into_any_element()],
        );

        page_frame("select-env")
            .title("Choose a network")
            .subtitle("Pick which SigVault Cloud deployment to connect to. Each network has its own accounts, wallets and sessions.")
            .action(
                button("env-refresh", "Refresh")
                    .outline()
                    .small()
                    .icon("refresh")
                    .loading(self.loading && !self.environments.is_empty())
                    .disabled(self.loading || self.submitting)
                    .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
            )
            .footer(footer)
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .when(authenticated, |f| {
                f.child(alert(Tone::Warning, "You're signed in. Sign out first to switch networks."))
            })
            .child(
                section("Networks")
                    .description("Test networks use test bitcoin with no real value.")
                    .child(self.render_list(cx)),
            )
            .child(
                div()
                    .max_w(FORM_WIDTH)
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(t.muted_foreground)
                    .child(icon("info").size(px(14.)).mt(px(1.)))
                    .child("To switch later, sign out and pick another network here. Wallets on this device aren't affected."),
            )
    }
}
