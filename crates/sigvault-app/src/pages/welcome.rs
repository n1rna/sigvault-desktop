//! `pages/Welcome.tsx` — first launch: choose Cloud (sign in) or Local.

use gpui::{div, prelude::*, px, Context, Hsla, SharedString, Window};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::cmd_set_app_mode;
use sigvault_core::State;

use crate::e2e::{E2e, E2eClick, Probe};
use crate::runtime::backend;
use crate::theme::{alpha, theme};
use crate::ui::{alert, badge, icon, logo, spinner_with, Tone};

pub struct WelcomePage {
    busy: Option<AppMode>,
    error: Option<SharedString>,
}

impl WelcomePage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            busy: None,
            error: None,
        }
    }

    fn choose(&mut self, mode: AppMode, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.error = None;
        self.busy = Some(mode);
        cx.notify();
        let task = backend(cx)
            .call(move |app, st| async move { cmd_set_app_mode(app, State::new(&st), mode).await });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                this.update(cx, |this, cx| {
                    this.error = Some(e.into());
                    this.busy = None;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn action_card(
        &self,
        mode: AppMode,
        accent: Hsla,
        tone: Tone,
        label: &'static str,
        title: &'static str,
        subtitle: &'static str,
        features: [&'static str; 3],
        icon_name: &'static str,
        cta: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let busy = self.busy == Some(mode);
        let disabled = self.busy.is_some() && !busy;
        div()
            .id(title)
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .p_5()
            .when(disabled, |d| {
                // Another workspace is opening: inert, text kept readable.
                d.bg(alpha(t.muted, 0.5))
                    .e2e(Probe::new("card", title).disabled(true))
            })
            .when(!disabled, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.border_color(alpha(accent, 0.5)))
                    .clickable(
                        Probe::new("card", title),
                        cx.listener(move |this, _, _, cx| this.choose(mode, cx)),
                    )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .size(px(40.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(alpha(accent, 0.1))
                            .child(icon(icon_name).size(px(20.)).text_color(accent)),
                    )
                    .child(badge(label, tone)),
            )
            .child(
                div()
                    .mt_4()
                    .text_size(px(17.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt_2()
                    .text_size(px(12.))
                    .line_height(px(19.))
                    .text_color(t.muted_foreground)
                    .child(subtitle),
            )
            .child(
                div()
                    .mt_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(features.into_iter().map(|f| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(icon("check").size(px(14.)).text_color(accent))
                            .child(f)
                    })),
            )
            .child(
                div()
                    .mt_6()
                    .pt_4()
                    .border_t_1()
                    .border_color(t.border)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(if busy { "Setting up…" } else { cta }),
                    )
                    .child(if busy {
                        spinner_with(px(16.), accent).into_any_element()
                    } else {
                        icon("arrow-right")
                            .size(px(16.))
                            .text_color(accent)
                            .into_any_element()
                    }),
            )
    }
}

impl Render for WelcomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let cloud = self.action_card(
            AppMode::Cloud,
            t.primary,
            Tone::Default,
            "Recommended",
            "SigVault Cloud",
            "Team multisig. SigVault coordinates signing between co-signers; keys on your own devices never leave them.",
            ["Multisig wallets shared with your team", "Signing sessions across hardware wallets", "Requires a SigVault account"],
            "cloud",
            "Continue to sign in",
            cx,
        );
        let local = self.action_card(
            AppMode::Local,
            t.accent,
            Tone::Secondary,
            "No account",
            "On this device",
            "A personal wallet that runs entirely on this computer. No account, no cloud, no third party.",
            ["Keys encrypted on this computer", "Sign and broadcast on-device", "Syncs with SigVault's servers or your own"],
            "hard-drive",
            "Continue without an account",
            cx,
        );

        div().id("welcome").size_full().overflow_y_scroll().child(
            div()
                .min_h_full()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .px_8()
                .py_12()
                .child(
                    div()
                        .w_full()
                        .max_w(px(720.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_center()
                                .gap(px(10.))
                                .child(logo(px(26.)))
                                .child(div().text_size(px(16.)).font_weight(gpui::FontWeight::SEMIBOLD).child("SigVault")),
                        )
                        .child(
                            div()
                                .mt_10()
                                .flex()
                                .flex_col()
                                .items_center()
                                .child(
                                    div()
                                        .text_size(px(26.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child("How do you want to use SigVault?"),
                                )
                                .child(
                                    div()
                                        .mt_3()
                                        .max_w(px(448.))
                                        .text_center()
                                        .text_size(px(13.))
                                        .line_height(px(21.))
                                        .text_color(t.muted_foreground)
                                        .child("Coordinate multisig with your team in SigVault Cloud, or keep a personal wallet on this device with no account. You can use both and switch any time from the workspace switcher in the top bar (⌘1 / ⌘2)."),
                                ),
                        )
                        .children(self.error.clone().map(|e| {
                            div().mt_8().flex().justify_center().child(alert(Tone::Destructive, e).max_w(px(448.)).w_full())
                        }))
                        .child(div().mt_10().flex().flex_row().gap_4().child(cloud).child(local))
                        .child(
                            div()
                                .mt_10()
                                .text_center()
                                .text_size(px(12.))
                                .line_height(px(18.))
                                .text_color(t.ink_2)
                                .child("Keys on your own devices never leave them. If a vault uses a SigVault co-signer key, SigVault holds that one key."),
                        ),
                ),
        )
    }
}
