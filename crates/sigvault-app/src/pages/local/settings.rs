//! `pages/local/LocalSettings.tsx` — local-mode settings (QBL-231).
//!
//! Default network, per-network Electrum (electrs) URLs via
//! `cmd_local_get_settings` / `cmd_local_set_settings`, keychain lock and
//! an About section. v1 supports regtest / signet / testnet4; mainnet is
//! gated off across the stack and intentionally absent here.
//!
//! Live URL validation mirrors the Rust-side `validate_electrs_url`:
//! accepts `tcp://host:port`, `ssl://host:port`, or bare `host:port`;
//! empty values are tolerated ("not configured yet"). The backend
//! re-validates at save time and is authoritative.

use std::path::PathBuf;

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, SharedString, Subscription, Window};
use sigvault_core::local_wallet::commands::{cmd_local_get_settings, cmd_local_set_settings};
use sigvault_core::local_wallet::settings::LocalSettings;
use sigvault_core::State;

use super::widgets::{loading_card, network_color, row_icon};
use crate::e2e::{E2eClick, Probe};
use crate::keychain::{keychain, KeychainEvent};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::{navigate, toast};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, appearance_section, button, card, divider, footer_bar, page_frame, section, InputEvent,
    TextInput, Tone, FORM_WIDTH,
};

/// `constants/networks.ts` — `SUPPORTED_NETWORKS`.
struct NetworkOption {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
}

const SUPPORTED_NETWORKS: [NetworkOption; 3] = [
    NetworkOption {
        id: "regtest",
        label: "Regtest",
        hint: "Local dev / integration",
    },
    NetworkOption {
        id: "signet",
        label: "Signet",
        hint: "Public test network",
    },
    NetworkOption {
        id: "testnet4",
        label: "Testnet 4",
        hint: "Newer public testnet",
    },
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
    let last_colon =
        match last_colon {
            Some(i) if i > 0 && i != stripped.len() - 1 => i,
            _ => return Some(
                "URL must include host:port — use tcp://host:port, ssl://host:port, or host:port"
                    .into(),
            ),
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
    /// Edited copy.
    settings: Option<LocalSettings>,
    /// Last saved copy; the save bar shows while they differ.
    saved: Option<LocalSettings>,
    loading: bool,
    saving: bool,
    error: Option<SharedString>,
    inputs: Vec<Entity<TextInput>>,
    data_dir: Option<PathBuf>,
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
            subs.push(
                cx.subscribe(&input, move |this, input, ev: &InputEvent, cx| match ev {
                    InputEvent::Changed => {
                        let url = input.read(cx).text().to_string();
                        this.update_url(network, url, cx);
                    }
                    InputEvent::Submit => this.save(cx),
                }),
            );
            inputs.push(input);
        }
        subs.push(cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()));

        let task = backend(cx)
            .call(|app, st| async move { cmd_local_get_settings(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(s) => this.apply_loaded(s, cx),
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
            saved: None,
            loading: true,
            saving: false,
            error: None,
            inputs,
            data_dir: backend(cx).app.path().app_data_dir().ok(),
            _subs: subs,
        }
    }

    /// Push loaded / reverted settings into the inputs.
    fn apply_loaded(&mut self, s: LocalSettings, cx: &mut Context<Self>) {
        // Set `settings` first: the inputs' Changed events write back into it.
        self.settings = Some(s.clone());
        self.saved = Some(s.clone());
        for (n, input) in SUPPORTED_NETWORKS.iter().zip(self.inputs.clone()) {
            let url = s.electrs_urls.get(n.id).cloned().unwrap_or_default();
            let invalid = validate_url(&url).is_some();
            input.update(cx, |i, cx| {
                i.set_text(url, cx);
                i.set_invalid(invalid, cx);
            });
        }
        self.settings = Some(s);
    }

    fn update_url(&mut self, network: &str, url: String, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        let invalid = validate_url(&url).is_some();
        // An absent entry and an empty one mean the same thing; keep the
        // map shape stable so the dirty check doesn't flag no-op edits.
        let unchanged_absent = url.is_empty() && !settings.electrs_urls.contains_key(network);
        if !unchanged_absent {
            settings.electrs_urls.insert(network.to_string(), url);
        }
        if let Some(ix) = SUPPORTED_NETWORKS.iter().position(|n| n.id == network) {
            self.inputs[ix].update(cx, |i, cx| i.set_invalid(invalid, cx));
        }
        cx.notify();
    }

    fn set_default_network(&mut self, network: &str, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
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

    fn dirty(&self) -> bool {
        self.settings.is_some() && self.settings != self.saved
    }

    fn revert(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.saved.clone() {
            self.apply_loaded(s, cx);
            self.error = None;
            cx.notify();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let has_errors = self.field_errors().iter().any(Option::is_some);
        let Some(settings) = self.settings.clone() else {
            return;
        };
        if has_errors || self.saving || !self.dirty() {
            return;
        }
        self.saving = true;
        self.error = None;
        cx.notify();
        let to_save = settings.clone();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_set_settings(app, State::new(&st), to_save).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => {
                        this.saved = Some(settings);
                        toast(cx, "Settings saved");
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

    // ── rendering ──

    fn render_network(&self, settings: &LocalSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let cards = SUPPORTED_NETWORKS.iter().map(|n| {
            let selected = settings.default_network == n.id;
            let id = n.id;
            div()
                .id(SharedString::from(format!("default-net-{}", n.id)))
                .flex_1()
                .flex_basis(px(0.))
                .flex()
                .flex_row()
                .items_start()
                .gap(px(10.))
                .rounded_lg()
                .border_1()
                .px_3()
                .py(px(10.))
                .cursor_pointer()
                .when(selected, |d| {
                    d.border_color(alpha(t.primary, 0.7))
                        .bg(alpha(t.primary, 0.06))
                })
                .when(!selected, |d| {
                    d.border_color(t.border)
                        .bg(t.card)
                        .hover(move |s| s.border_color(alpha(t.primary, 0.4)))
                })
                .clickable(
                    Probe::new("radio", n.label).selected(selected),
                    cx.listener(move |this, _, _, cx| this.set_default_network(id, cx)),
                )
                .child(radio(selected))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.foreground)
                                .child(div().size(px(6.)).rounded_full().bg(network_color(n.id)))
                                .child(n.label),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(t.muted_foreground)
                                .child(n.hint),
                        ),
                )
        });
        section("Default network")
            .description("Pre-selected when you create or restore a wallet. Mainnet isn't available in this version.")
            .child(div().flex().flex_row().gap_2().children(cards))
    }

    fn render_servers(&self) -> impl IntoElement {
        let t = theme();
        let errors = self.field_errors();
        let mut c = card();
        for (i, ((n, input), err)) in SUPPORTED_NETWORKS
            .iter()
            .zip(self.inputs.iter())
            .zip(errors)
            .enumerate()
        {
            if i > 0 {
                c = c.child(divider());
            }
            c = c.child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_4()
                    .px_4()
                    .py_3()
                    .child(
                        div()
                            .w(px(110.))
                            .flex_none()
                            .pt(px(9.))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .text_color(t.foreground)
                            .child(div().size(px(6.)).rounded_full().bg(network_color(n.id)))
                            .child(n.label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(input.clone())
                            .children(err.map(|e| {
                                div().text_size(px(12.)).text_color(t.destructive).child(e)
                            })),
                    ),
            );
        }
        section("Electrum servers")
            .description("Used to sync wallets and broadcast transactions. Use ssl://host:port, tcp://host:port or host:port; leave blank for networks you don't use.")
            .child(c)
    }

    fn render_keychain(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let kc = keychain(cx).read(cx);
        let n = kc.keys().len();
        let hot = kc.hot_count();
        let has_pw = kc.has_password();
        let unlocked = kc.is_unlocked();
        let (icon_name, color, title, body): (&str, _, &str, String) = if !has_pw {
            (
                "key",
                t.muted_foreground,
                "No keychain password",
                "You'll set one the first time you store a private key in the Key library.".into(),
            )
        } else if unlocked {
            (
                "unlock",
                t.success,
                "Keychain unlocked",
                format!(
                    "{hot} private key{} available for signing until you lock it.",
                    if hot == 1 { "" } else { "s" }
                ),
            )
        } else {
            (
                "lock",
                t.muted_foreground,
                "Keychain locked",
                "Private keys are encrypted. You'll be asked for the password when one is needed."
                    .into(),
            )
        };
        section("Keychain")
            .description(format!(
                "{n} key{} in your Key library.",
                if n == 1 { "" } else { "s" }
            ))
            .action(
                button("open-keys", "Key library")
                    .ghost()
                    .small()
                    .icon_right("chevron-right")
                    .on_click(|_, _, cx| navigate(Page::LocalKeys, cx)),
            )
            .child(
                card().child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .px_4()
                        .min_h(px(64.))
                        .py_3()
                        .child(row_icon(icon_name, color, unlocked))
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
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(t.muted_foreground)
                                        .child(body),
                                ),
                        )
                        .when(unlocked, |d| {
                            d.child(
                                button("lock-keychain", "Lock now")
                                    .outline()
                                    .small()
                                    .icon("lock")
                                    .on_click(|_, _, cx| {
                                        keychain(cx).update(cx, |k, cx| k.lock(cx));
                                        toast(cx, "Keychain locked");
                                    }),
                            )
                        }),
                ),
            )
    }

    fn render_about(&self) -> impl IntoElement {
        let t = theme();
        let row = |k: &'static str, v: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .min_h(px(48.))
                .px_4()
                .py_2()
                .child(
                    div()
                        .w(px(110.))
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(k),
                )
                .child(div().flex_1().min_w_0().text_size(px(13.)).child(v))
        };
        let dir = self.data_dir.clone();
        let dir_el = match dir {
            Some(path) => {
                let reveal = path.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.foreground)
                            .child(path.display().to_string()),
                    )
                    .child(
                        button("reveal-data-dir", reveal_label())
                            .outline()
                            .small()
                            .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
                    )
                    .into_any_element()
            }
            None => div()
                .text_color(t.muted_foreground)
                .child("Unavailable")
                .into_any_element(),
        };
        section("About").child(
            card()
                .child(row(
                    "Version",
                    div()
                        .child(format!("SigVault {}", env!("CARGO_PKG_VERSION")))
                        .into_any_element(),
                ))
                .child(divider())
                .child(row(
                    "Mode",
                    div()
                        .child("Local — wallets live on this computer")
                        .into_any_element(),
                ))
                .child(divider())
                .child(row("Data folder", dir_el)),
        )
    }
}

fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Open folder"
    }
}

fn radio(selected: bool) -> impl IntoElement {
    let t = theme();
    div()
        .mt(px(2.))
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(if selected { t.primary } else { t.line_control })
        .when(selected, |d| {
            d.child(div().size(px(6.)).rounded_full().bg(t.primary))
        })
}

impl Render for LocalSettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_errors = self.field_errors().iter().any(Option::is_some);
        let dirty = self.dirty();
        let mut frame = page_frame("local-settings")
            .title("Settings")
            .subtitle("Preferences for wallets on this computer.");
        if dirty {
            frame = frame.footer(footer_bar(
                vec![div()
                    .text_size(px(12.))
                    .text_color(if has_errors {
                        theme().destructive
                    } else {
                        theme().muted_foreground
                    })
                    .child(if has_errors {
                        "Fix the highlighted server URLs to save"
                    } else {
                        "You have unsaved changes"
                    })
                    .into_any_element()],
                vec![
                    button("revert", "Revert")
                        .ghost()
                        .disabled(self.saving)
                        .on_click(cx.listener(|this, _, _, cx| this.revert(cx)))
                        .into_any_element(),
                    button("save", "Save changes")
                        .loading(self.saving)
                        .disabled(has_errors)
                        .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                        .into_any_element(),
                ],
            ));
        }
        let column = div().w_full().max_w(FORM_WIDTH).flex().flex_col().gap_8();
        let column = match self.settings.clone() {
            Some(s) => column
                .child(self.render_network(&s, cx))
                .child(self.render_servers()),
            None if self.loading => column.child(loading_card("Loading settings…")),
            None => column,
        };
        frame
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .child(
                column
                    .child(appearance_section(cx))
                    .child(self.render_keychain(cx))
                    .child(self.render_about()),
            )
    }
}
