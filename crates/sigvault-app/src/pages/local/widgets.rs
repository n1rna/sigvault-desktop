//! Small pieces shared by the local-mode wallet pages (wallet list,
//! dashboard, receive, send, settings): tooltips, list rows, wallet
//! badges, dialog chrome and amount formatting.

use gpui::{
    div, prelude::*, px, AnyElement, AnyView, App, ClipboardItem, Context, Div, ElementId, Hsla,
    SharedString, Stateful, Window,
};

use crate::theme::{alpha, theme};
use crate::ui::{badge, card, icon, spinner_with, Tone};

/// Plain text tooltip, the equivalent of an HTML `title` attribute.
pub struct TextTooltip(SharedString);

impl Render for TextTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(t.border)
            .bg(t.popover)
            .shadow_md()
            .text_size(px(12.))
            .text_color(t.foreground)
            .child(self.0.clone())
    }
}

/// Builder for `.tooltip(...)`.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |_, cx| cx.new(|_| TextTooltip(text.clone())).into()
}

/// Card with a spinner and a muted line, shown while a page loads.
pub fn loading_card(text: impl Into<SharedString>) -> Div {
    let t = theme();
    card()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .h(px(56.))
        .px_4()
        .text_size(px(13.))
        .text_color(t.muted_foreground)
        .child(spinner_with(px(14.), t.muted_foreground))
        .child(text.into())
}

/// Bordered list container; put [`list_row`]s in it separated by
/// `ui::divider()`.
pub fn list_card() -> Div {
    card().overflow_hidden()
}

/// One ~56px row inside a [`list_card`].
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

/// Muted column header row for a [`list_card`].
pub fn list_header(cols: Vec<AnyElement>) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .h(px(32.))
        .px_4()
        .bg(alpha(t.muted, 0.35))
        .text_size(px(12.))
        .text_color(t.muted_foreground)
        .children(cols)
}

/// 32px rounded tile holding an icon, leading a list row.
pub fn row_icon(name: &str, color: Hsla, tinted: bool) -> Div {
    let t = theme();
    div()
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .map(|d| {
            if tinted {
                d.bg(alpha(color, 0.12))
            } else {
                d.border_1().border_color(t.border).bg(t.background)
            }
        })
        .child(icon(name).size(px(15.)).text_color(color))
}

/// 28px icon button revealed on row hover (lock, delete, copy).
pub fn hover_action(
    id: impl Into<ElementId>,
    icon_name: &'static str,
    title: impl Into<SharedString>,
    destructive: bool,
) -> Stateful<Div> {
    let t = theme();
    let id: ElementId = id.into();
    let hover_fg = if destructive {
        t.destructive
    } else {
        t.foreground
    };
    let hover_bg = if destructive {
        alpha(t.destructive, 0.12)
    } else {
        t.muted
    };
    let group: SharedString = format!("ha-{id}").into();
    div()
        .id(id)
        .group(group.clone())
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .tooltip(tooltip(title))
        .child(
            icon(icon_name)
                .size(px(14.))
                .text_color(t.muted_foreground)
                .group_hover(group, move |s| s.text_color(hover_fg)),
        )
}

/// Small copy-to-clipboard icon button with a toast.
pub fn copy_button(
    id: impl Into<ElementId>,
    value: impl Into<String>,
    title: &'static str,
) -> Stateful<Div> {
    let value: String = value.into();
    hover_action(id, "copy", title, false).on_click(move |_, _, cx| {
        cx.stop_propagation();
        cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
        crate::shell::toast(cx, "Copied to clipboard");
    })
}

/// Dialog title + description, used at the top of every local modal.
pub fn dialog_title(title: impl Into<SharedString>, description: Option<SharedString>) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(px(16.))
                .line_height(px(22.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.foreground)
                .child(title.into()),
        )
        .children(description.map(|d| {
            div()
                .text_size(px(13.))
                .line_height(px(19.))
                .text_color(t.muted_foreground)
                .child(d)
        }))
}

/// Right-aligned dialog buttons (Cancel, then the confirm action).
pub fn dialog_actions(children: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .justify_end()
        .gap_2()
        .pt_1()
        .children(children)
}

/// Human network name: `Regtest`, `Signet`, `Testnet 4`, `Mainnet`.
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

/// Human policy name for `WalletSummary::policy_type`.
pub fn policy_label(p: &str) -> String {
    match p {
        "singlesig" | "singlesig_hot" => "Single key".into(),
        "singlesig_hardware" => "Single key · hardware".into(),
        "descriptor" => "Descriptor".into(),
        "multisig" => "Multisig".into(),
        "timelocked" => "Timelocked".into(),
        "watch_only" => "Watch-only".into(),
        other => other.to_string(),
    }
}

/// Colour that identifies a network (matches the sidebar dots).
pub fn network_color(network: &str) -> Hsla {
    let t = theme();
    match network {
        "bitcoin" | "mainnet" => t.bitcoin,
        "testnet" | "testnet4" => t.success,
        "signet" => t.warning,
        _ => t.accent,
    }
}

/// Network pill with a coloured dot.
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
        .line_height(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(t.muted_foreground)
        .child(div().size(px(6.)).rounded_full().bg(network_color(network)))
        .child(network_label(network))
}

/// Policy pill.
pub fn policy_badge(policy: &str) -> Div {
    badge(policy_label(policy), Tone::Secondary)
}

/// 3px separator dot between metadata items.
pub fn sep_dot() -> Div {
    div()
        .size(px(3.))
        .flex_none()
        .rounded_full()
        .bg(alpha(theme().muted_foreground, 0.5))
}

/// Insert thousands separators (`toLocaleString()` for integers).
pub fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `formatBtc` without a unit suffix: `-0.00100000`.
pub fn btc_amount(sat: i64) -> String {
    let sign = if sat < 0 { "-" } else { "" };
    let abs = sat.unsigned_abs();
    format!("{sign}{}.{:08}", abs / 100_000_000, abs % 100_000_000)
}

/// `Nov 18, 2026` for a unix timestamp.
pub fn format_date(unix: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%b %-d, %Y").to_string(),
        _ => "—".into(),
    }
}

/// `just now`, `5m ago`, `3h ago`, `2d ago`, else a date.
pub fn format_relative(unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let diff = now.saturating_sub(unix);
    match diff {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", diff / 60),
        3600..=86399 => format!("{}h ago", diff / 3600),
        86400..=604799 => format!("{}d ago", diff / 86400),
        _ => format_date(unix as i64),
    }
}

/// Default directory for save dialogs: `~/Downloads`, else `~`.
pub fn default_save_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let downloads = home.join("Downloads");
    if downloads.is_dir() {
        downloads
    } else {
        home
    }
}
