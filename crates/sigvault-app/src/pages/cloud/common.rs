//! Small helpers shared by the cloud pages. Patterns mirror the local pages
//! (`pages/local/widgets.rs`, `keys/sources.rs`) but are kept here so the
//! two workspaces don't import each other's internals.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, pulsating_between, Animation, AnimationExt, AnyElement, Div, ElementId,
    FontWeight, Hsla, Pixels, SharedString, Stateful,
};
use sigvault_core::host::WebviewWindow;
use sigvault_core::AppHandle;

use crate::theme::{alpha, theme};
use crate::ui::{card, divider, dot, icon};

pub const DEVICE_REGISTRATION: &str = "DEVICE_REGISTRATION";
pub const TRANSACTION_SIGNING: &str = "TRANSACTION_SIGNING";

/// `shortId` from the TSX pages: `abcdef…uvwxyz` for ids longer than 14.
pub fn short_id(id: &str) -> String {
    let n = id.chars().count();
    if n <= 14 {
        return id.to_string();
    }
    let start: String = id.chars().take(6).collect();
    let end: String = id.chars().skip(n - 6).collect();
    format!("{start}…{end}")
}

/// The `main` webview window, as required by Tauri-shaped commands.
pub fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main").ok_or_else(|| "no window".to_string())
}

/// Human title for a backend session type (`TRANSACTION_SIGNING` →
/// "Transaction signing").
pub fn session_title(session_type: &str) -> String {
    match session_type {
        DEVICE_REGISTRATION => "Device registration".into(),
        TRANSACTION_SIGNING => "Transaction signing".into(),
        other => {
            let friendly = other.replace('_', " ").to_lowercase();
            let mut chars = friendly.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => friendly,
            }
        }
    }
}

/// One-line explanation of what a session type asks of this signer.
pub fn session_blurb(session_type: &str) -> &'static str {
    match session_type {
        DEVICE_REGISTRATION => "Share a hardware wallet's public key so it can join a team wallet.",
        TRANSACTION_SIGNING => "Review a team transaction and approve it on your hardware wallet.",
        _ => "A signing session coordinated by SigVault Cloud.",
    }
}

pub fn session_icon(session_type: &str) -> &'static str {
    match session_type {
        DEVICE_REGISTRATION => "hw-device",
        TRANSACTION_SIGNING => "file-key",
        _ => "users",
    }
}

/// Relative time for an RFC 3339 timestamp: "just now", "5m ago", "3d ago".
pub fn format_relative(iso: Option<&str>) -> String {
    let Some(then) = iso.and_then(parse_time) else {
        return "—".into();
    };
    let now = chrono::Utc::now();
    let diff = (now.timestamp_millis() - then.timestamp_millis()).max(0);
    let mins = diff / 60_000;
    if mins < 1 {
        return "just now".into();
    }
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hrs = mins / 60;
    if hrs < 24 {
        return format!("{hrs}h ago");
    }
    let days = hrs / 24;
    if days < 7 {
        return format!("{days}d ago");
    }
    then.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()
}

pub fn parse_time(iso: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(iso).ok()
}

/// Rounded tile holding an icon, leading a list row (same as the key
/// library's tiles).
pub fn icon_tile(name: &'static str, size: f32, color: Option<Hsla>) -> Div {
    let t = theme();
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(alpha(t.muted, 0.8))
        .border_1()
        .border_color(alpha(t.border, 0.7))
        .child(icon(name).size(px(size * 0.5)).text_color(color.unwrap_or(t.foreground)))
}

pub fn network_label(n: &str) -> String {
    match n {
        "bitcoin" | "mainnet" => "Mainnet".into(),
        "testnet" => "Testnet".into(),
        "testnet4" => "Testnet 4".into(),
        "signet" => "Signet".into(),
        "regtest" => "Regtest".into(),
        other => other.to_string(),
    }
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

/// Network pill with a coloured dot (matches the local pages).
pub fn network_badge(network: &str) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .border_1()
        .border_color(t.border)
        .px(px(8.))
        .py(px(2.))
        .text_size(px(11.))
        .line_height(px(14.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(t.muted_foreground)
        .child(div().size(px(6.)).rounded_full().bg(network_color(network)))
        .child(network_label(network))
}

/// Network badge, unless `name` already says the network ("Testnet").
pub fn network_badge_unless_named(name: &str, network: &str) -> Option<Div> {
    (!name.eq_ignore_ascii_case(&network_label(network))).then(|| network_badge(network))
}

/// Badge with a leading status dot ("Connected", "Live").
pub fn status_badge(label: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .px(px(9.))
        .py(px(3.))
        .text_size(px(11.))
        .line_height(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(color)
        .bg(alpha(color, 0.14))
        .child(dot(color))
        .child(label.into())
}

/// One ~56px row inside a list `card()`; separate rows with `divider()`.
pub fn list_row(id: impl Into<ElementId>) -> Stateful<Div> {
    let t = theme();
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .min_h(px(56.))
        .px_4()
        .py_2()
        .hover(move |s| s.bg(alpha(t.muted, 0.5)))
}

/// Label / value row used in review cards (same as the local send review).
pub fn review_row(label: impl Into<SharedString>, value: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_4()
        .px_4()
        .py(px(10.))
        .child(
            div()
                .w(px(120.))
                .flex_none()
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().flex_1().min_w_0().text_size(px(13.)).line_height(px(18.)).child(value))
}

/// 3px separator dot between metadata items.
pub fn sep_dot() -> Div {
    div().size(px(3.)).flex_none().rounded_full().bg(alpha(theme().muted_foreground, 0.5))
}

/// Pulsing placeholder block.
pub fn skeleton(id: impl Into<ElementId>, width: Pixels, height: Pixels) -> AnyElement {
    let t = theme();
    div()
        .w(width)
        .h(height)
        .rounded(px(4.))
        .bg(alpha(t.muted, 0.9))
        .with_animation(
            id,
            Animation::new(Duration::from_millis(1600))
                .repeat()
                .with_easing(pulsating_between(0.4, 1.0)),
            |d, delta| d.opacity(delta),
        )
        .into_any_element()
}

/// A list card of `n` placeholder rows shaped like real rows (icon tile,
/// two text lines, trailing pill).
pub fn skeleton_list(prefix: &'static str, n: usize) -> Div {
    let mut list = card().overflow_hidden();
    for i in 0..n {
        if i > 0 {
            list = list.child(divider());
        }
        list = list.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .h(px(60.))
                .px_4()
                .child(skeleton((prefix, i * 4), px(32.), px(32.)))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(skeleton((prefix, i * 4 + 1), px(180.), px(12.)))
                        .child(skeleton((prefix, i * 4 + 2), px(120.), px(10.))),
                )
                .child(skeleton((prefix, i * 4 + 3), px(64.), px(20.))),
        );
    }
    list
}

/// Segmented filter with owned keys (like `ui::tabs`, but for keys known
/// only at runtime). `items` are (key, label).
pub fn segmented<F>(id: &'static str, items: Vec<(String, String)>, active: &str, on_select: F) -> Stateful<Div>
where
    F: Fn(String, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
{
    let t = theme();
    div()
        .id(id)
        .flex()
        .flex_row()
        .gap_1()
        .p(px(3.))
        .rounded_md()
        .bg(t.muted)
        .children(items.into_iter().enumerate().map(|(ix, (key, text))| {
            let selected = key == active;
            let handler = on_select.clone();
            div()
                .id((id, ix))
                .flex()
                .justify_center()
                .px_3()
                .py(px(5.))
                .rounded(px(5.))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .text_color(if selected { t.foreground } else { t.muted_foreground })
                .when(selected, |d| d.bg(t.card).border_1().border_color(t.border))
                .when(!selected, |d| d.hover(move |s| s.text_color(t.foreground)))
                .on_click(move |_, w, cx| handler(key.clone(), w, cx))
                .child(text)
        }))
}

/// Centered empty / status block inside a card: icon tile, title, body.
pub fn card_message(icon_name: &'static str, color: Option<Hsla>, title: impl Into<SharedString>, body: impl Into<SharedString>) -> Div {
    let t = theme();
    card()
        .items_center()
        .gap_2()
        .py_12()
        .px_6()
        .child(icon_tile(icon_name, 44., color))
        .child(
            div()
                .mt_2()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(
            div()
                .max_w(px(440.))
                .text_center()
                .text_size(px(13.))
                .line_height(px(19.))
                .text_color(t.muted_foreground)
                .child(body.into()),
        )
}

/// `result.message || fallback`.
pub fn non_empty(msg: String, fallback: &str) -> String {
    if msg.is_empty() {
        fallback.to_string()
    } else {
        msg
    }
}

/// Debug builds only: `SIGVAULT_DEV_CLOUD_STEP` drives a session workflow to
/// a given state for screenshots (see `dev_fixtures.rs`). Always `None` in
/// release builds.
pub fn dev_step() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var("SIGVAULT_DEV_CLOUD_STEP").ok()
    } else {
        None
    }
}
