//! `pages/local/LocalSettings.tsx` — local-mode settings (QBL-231).
//!
//! Per-network electrs URL inputs and default-network picker for
//! `cmd_local_get_settings` / `cmd_local_set_settings`. v1 supports
//! regtest / signet / testnet4; mainnet is gated off across the stack and
//! intentionally absent here.
//!
//! Live URL validation mirrors the Rust-side `validate_electrs_url`:
//! accepts `tcp://host:port`, `ssl://host:port`, or bare `host:port`;
//! empty values are tolerated ("not configured yet"). The backend
//! re-validates at save time and is authoritative.

use std::time::Duration;

use gpui::{div, prelude::*, px, Context, Div, Entity, SharedString, Subscription, Window};
use sigvault_core::local_wallet::commands::{cmd_local_get_settings, cmd_local_set_settings};
use sigvault_core::local_wallet::settings::LocalSettings;
use sigvault_core::State;

use super::widgets::{back_button, header_bar, status_box};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, InputEvent, TextInput, Tone};

/// `constants/networks.ts` — `SUPPORTED_NETWORKS`.
struct NetworkOption {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
}

const SUPPORTED_NETWORKS: [NetworkOption; 3] = [
    NetworkOption { id: "regtest", label: "Regtest", hint: "Local dev / integration" },
    NetworkOption { id: "signet", label: "Signet", hint: "Public test network" },
    NetworkOption { id: "testnet4", label: "Testnet 4", hint: "Newer public testnet" },
];

/// Mirror of `validate_electrs_url` in `local_wallet/settings.rs`. Empty
/// strings are valid here ("not configured yet").
fn validate_url(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let stripped = url
        .strip_prefix("tcp://")
        .or_else(|| url.strip_prefix("ssl://"))
        .unwrap_or(url);
    let last_colon = stripped.rfind(':');
    let last_colon = match last_colon {
        Some(i) if i > 0 && i != stripped.len() - 1 => i,
        _ => {
            return Some(
                "URL must include host:port — use tcp://host:port, ssl://host:port, or host:port"
                    .into(),
            )
        }
    };
    let host = &stripped[..last_colon];
    let port_str = &stripped[last_colon + 1..];
    if host.is_empty() {
        return Some("URL is missing the host".into());
    }
    // JS `Number(portStr)` + `Number.isInteger` + range check.
    let ok = port_str
        .trim()
        .parse::<f64>()
        .map(|p| p.fract() == 0. && (1. ..=65535.).contains(&p))
        .unwrap_or(false);
    if !ok {
        return Some(format!(
            "Port must be a number between 1 and 65535 (got \"{port_str}\")"
        ));
    }
    None
}

pub struct LocalSettingsPage {
    settings: Option<LocalSettings>,
    loading: bool,
    saving: bool,
    error: Option<SharedString>,
    saved_flash: bool,
    saved_epoch: usize,
    inputs: Vec<Entity<TextInput>>,
    _subs: Vec<Subscription>,
}

impl LocalSettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut inputs = Vec::new();
        let mut subs = Vec::new();
        for n in SUPPORTED_NETWORKS.iter() {
            let placeholder = if n.id == "regtest" {
                "ssl://ers.regtest.sigvault.org:443"
            } else {
                "ssl://your.electrs.host:50002"
            };
            let input = cx.new(|cx| TextInput::new(window, cx).mono().placeholder(placeholder));
            let network = n.id;
            subs.push(cx.subscribe(&input, move |this, input, ev: &InputEvent, cx| {
                if let InputEvent::Changed = ev {
                    let url = input.read(cx).text().to_string();
                    this.update_url(network, url, cx);
                }
            }));
            inputs.push(input);
        }

        let task = backend(cx).call(|app, st| async move {
            cmd_local_get_settings(app, State::new(&st)).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(s) => {
                        for (n, input) in SUPPORTED_NETWORKS.iter().zip(this.inputs.clone()) {
                            let url = s.electrs_urls.get(n.id).cloned().unwrap_or_default();
                            let invalid = validate_url(&url).is_some();
                            input.update(cx, |i, cx| {
                                i.set_text(url, cx);
                                i.set_invalid(invalid, cx);
                            });
                        }
                        this.settings = Some(s);
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();

        Self {
            settings: None,
            loading: true,
            saving: false,
            error: None,
            saved_flash: false,
            saved_epoch: 0,
            inputs,
            _subs: subs,
        }
    }

    fn update_url(&mut self, network: &str, url: String, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else { return };
        let invalid = validate_url(&url).is_some();
        settings.electrs_urls.insert(network.to_string(), url);
        if let Some(ix) = SUPPORTED_NETWORKS.iter().position(|n| n.id == network) {
            self.inputs[ix].update(cx, |i, cx| i.set_invalid(invalid, cx));
        }
        cx.notify();
    }

    fn set_default_network(&mut self, network: &str, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else { return };
        settings.default_network = network.to_string();
        cx.notify();
    }

    fn field_errors(&self) -> Vec<Option<String>> {
        let Some(s) = self.settings.as_ref() else {
            return vec![None; SUPPORTED_NETWORKS.len()];
        };
        SUPPORTED_NETWORKS
            .iter()
            .map(|n| validate_url(s.electrs_urls.get(n.id).map(String::as_str).unwrap_or("")))
            .collect()
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let has_errors = self.field_errors().iter().any(Option::is_some);
        let Some(settings) = self.settings.clone() else { return };
        if has_errors || self.saving {
            return;
        }
        self.saving = true;
        self.error = None;
        cx.notify();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_set_settings(app, State::new(&st), settings).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => {
                        this.saved_flash = true;
                        this.saved_epoch += 1;
                        let epoch = this.saved_epoch;
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(Duration::from_millis(1800)).await;
                            this.update(cx, |this, cx| {
                                if this.saved_epoch == epoch {
                                    this.saved_flash = false;
                                    cx.notify();
                                }
                            })
                            .ok();
                        })
                        .detach();
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.saving = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_body(&self, settings: &LocalSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let errors = self.field_errors();
        let has_errors = errors.iter().any(Option::is_some);

        let network_cards = SUPPORTED_NETWORKS.iter().map(|n| {
            let selected = settings.default_network == n.id;
            let id = n.id;
            div()
                .id(SharedString::from(format!("default-net-{}", n.id)))
                .flex_1()
                .flex_basis(px(0.))
                .flex()
                .flex_col()
                .items_start()
                .rounded_md()
                .border_1()
                .px_3()
                .py(px(10.))
                .cursor_pointer()
                .when(selected, |d| d.border_color(t.primary).bg(alpha(t.primary, 0.06)))
                .when(!selected, |d| {
                    d.border_color(t.border)
                        .bg(t.card)
                        .hover(move |s| s.border_color(alpha(t.primary, 0.5)))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.set_default_network(id, cx)))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.foreground)
                        .child(n.label),
                )
                .child(
                    div()
                        .mt(px(2.))
                        .text_size(px(11.))
                        .text_color(t.muted_foreground)
                        .child(n.hint),
                )
        });

        let url_fields = SUPPORTED_NETWORKS
            .iter()
            .zip(self.inputs.iter())
            .zip(errors)
            .map(|((n, input), err)| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_end()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.muted_foreground)
                                    .child(n.label),
                            )
                            .child(badge(n.id, Tone::Outline)),
                    )
                    .child(div().mt_2().child(input.clone()))
                    .children(err.map(|e| {
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .text_color(t.destructive)
                            .child(e)
                    }))
            });

        div()
            .flex()
            .flex_col()
            .gap_8()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(section_title("Default network"))
                    .child(section_desc().child(
                        "Pre-selected on the wallet creation wizard. Mainnet is not available in v1.",
                    ))
                    .child(div().mt_4().flex().flex_row().gap_2().children(network_cards)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(section_title("Electrs endpoints"))
                    .child(rich_paragraph(&[
                        ("URLs the wallet connects to for sync and broadcast. Accepts", false),
                        ("ssl://host:port", true),
                        (",", false),
                        ("tcp://host:port", true),
                        (", or bare", false),
                        ("host:port", true),
                        (". Leave blank if you haven't set up that network yet.", false),
                    ]))
                    .child(div().mt_4().flex().flex_col().gap_4().children(url_fields)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        button("save", if self.saving { "Saving…" } else { "Save settings" })
                            .large()
                            .disabled(self.saving || has_errors)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .when(self.saved_flash, |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.success)
                                .child("✓ Saved"),
                        )
                    })
                    .when(has_errors && !self.saving, |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(t.muted_foreground)
                                .child("Fix the highlighted URLs to enable save"),
                        )
                    }),
            )
    }
}

fn section_title(text: &'static str) -> Div {
    div()
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme().foreground)
        .child(text)
}

fn section_desc() -> Div {
    div()
        .mt_1()
        .text_size(px(12.))
        .line_height(px(19.))
        .text_color(theme().muted_foreground)
}

/// Paragraph mixing body text with inline monospace spans. Each word is
/// its own flex item so the line still wraps at word boundaries.
fn rich_paragraph(parts: &[(&'static str, bool)]) -> Div {
    let t = theme();
    let mut el = section_desc().flex().flex_row().flex_wrap().items_baseline();
    let space = px(3.5);
    for (text, is_mono) in parts.iter() {
        if *is_mono {
            el = el.child(
                div()
                    .mx_1()
                    .font_family(FONT_MONO)
                    .text_size(px(11.))
                    .text_color(t.foreground)
                    .child(*text),
            );
        } else {
            // Words are separated by a space; text adjacent to a mono span
            // relies on the span's own margin (like `mx-1` in the TSX).
            let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
            let n = words.len();
            for (j, w) in words.into_iter().enumerate() {
                el = el.child(div().when(j + 1 < n, |d| d.mr(space)).child(w));
            }
        }
    }
    el
}

impl Render for LocalSettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let header = header_bar()
            .justify_start()
            .child(back_button("back", "Back to wallets", |_, _, cx| {
                navigate(Page::LocalWallets, cx)
            }))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.foreground)
                    .child("Settings"),
            );

        let body = self.settings.clone().map(|s| self.render_body(&s, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div().w_full().flex().flex_col().items_center().px_6().py(px(28.)).child(
                            div()
                                .w_full()
                                .max_w(px(672.))
                                .flex()
                                .flex_col()
                                .gap_8()
                                .when(self.loading, |d| d.child(status_box("Loading settings…")))
                                .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                                .children(body),
                        ),
                    ),
            )
    }
}
