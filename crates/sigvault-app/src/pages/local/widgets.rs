//! Small pieces shared by the local-mode pages: the sticky header bar,
//! icon back button, `title=` tooltips and status boxes.

use gpui::{
    div, prelude::*, px, AnyView, App, ClickEvent, Context, Div, ElementId, SharedString, Stateful,
    Window,
};

use crate::theme::{alpha, theme};
use crate::ui::icon;

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
            .text_size(px(11.))
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

/// Top header bar used by the wallet dashboard / receive / settings pages.
pub fn header_bar() -> Div {
    let t = theme();
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .border_b_1()
        .border_color(t.border)
        .bg(alpha(t.card, 0.6))
        .px_6()
        .py_3()
}

/// 32px ghost chevron button that navigates back.
pub fn back_button(
    id: impl Into<ElementId>,
    title: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let t = theme();
    div()
        .id(id)
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_color(t.muted_foreground)
        .hover(move |s| s.bg(t.muted).text_color(t.foreground))
        .tooltip(tooltip(title))
        .on_click(on_click)
        .child(icon("chevron-left").size(px(16.)).text_color(t.muted_foreground))
}

/// Bordered muted status box ("Loading wallets…").
pub fn status_box(text: impl Into<SharedString>) -> Div {
    let t = theme();
    div()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(t.card)
        .px_4()
        .py_3()
        .text_size(px(13.))
        .text_color(t.muted_foreground)
        .child(text.into())
}

/// 4px separator dot between metadata items.
pub fn sep_dot() -> Div {
    div()
        .size(px(4.))
        .flex_none()
        .rounded_full()
        .bg(alpha(theme().muted_foreground, 0.4))
}

/// Insert thousands separators (`toLocaleString()` for integers).
pub fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
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
