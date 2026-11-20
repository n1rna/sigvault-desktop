//! `setup/ui.tsx` — small presentational pieces shared by the create and
//! recover wizards.
// A shared kit: some helpers are only used by the key-source flows.
#![allow(dead_code)]

use gpui::{
    div, prelude::*, px, AnyElement, App, Div, ElementId, FontWeight, SharedString, Stateful,
    Window,
};

use crate::e2e::{E2e, E2eClick, Probe};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{button, Button};

/// Mirrors `constants/networks.ts` (mainnet intentionally absent).
pub const SUPPORTED_NETWORKS: [(&str, &str, &str); 3] = [
    ("regtest", "Regtest", "Local testing"),
    ("signet", "Signet", "Public test network"),
    ("testnet4", "Testnet 4", "Public testnet"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AlertTone {
    Error,
    Warning,
    Info,
    Success,
}

/// `<Alert tone title>children</Alert>`.
pub fn setup_alert(tone: AlertTone, title: Option<SharedString>, body: Option<AnyElement>) -> Div {
    let t = theme();
    let (border, bg, fg) = match tone {
        AlertTone::Error => (
            alpha(t.destructive, 0.3),
            alpha(t.destructive, 0.06),
            t.destructive,
        ),
        AlertTone::Warning => (alpha(t.warning, 0.4), alpha(t.warning, 0.07), t.foreground),
        AlertTone::Info => (t.border, alpha(t.card, 0.4), t.muted_foreground),
        AlertTone::Success => (alpha(t.success, 0.4), alpha(t.success, 0.07), t.foreground),
    };
    let has_title = title.is_some();
    div()
        .flex()
        .flex_col()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(bg)
        .px(px(14.))
        .py(px(12.))
        .text_size(px(12.))
        .line_height(px(19.))
        .text_color(fg)
        .children(title.map(|ti| {
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.foreground)
                .child(ti)
        }))
        .children(body.map(|b| {
            div()
                .when(has_title, |d| d.mt_1())
                .when(tone != AlertTone::Error, |d| {
                    d.text_color(t.muted_foreground)
                })
                .child(b)
        }))
}

/// Body-only alert with plain text.
pub fn alert_text(tone: AlertTone, text: impl Into<SharedString>) -> Div {
    setup_alert(tone, None, Some(text.into().into_any_element()))
}

/// `<FieldLabel hint>label</FieldLabel>`.
pub fn field_label(text: impl Into<SharedString>, hint: Option<SharedString>) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.muted_foreground)
                .child(text.into()),
        )
        .children(hint.map(|h| {
            div()
                .mt_1()
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(t.muted_foreground)
                .child(h)
        }))
}

pub struct SegOption {
    pub value: SharedString,
    pub label: SharedString,
    pub disabled: bool,
}

pub fn seg(value: impl Into<SharedString>, label: impl Into<SharedString>) -> SegOption {
    SegOption {
        value: value.into(),
        label: label.into(),
        disabled: false,
    }
}

/// `<Segmented>` — a row of mutually exclusive buttons.
pub fn segmented(
    id: impl Into<ElementId>,
    value: &str,
    options: Vec<SegOption>,
    small: bool,
    on_change: impl Fn(SharedString, &mut Window, &mut App) + Clone + 'static,
) -> Stateful<Div> {
    let t = theme();
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(t.card)
        .p(px(2.))
        .when(!small, |d| d.w_full())
        .children(options.into_iter().enumerate().map(|(i, o)| {
            let selected = o.value.as_ref() == value;
            let handler = on_change.clone();
            let v = o.value.clone();
            div()
                .id(("seg", i))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .map(|d| {
                    if small {
                        d.px(px(10.)).py(px(4.)).text_size(px(12.))
                    } else {
                        d.h(px(32.)).flex_1().text_size(px(12.))
                    }
                })
                // Selected: brand fill (≥3:1 against the card in both
                // themes) with white semibold text.
                .map(|d| {
                    if selected {
                        d.bg(t.brand_fill)
                            .text_color(t.brand_fill_contrast)
                            .font_weight(FontWeight::SEMIBOLD)
                    } else if o.disabled {
                        d.text_color(t.ink_3).font_weight(FontWeight::MEDIUM)
                    } else {
                        d.text_color(t.muted_foreground)
                            .font_weight(FontWeight::MEDIUM)
                            .hover(move |s| s.text_color(t.foreground).bg(t.hover))
                    }
                })
                .map(|d| {
                    let probe = Probe::new("tab", o.label.to_string())
                        .selected(selected)
                        .disabled(o.disabled);
                    if o.disabled {
                        d.e2e(probe)
                    } else {
                        d.cursor_pointer()
                            .clickable(probe, move |_, w, cx| handler(v.clone(), w, cx))
                    }
                })
                .child(o.label)
        }))
}

/// `<PrimaryButton>` (h-11).
pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    button(id, label).large()
}

/// `<SecondaryButton>`.
pub fn secondary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    button(id, label).outline().large()
}

/// Bordered option card used by the network / preset / method grids.
pub fn option_card(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    let t = theme();
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_start()
        .rounded_md()
        .border_1()
        .cursor_pointer()
        .border_color(if selected { t.primary } else { t.border })
        .bg(if selected {
            alpha(t.primary, 0.06)
        } else {
            t.card
        })
        .when(!selected, |d| {
            d.hover(move |s| s.border_color(alpha(t.primary, 0.6)))
        })
}

/// Network choice, shared by the create and recover wizards.
pub fn network_picker(
    value: &str,
    on_change: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    div()
        .flex()
        .flex_col()
        .child(field_label("Network", None))
        .child(network_options(value, on_change).mt_2())
}

/// The network option cards without a label (wrap in `crate::ui::field`).
pub fn network_options(
    value: &str,
    on_change: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .gap_2()
        .children(SUPPORTED_NETWORKS.iter().map(|(id, label, hint)| {
            let handler = on_change.clone();
            let id: &'static str = id;
            option_card(SharedString::from(format!("net-{id}")), value == id)
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .px_3()
                .py(px(10.))
                .clickable(
                    Probe::new("option", *label).selected(value == id),
                    move |_, w, cx| handler(id, w, cx),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.foreground)
                        .child(*label),
                )
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .mt(px(2.))
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(*hint),
                )
        }))
}

/// `<Fingerprint>` — lower-cased mono.
pub fn fingerprint(value: &str) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(12.))
        .text_color(theme().foreground)
        .child(value.to_lowercase())
}

pub fn truncate_middle(s: &str, head: usize, tail: usize) -> String {
    let n = s.chars().count();
    if n <= head + tail + 1 {
        return s.to_string();
    }
    let start: String = s.chars().take(head).collect();
    let end: String = s.chars().skip(n - tail).collect();
    format!("{start}…{end}")
}

/// Small destructive inline message (`text-[11px] text-destructive`).
pub fn error_line(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .text_color(theme().destructive)
        .child(text.into())
}

/// Muted 11px helper text.
pub fn hint_text(text: impl Into<SharedString>) -> Div {
    let t = theme();
    div()
        .text_size(px(12.))
        .line_height(px(17.))
        .text_color(t.muted_foreground)
        .child(text.into())
}

/// Muted 12px paragraph.
pub fn para(text: impl Into<SharedString>) -> Div {
    let t = theme();
    div()
        .text_size(px(12.))
        .line_height(px(19.))
        .text_color(t.muted_foreground)
        .child(text.into())
}
