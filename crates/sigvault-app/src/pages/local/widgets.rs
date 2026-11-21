//! Small pieces shared by the local-mode wallet pages (wallet list,
//! dashboard, receive, send, settings): tooltips, list rows, wallet
//! badges, dialog chrome and amount formatting.

use gpui::{
    div, prelude::*, px, AnyElement, AnyView, App, ClipboardItem, Context, Div, ElementId, Hsla,
    SharedString, Stateful, Window,
};

use crate::e2e::{E2eClick, Probe};
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
    hover_action(id, "copy", title, false).clickable(
        Probe::new("button", title),
        move |_, _, cx| {
            cx.stop_propagation();
            cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
            crate::shell::toast(cx, "Copied to clipboard");
        },
    )
}

/// Container for [`hover_action`]s that appear on hover of `group`, and
/// also while one of them has keyboard focus (so Tab never lands on an
/// invisible button).
pub fn hover_actions(
    group: impl Into<SharedString>,
    key: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let handle = window
        .use_keyed_state(key, cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    // `in_focus` styles the element *inside* a focused one; here a child
    // holds focus, which `contains_focused` answers (last frame's tree).
    let shown = handle.contains_focused(window, cx);
    div()
        .track_focus(&handle)
        .opacity(if shown { 1. } else { 0. })
        .group_hover(group, |s| s.opacity(1.))
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
        .text_size(px(12.))
        .line_height(px(16.))
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

/// A wallet-backend error as a plain sentence. Internal prefixes
/// (`persist: …`, `build_tx: …`, `seed store: …`) become a message that
/// names what failed; the raw text follows as "Details" when it adds
/// something.
pub fn plain_error(raw: &str) -> String {
    let raw = raw.trim();
    let lower = raw.to_lowercase();
    let plain = if lower.contains("decryption failed") || lower.contains("wrong passphrase") {
        // The message is the whole story here.
        return "That passphrase didn't unlock the wallet. Check it and try again.".into();
    } else if lower.contains("couldn't reach")
        || lower.contains("electrum")
        || lower.contains("esplora")
        || lower.contains("connection refused")
    {
        "Couldn't reach your server. Check the server address in Settings and your connection."
    } else if lower.starts_with("build_tx") || lower.contains("insufficient") {
        "Couldn't build the transaction."
    } else if lower.starts_with("persist") || lower.starts_with("initialize") {
        "Couldn't save the wallet to disk."
    } else if lower.starts_with("io") || lower.starts_with("serde") {
        "Couldn't read the wallet files on this computer."
    } else if lower.starts_with("wallet not found") {
        "This wallet isn't on this computer any more."
    } else if lower.contains("is not unlocked") {
        // Details would only add the wallet's internal id.
        return "This wallet is locked. Unlock it from the wallet's page, then try again.".into();
    } else if lower.contains("hardware wallet manager not initialized")
        || lower.contains("hardware manager unavailable")
    {
        return "Hardware wallet support isn't ready yet. Reopen the workspace from the top bar, then try again.".into();
    } else if lower.contains("no environment selected") {
        return "Choose a SigVault Cloud environment first: sign in from the SigVault Cloud workspace, then try again.".into();
    } else {
        return scrub_ids(raw);
    };
    format!("{plain} Details: {}", scrub_ids(raw))
}

/// Drop internal ids (UUIDs) in a backend message: they mean nothing to
/// the person reading it.
fn scrub_ids(raw: &str) -> String {
    let is_uuid = |w: &str| {
        let parts: Vec<&str> = w.split('-').collect();
        parts.len() == 5
            && [8, 4, 4, 4, 12]
                .iter()
                .zip(&parts)
                .all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit()))
    };
    raw.split(' ')
        .filter(|w| !is_uuid(w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::plain_error;

    #[test]
    fn plain_error_names_the_failure() {
        assert_eq!(
            plain_error("seed store: decryption failed (wrong passphrase or tampered file)"),
            "That passphrase didn't unlock the wallet. Check it and try again."
        );
        assert_eq!(
            plain_error("persist: disk full"),
            "Couldn't save the wallet to disk. Details: persist: disk full"
        );
        assert_eq!(plain_error("Something specific"), "Something specific");
        let locked = plain_error("wallet '206789aa-7bcc-4875-91f9-0cbcd17c8133' is not unlocked");
        assert!(!locked.contains("206789aa"), "{locked}");
        assert_eq!(
            plain_error("io: wallet 206789aa-7bcc-4875-91f9-0cbcd17c8133 missing"),
            "Couldn't read the wallet files on this computer. Details: io: wallet missing"
        );
        assert!(
            plain_error("Hardware wallet manager not initialized (no environment selected)")
                .starts_with("Hardware wallet support isn't ready")
        );
    }
}
