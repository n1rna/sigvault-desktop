//! Small stateless building blocks: icons, spinner, badges, cards, alerts,
//! checkboxes, the SigVault mark, copyable fields, page scaffolding.

use std::time::Duration;

use gpui::{
    div, percentage, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ClickEvent,
    ClipboardItem, Div, ElementId, Hsla, Pixels, SharedString, Stateful, Svg, Transformation,
    Window,
};

use crate::e2e::{E2e, E2eClick, Probe};
use crate::theme::{alpha, theme, FONT_MONO, RADIUS_CARD, RADIUS_CHIP, RADIUS_CONTROL};

/// A stroked icon from `assets/icons/<name>.svg`. Color follows
/// `text_color`; default size 16px.
pub fn icon(name: &str) -> Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(16.))
        .flex_none()
        .text_color(theme().muted_foreground)
}

/// Rotating loader arc in the primary colour.
pub fn spinner(size: Pixels) -> gpui::AnimationElement<Svg> {
    spinner_with(size, theme().primary)
}

/// Rotating loader arc in a given colour.
pub fn spinner_with(size: Pixels, color: Hsla) -> gpui::AnimationElement<Svg> {
    icon("loader").size(size).text_color(color).with_animation(
        "spinner",
        Animation::new(Duration::from_millis(900)).repeat(),
        |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

/// The SigVault 3×3 mark (orange centre), drawn with quads.
pub fn logo(size: Pixels) -> Div {
    let t = theme();
    let cell = size * (30. / 102.);
    let gap = size * (6. / 102.);
    let row = |center: bool| {
        div()
            .flex()
            .flex_row()
            .gap(gap)
            .child(div().size(cell).bg(t.foreground))
            .child(
                div()
                    .size(cell)
                    .bg(if center { t.bitcoin } else { t.foreground }),
            )
            .child(div().size(cell).bg(t.foreground))
    };
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(gap)
        .size(size)
        .child(row(false))
        .child(row(true))
        .child(row(false))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Default,
    Secondary,
    Outline,
    Success,
    Warning,
    Destructive,
    Accent,
}

impl Tone {
    /// Fill / border / icon colour.
    pub fn color(self) -> Hsla {
        let t = theme();
        match self {
            Tone::Default => t.primary,
            Tone::Secondary | Tone::Outline => t.muted_foreground,
            Tone::Success => t.success,
            Tone::Warning => t.warning,
            Tone::Destructive => t.destructive,
            Tone::Accent => t.accent,
        }
    }

    /// Text colour for this tone (AA on page, surface, card and inset in
    /// both themes): the `*_ink` variant where fill and text differ.
    pub fn ink(self) -> Hsla {
        let t = theme();
        match self {
            Tone::Default | Tone::Accent => t.brand_ink,
            Tone::Secondary | Tone::Outline => t.ink_2,
            Tone::Success => t.success,
            Tone::Warning => t.warning_ink,
            Tone::Destructive => t.destructive,
        }
    }
}

/// Pill badge (`components/ui/badge.tsx`).
pub fn badge(label: impl Into<SharedString>, tone: Tone) -> Div {
    let t = theme();
    let color = tone.color();
    // Web `badge.tsx` "-light" variants: pill, tinted border + 10% fill,
    // Manrope 12px medium, text in the tone's ink.
    let mut el = div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap_1()
        .rounded_full()
        .border_1()
        .px(px(8.))
        .py(px(1.))
        .text_size(px(12.))
        .line_height(px(18.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(tone.ink());
    el = match tone {
        Tone::Secondary => el.border_color(t.line).bg(t.inset),
        Tone::Outline => el.border_color(t.line),
        _ => el
            .border_color(alpha(color, 0.35))
            .bg(alpha(color, if t.is_dark { 0.10 } else { 0.08 })),
    };
    let label: SharedString = label.into();
    el.e2e(Probe::new("badge", label.to_string())).child(label)
}

/// Small status chip (web `SwitcherPill`): `local`, `signed in`, `locked`.
/// Manrope 12px medium, ink_2, shown as given.
pub fn pill(label: impl Into<SharedString>) -> Div {
    let t = theme();
    let label: SharedString = label.into();
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(RADIUS_CHIP)
        .border_1()
        .border_color(t.line)
        .bg(t.inset)
        .px(px(6.))
        .py(px(1.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(t.ink_2)
        .child(label)
}

/// Small coloured status dot.
pub fn dot(color: Hsla) -> Div {
    div().size(px(6.)).flex_none().rounded_full().bg(color)
}

/// Card surface (web `card.tsx`): rounded-card, surface-2, hairline ring.
pub fn card() -> Div {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .rounded(RADIUS_CARD)
        .border_1()
        .border_color(t.line)
        .bg(t.surface_2)
        .text_color(t.ink)
}

/// Inline alert banner: destructive / warning / success / info.
pub fn alert(tone: Tone, message: impl Into<SharedString>) -> Div {
    let color = tone.color();
    let message: SharedString = message.into();
    let icon_name = match tone {
        Tone::Destructive => "alert-circle",
        Tone::Warning => "alert-triangle",
        Tone::Success => "circle-check",
        _ => "info",
    };
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap(px(10.))
        .rounded(RADIUS_CONTROL)
        .border_1()
        .border_color(alpha(color, 0.35))
        .bg(alpha(color, if theme().is_dark { 0.06 } else { 0.08 }))
        .px(px(14.))
        .py(px(12.))
        .text_size(px(13.))
        .line_height(px(19.))
        .text_color(tone.ink())
        .child(
            icon(icon_name)
                .size(px(14.))
                .mt(px(2.5))
                .text_color(tone.ink()),
        )
        .e2e(Probe::new("alert", message.to_string()))
        .child(div().flex_1().min_w_0().child(message))
}

/// Sentence-case first letter (labels are passed as plain words).
fn sentence(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Small label for groups, captions and stat names: Manrope 12px semibold,
/// ink_2, sentence case. (GPUI 0.2 has no letter-spacing, so labels stay
/// sentence case rather than cramped capitals.) Never put one directly
/// above a page or section heading.
pub fn eyebrow(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    div()
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme().ink_2)
        .child(sentence(&text))
}

/// Web `SectionHead`: label on the left, optional trailing on the right. No
/// rule.
pub fn section_head(text: impl Into<SharedString>, trailing: Option<AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .child(eyebrow(text))
        .children(trailing)
}

/// Web `DashboardStats`: a hairline grid of stat cells (Manrope captions,
/// tabular figure value).
/// Each entry is (label, value, hint).
pub fn stat_grid(stats: Vec<(SharedString, SharedString, SharedString)>) -> Div {
    let t = theme();
    let n = stats.len();
    div()
        .flex()
        .flex_row()
        .rounded(RADIUS_CARD)
        .border_1()
        .border_color(t.line)
        .overflow_hidden()
        .children(
            stats
                .into_iter()
                .enumerate()
                .map(move |(i, (label, value, hint))| {
                    div()
                        .flex_1()
                        .flex_basis(px(0.))
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_6()
                        .bg(t.page)
                        .when(i + 1 < n, |d| d.border_r_1().border_color(t.line))
                        .child(eyebrow(label))
                        .child(
                            div()
                                .text_size(px(32.))
                                .line_height(px(40.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.ink)
                                .truncate()
                                .child(value),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(18.))
                                .text_color(t.ink_2)
                                .child(hint),
                        )
                }),
        )
}

/// Page title (h1, web `PageHeader`).
pub fn title(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(34.))
        .line_height(px(40.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme().foreground)
        .child(text.into())
}

/// Section / card heading (h2/h3, web `tracking-card-title`).
pub fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(16.))
        .line_height(px(22.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme().foreground)
        .child(text.into())
}

/// Muted body text.
pub fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(13.))
        .line_height(px(19.))
        .text_color(theme().muted_foreground)
        .child(text.into())
}

/// Monospace text: real data only (addresses, txids, amounts, fees,
/// fingerprints, xpubs, descriptors, derivation paths, heights, seed words).
pub fn mono(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(12.))
        .line_height(px(17.))
        .text_color(theme().foreground)
        .child(text.into())
}

/// Form label.
pub fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme().foreground)
        .child(text.into())
}

/// Label + control + optional hint, stacked.
pub fn field(
    label_text: impl Into<SharedString>,
    control: impl IntoElement,
    hint: Option<SharedString>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(label(label_text))
        .child(control)
        .children(hint.map(|h| {
            div()
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(theme().ink_2)
                .child(h)
        }))
}

pub fn divider() -> Div {
    div().h(px(1.)).w_full().bg(theme().line)
}

/// Key/value row used in detail panels.
pub fn kv_row(key: impl Into<SharedString>, value: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .py(px(6.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme().muted_foreground)
                .flex_none()
                .child(key.into()),
        )
        .child(div().min_w_0().text_size(px(12.5)).child(value))
}

/// Checkbox with a label. Pass `on_toggle` to receive the new value.
pub fn checkbox(
    id: impl Into<ElementId>,
    checked: bool,
    text: impl Into<SharedString>,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let t = theme();
    let text: SharedString = text.into();
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_start()
        .gap(px(10.))
        .cursor_pointer()
        .clickable(
            Probe::new("checkbox", text.to_string()).selected(checked),
            move |_: &ClickEvent, w, cx| on_toggle(!checked, w, cx),
        )
        .child(
            div()
                .mt(px(2.))
                .size(px(16.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if checked { t.primary } else { t.line_control })
                .when(checked, |d| d.bg(t.primary))
                .when(checked, |d| {
                    d.child(icon("check").size(px(12.)).text_color(t.primary_foreground))
                }),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(13.))
                .line_height(px(19.))
                .text_color(t.foreground)
                .child(text),
        )
}

/// Radio-style selectable card used for choosing between options. Make it
/// interactive with `.clickable(Probe::new("choice", label).selected(..), f)`,
/// which also makes it a tab stop (Enter / Space choose it).
pub fn choice_card(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    let t = theme();
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap_1()
        .p_4()
        .rounded_lg()
        .border_1()
        .cursor_pointer()
        .border_color(if selected { t.brand } else { t.border })
        .bg(if selected {
            alpha(t.brand, if t.is_dark { 0.08 } else { 0.06 })
        } else {
            t.card
        })
        .hover(move |s| s.border_color(alpha(t.brand, 0.5)))
}

/// Segmented tabs. `items` are (key, label); `on_select` gets the key.
pub fn tabs(
    id: impl Into<ElementId>,
    items: &[(&'static str, SharedString)],
    active: &str,
    on_select: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
) -> Stateful<Div> {
    let t = theme();
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .flex()
        .flex_row()
        .gap_1()
        .p(px(3.))
        .rounded_md()
        .bg(t.muted)
        .children(items.iter().map(|(key, text)| {
            let key: &'static str = key;
            let selected = key == active;
            let handler = on_select.clone();
            div()
                .id(SharedString::from(format!("tab-{key}")))
                .flex_1()
                .flex()
                .justify_center()
                .px_3()
                .py(px(5.))
                .rounded(px(5.))
                .text_size(px(12.5))
                .cursor_pointer()
                // Selected: brand fill (≥3:1 against the track in both
                // themes) with white semibold text.
                .map(|d| {
                    if selected {
                        d.bg(t.brand_fill)
                            .text_color(t.brand_fill_contrast)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                    } else {
                        d.text_color(t.muted_foreground)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .hover(move |s| s.text_color(t.foreground).bg(t.hover_strong))
                    }
                })
                .clickable(
                    Probe::new("tab", text.to_string()).selected(selected),
                    move |_, w, cx| handler(key, w, cx),
                )
                .child(text.clone())
        }))
}

/// Read-only monospace value with a copy button.
pub fn copy_field(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Div {
    let t = theme();
    let value: SharedString = value.into();
    let v2 = value.clone();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(t.inset)
        .pl_3()
        .pr_1()
        .py_1()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(t.foreground)
                .child(value),
        )
        .child(
            div()
                .id(id)
                .flex_none()
                .p(px(6.))
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover_strong))
                .clickable(
                    Probe::new("button", "Copy to clipboard"),
                    move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(v2.to_string()));
                        crate::shell::toast(cx, "Copied to clipboard");
                    },
                )
                .child(icon("copy").size(px(14.))),
        )
}

/// Centered empty state with icon, title and description.
pub fn empty_state(
    icon_name: &str,
    heading_text: impl Into<SharedString>,
    body: impl Into<SharedString>,
) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .py_12()
        .px_6()
        .child(
            div()
                .size(px(44.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(t.muted)
                .child(icon(icon_name).size(px(20.))),
        )
        .child(heading(heading_text))
        .child(muted(body).max_w(px(380.)).text_center())
}

/// Full-height vertically scrolling page with a centered content column of
/// `max_width`. Pages render `page_scroll("id", 720.).child(...)`.
pub fn page_scroll(id: impl Into<ElementId>, max_width: f32) -> PageScroll {
    PageScroll {
        id: id.into(),
        max_width,
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct PageScroll {
    id: ElementId,
    max_width: f32,
    children: Vec<AnyElement>,
}

impl ParentElement for PageScroll {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for PageScroll {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().size_full().child(
            div().id(self.id).size_full().overflow_y_scroll().child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .px_8()
                    .py_8()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(self.max_width))
                            .flex()
                            .flex_col()
                            .gap_6()
                            .children(self.children),
                    ),
            ),
        )
    }
}

/// "← Back" text link used at the top of drill-down pages.
pub fn back_link(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let t = theme();
    let text: SharedString = text.into();
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .rounded(RADIUS_CHIP)
        .text_size(px(13.))
        .text_color(t.muted_foreground)
        .cursor_pointer()
        .hover(move |s| s.text_color(t.foreground))
        .clickable(Probe::new("link", text.to_string()), on_click)
        .child(icon("arrow-left").size(px(14.)))
        .child(text)
}

/// Horizontal step indicator: "1 Policy — 2 Keys — 3 Finish".
pub fn stepper(steps: &[&str], current: usize) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .children(steps.iter().enumerate().map(|(i, s)| {
            let done = i < current;
            let active = i == current;
            let color = if active || done {
                t.brand_ink
            } else {
                t.muted_foreground
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .when(i > 0, |d| d.child(div().w(px(24.)).h(px(1.)).bg(t.border)))
                .child(
                    div()
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(color)
                        .when(active, |d| d.bg(alpha(t.brand, 0.15)))
                        .text_size(px(12.))
                        .text_color(color)
                        .child(if done {
                            icon("check")
                                .size(px(12.))
                                .text_color(color)
                                .into_any_element()
                        } else {
                            format!("{}", i + 1).into_any_element()
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(if active {
                            t.foreground
                        } else {
                            t.muted_foreground
                        })
                        .child(s.to_string()),
                )
        }))
}

/// Format satoshis as BTC with 8 decimals: `1.23456789 BTC`.
pub fn format_btc(sat: i64) -> String {
    let neg = sat < 0;
    let abs = sat.unsigned_abs();
    format!(
        "{}{}.{:08} BTC",
        if neg { "-" } else { "" },
        abs / 100_000_000,
        abs % 100_000_000
    )
}

/// Format satoshis with thousands separators: `12,345 sat`.
pub fn format_sat(sat: i64) -> String {
    let s = sat.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{}{} sat", if sat < 0 { "-" } else { "" }, out)
}

/// Shorten long identifiers: `bc1qxy…0wlh`.
pub fn truncate_middle(s: &str, keep: usize) -> String {
    if s.chars().count() <= keep * 2 + 1 {
        return s.to_string();
    }
    let start: String = s.chars().take(keep).collect();
    let end: String = s
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{start}…{end}")
}

/// Local date/time for a unix timestamp (seconds).
pub fn format_timestamp(secs: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%b %-d, %Y %H:%M").to_string(),
        _ => "—".to_string(),
    }
}
