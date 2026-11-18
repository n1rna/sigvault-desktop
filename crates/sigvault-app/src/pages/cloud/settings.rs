//! `/cloud/settings` — SigVault Cloud workspace settings: account,
//! network, workspace switch and about.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{cmd_get_current_user, cmd_list_environments, cmd_logout, cmd_navigate};
use sigvault_core::env_config::EnvConfig;
use sigvault_core::State;

use super::common::{icon_tile, main_window, network_badge_unless_named};
use crate::runtime::backend;
use crate::state::store;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, button, card, divider, page_frame, section, Tone, FORM_WIDTH};

#[derive(Default, Clone)]
struct Account {
    name: Option<String>,
    email: Option<String>,
    username: Option<String>,
}

pub struct CloudSettingsPage {
    account: Option<Account>,
    env: Option<EnvConfig>,
    env_loaded: bool,
    signing_out: bool,
    changing_env: bool,
    error: Option<SharedString>,
}

impl CloudSettingsPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store(cx), |_, _, cx| cx.notify()).detach();
        let user = backend(cx).call(|_app, st| async move { cmd_get_current_user(State::new(&st)).await });
        let envs = backend(cx).call(|app, st| async move { cmd_list_environments(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let account = user.await.ok().map(|u| Account { name: u.name, email: u.email, username: u.username });
            let env = envs.await.ok().and_then(|r| {
                let id = r.selected_id?;
                r.environments.into_iter().find(|e| e.id == id)
            });
            this.update(cx, |this, cx| {
                this.account = account;
                this.env = env;
                this.env_loaded = true;
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self { account: None, env: None, env_loaded: false, signing_out: false, changing_env: false, error: None }
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        self.signing_out = true;
        self.error = None;
        cx.notify();
        let task = backend(cx).call(|app, st| async move { cmd_logout(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if !r.success => this.error = Some(r.message.into()),
                    Err(e) => this.error = Some(e.into()),
                    _ => {}
                }
                this.signing_out = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Only possible while signed out (the backend refuses to switch
    /// environments with a live token).
    fn change_env(&mut self, cx: &mut Context<Self>) {
        self.changing_env = true;
        cx.notify();
        let task = backend(cx).call(|app, st| async move {
            let window = main_window(&app)?;
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

    fn setting_row(title: &'static str, body: impl Into<SharedString>, action: Option<AnyElement>) -> gpui::Div {
        let t = theme();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .px_4()
            .py_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .text_color(t.muted_foreground)
                            .child(body.into()),
                    ),
            )
            .children(action.map(|a| div().flex_none().child(a)))
    }

    fn render_account(&self, authenticated: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let a = self.account.clone().unwrap_or_default();
        let primary = a
            .name
            .clone()
            .or_else(|| a.username.clone())
            .or_else(|| a.email.clone())
            .unwrap_or_else(|| if authenticated { "Signed in".into() } else { "Not signed in".into() });
        let secondary = a.email.clone().filter(|e| Some(e) != Some(&primary));
        let initial = primary.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        card()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .child(
                div()
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(alpha(t.primary, 0.15))
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.primary)
                    .child(initial),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(primary))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(secondary.unwrap_or_else(|| "SigVault account".into())),
                    ),
            )
            .when(authenticated, |c| {
                c.child(
                    button("sign-out", if self.signing_out { "Signing out…" } else { "Sign out" })
                        .outline()
                        .small()
                        .icon("log-out")
                        .loading(self.signing_out)
                        .on_click(cx.listener(|this, _, _, cx| this.sign_out(cx))),
                )
            })
            .into_any_element()
    }

    fn render_network(&self, authenticated: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
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
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div().flex().flex_row().items_center().gap_2().child(
                            div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(
                                self.env
                                    .as_ref()
                                    .map(|e| e.name.clone())
                                    .unwrap_or_else(|| if self.env_loaded { "No network selected".into() } else { "Loading…".into() }),
                            ),
                        )
                        .children(self.env.as_ref().and_then(|e| network_badge_unless_named(&e.name, &e.network))),
                    )
                    .children(self.env.as_ref().map(|e| {
                        div()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(e.api_base_url.trim_start_matches("https://").trim_start_matches("http://").to_string())
                    })),
            );
        card()
            .child(env_row)
            .child(divider())
            .child(Self::setting_row(
                "Change network",
                if authenticated {
                    "Accounts belong to one network. Sign out first, then choose another network on the sign-in screen."
                } else {
                    "Choose another SigVault Cloud deployment."
                },
                Some(
                    button("change-env", "Change network")
                        .outline()
                        .small()
                        .loading(self.changing_env)
                        .disabled(authenticated || self.changing_env)
                        .on_click(cx.listener(|this, _, _, cx| this.change_env(cx)))
                        .into_any_element(),
                ),
            ))
            .into_any_element()
    }
}

impl Render for CloudSettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let authenticated = store(cx).read(cx).authenticated;
        let workspace = card().child(Self::setting_row(
            "On this device",
            "Personal wallets stored and signed on this computer — no account needed. Your cloud session stays signed in.",
            Some(
                button("switch-local", "Switch")
                    .outline()
                    .small()
                    .icon("hard-drive")
                    .on_click(|_, _, cx| crate::shell::switch_space(AppMode::Local, cx))
                    .into_any_element(),
            ),
        ));
        let about = card().child(Self::setting_row(
            "Version",
            format!("SigVault {}", env!("CARGO_PKG_VERSION")),
            None,
        ));

        page_frame("cloud-settings")
            .title("Settings")
            .subtitle("Your SigVault Cloud account and network.")
            .child(
                div()
                    .max_w(FORM_WIDTH)
                    .flex()
                    .flex_col()
                    .gap_6()
                    .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                    .child(section("Account").child(self.render_account(authenticated, cx)))
                    .child(
                        section("Network")
                            .description("The SigVault Cloud deployment this app talks to.")
                            .child(self.render_network(authenticated, cx)),
                    )
                    .child(
                        section("Workspace")
                            .description("Switch any time from the sidebar (⌘1 / ⌘2).")
                            .child(workspace),
                    )
                    .child(section("About").child(about)),
            )
    }
}
