//! Small stateless building blocks: icons, spinner, badges, cards, alerts,
//! checkboxes, the SigVault mark, copyable fields, page scaffolding.

use std::time::Duration;

use gpui::{
    div, percentage, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ClickEvent,
    ClipboardItem, Div, ElementId, Hsla, Pixels, SharedString, Stateful, Svg, Transformation,
    Window,
};

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
}

/// Pill badge (`components/ui/badge.tsx`).
pub fn badge(label: impl Into<SharedString>, tone: Tone) -> Div {
    let t = theme();
    let color = tone.color();
    // Web `badge.tsx` "-light" variants: pill, tinted border + 10% fill.
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
        .text_color(color);
    el = match tone {
        Tone::Secondary => el.border_color(t.line).bg(t.inset).text_color(t.ink_2),
        Tone::Outline => el.border_color(t.line).text_color(t.ink_2),
        _ => el.border_color(alpha(color, 0.25)).bg(alpha(color, 0.10)),
    };
    el.child(label.into())
}

/// Neon-style mono chip (web `SwitcherPill`): `PERSONAL`, `REGTEST`, roles.
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
        .font_family(FONT_MONO)
        .text_size(px(10.))
        .line_height(px(15.))
        .text_color(t.ink_2)
        .child(label.to_uppercase())
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
        .border_color(alpha(color, 0.3))
        .bg(alpha(color, 0.06))
        .px(px(14.))
        .py(px(12.))
        .text_size(px(12.))
        .line_height(px(17.))
        .text_color(color)
        .child(icon(icon_name).size(px(14.)).mt(px(1.5)).text_color(color))
        .child(div().flex_1().min_w_0().child(message.into()))
}

/// Mono uppercase eyebrow (web `SectionHead` label / stat labels).
pub fn eyebrow(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    div()
        .font_family(FONT_MONO)
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(theme().ink_2)
        .child(text.to_uppercase())
}

/// Web `SectionHead`: mono uppercase label, a hairline rule, optional trailing.
pub fn section_head(text: impl Into<SharedString>, trailing: Option<AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .child(eyebrow(text))
        .child(div().h(px(1.)).flex_1().bg(theme().line))
        .children(trailing)
}

/// Web `DashboardStats`: a hairline grid of mono stat cells.
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
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(10.))
                                .text_color(t.ink_2)
                                .child(label.to_uppercase()),
                        )
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(34.))
                                .line_height(px(42.))
                                .text_color(t.ink)
                                .truncate()
                                .child(value),
                        )
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(10.))
                                .text_color(alpha(t.ink_2, 0.7))
                                .child(hint.to_uppercase()),
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

/// Monospace text (addresses, txids, fingerprints).
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
                .text_size(px(11.5))
                .line_height(px(16.))
                .text_color(alpha(theme().muted_foreground, 0.85))
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
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_start()
        .gap(px(10.))
        .cursor_pointer()
        .on_click(move |_: &ClickEvent, w, cx| on_toggle(!checked, w, cx))
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
                .border_color(if checked { t.primary } else { t.border })
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
                .child(text.into()),
        )
}

/// Radio-style selectable card used for choosing between options.
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
        .border_color(if selected {
            alpha(t.primary, 0.6)
        } else {
            t.border
        })
        .bg(if selected {
            alpha(t.primary, 0.06)
        } else {
            t.card
        })
        .hover(move |s| s.border_color(alpha(t.primary, 0.4)))
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
                .font_weight(gpui::FontWeight::MEDIUM)
                .cursor_pointer()
                .text_color(if selected {
                    t.foreground
                } else {
                    t.muted_foreground
                })
                .when(selected, |d| d.bg(t.card).border_1().border_color(t.border))
                .on_click(move |_, w, cx| handler(key, w, cx))
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
        .bg(alpha(t.background, 0.6))
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
                .hover(|s| s.bg(t.muted))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(v2.to_string()));
                    crate::shell::toast(cx, "Copied to clipboard");
                })
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
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_size(px(12.5))
        .text_color(t.muted_foreground)
        .cursor_pointer()
        .hover(move |s| s.text_color(t.foreground))
        .on_click(on_click)
        .child(icon("arrow-left").size(px(14.)))
        .child(text.into())
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
                t.primary
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
                        .when(active, |d| d.bg(alpha(t.primary, 0.15)))
                        .text_size(px(11.))
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
