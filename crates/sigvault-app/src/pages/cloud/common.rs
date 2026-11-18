//! Small helpers shared by the cloud-mode pages (kept local to `pages/cloud`
//! so other page ports are unaffected).

use std::time::Duration;

use gpui::{
    div, prelude::*, px, pulsating_between, Animation, AnimationExt, AnyElement, ElementId,
    Pixels, SharedString,
};
use sigvault_core::host::WebviewWindow;
use sigvault_core::AppHandle;

use crate::theme::{alpha, theme};

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

/// `animate-pulse` placeholder block filling the width.
pub fn skeleton(id: impl Into<ElementId>, height: Pixels) -> AnyElement {
    let t = theme();
    div()
        .w_full()
        .h(height)
        .bg(t.card)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(2000))
                .repeat()
                .with_easing(pulsating_between(0.5, 1.0)),
            |d, delta| d.opacity(delta),
        )
        .into_any_element()
}

/// Rounded pulsing pill used as a placeholder for meta chips.
pub fn skeleton_pill(id: impl Into<ElementId>, width: Pixels) -> AnyElement {
    let t = theme();
    div()
        .w(width)
        .h(px(24.))
        .rounded_full()
        .bg(alpha(t.muted, 0.4))
        .with_animation(
            id,
            Animation::new(Duration::from_millis(2000))
                .repeat()
                .with_easing(pulsating_between(0.5, 1.0)),
            |d, delta| d.opacity(delta),
        )
        .into_any_element()
}

/// Two stacked skeleton panels (`grid gap-px … h-48 / h-64`).
pub fn content_skeleton(prefix: &'static str) -> AnyElement {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .gap(px(1.))
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(t.border)
        .bg(t.border)
        .child(skeleton((prefix, 0usize), px(192.)))
        .child(skeleton((prefix, 1usize), px(256.)))
        .into_any_element()
}

/// The plain destructive box used by DeviceDiscovery / DeviceCreationSession
/// (`rounded-md border-destructive/30 bg-destructive/10 p-3 text-sm`).
pub fn plain_error(message: impl Into<SharedString>) -> AnyElement {
    let t = theme();
    div()
        .rounded_md()
        .border_1()
        .border_color(alpha(t.destructive, 0.3))
        .bg(alpha(t.destructive, 0.1))
        .p_3()
        .text_size(px(14.))
        .line_height(px(20.))
        .text_color(t.destructive)
        .child(message.into())
        .into_any_element()
}
