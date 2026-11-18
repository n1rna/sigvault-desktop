use gpui::{
    anchored, deferred, div, prelude::*, px, Context, Corner, EventEmitter, SharedString, Window,
};

use super::common::icon;
use crate::theme::{alpha, theme};

#[derive(Clone)]
pub struct SelectOption {
    pub value: SharedString,
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub disabled: bool,
}

impl SelectOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
            disabled: false,
        }
    }
    pub fn description(mut self, d: impl Into<SharedString>) -> Self {
        self.description = Some(d.into());
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

#[derive(Clone, Debug)]
pub enum SelectEvent {
    Changed(SharedString),
}

/// Dropdown select. Create with `cx.new(|_| Select::new(options))`,
/// subscribe for [`SelectEvent::Changed`], read with `.value()`.
pub struct Select {
    options: Vec<SelectOption>,
    selected: Option<usize>,
    placeholder: SharedString,
    open: bool,
    disabled: bool,
}

impl EventEmitter<SelectEvent> for Select {}

impl Select {
    pub fn new(options: Vec<SelectOption>) -> Self {
        Self {
            options,
            selected: None,
            placeholder: "Select…".into(),
            open: false,
            disabled: false,
        }
    }

    pub fn placeholder(mut self, p: impl Into<SharedString>) -> Self {
        self.placeholder = p.into();
        self
    }

    pub fn with_value(mut self, value: &str) -> Self {
        self.selected = self.options.iter().position(|o| o.value.as_ref() == value);
        self
    }

    pub fn value(&self) -> Option<SharedString> {
        self.selected.map(|i| self.options[i].value.clone())
    }

    pub fn set_value(&mut self, value: &str, cx: &mut Context<Self>) {
        self.selected = self.options.iter().position(|o| o.value.as_ref() == value);
        cx.notify();
    }

    pub fn set_options(&mut self, options: Vec<SelectOption>, cx: &mut Context<Self>) {
        let current = self.value();
        self.options = options;
        self.selected = current.and_then(|v| self.options.iter().position(|o| o.value == v));
        cx.notify();
    }

    pub fn set_disabled(&mut self, d: bool, cx: &mut Context<Self>) {
        self.disabled = d;
        cx.notify();
    }
}

impl Render for Select {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let current = self
            .selected
            .map(|i| self.options[i].label.clone())
            .unwrap_or_else(|| self.placeholder.clone());
        let has_value = self.selected.is_some();
        let disabled = self.disabled;

        div()
            .w_full()
            .child(
                div()
                    .id("select-trigger")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .px_3()
                    .py(px(7.))
                    .rounded_md()
                    .border_1()
                    .border_color(if self.open { alpha(t.ring, 0.7) } else { t.input })
                    .bg(alpha(t.background, 0.6))
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .when(disabled, |d| d.opacity(0.55))
                    .when(!disabled, |d| d.cursor_pointer())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !disabled {
                            this.open = !this.open;
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .text_color(if has_value { t.foreground } else { t.muted_foreground })
                            .child(current),
                    )
                    .child(icon("chevron-down").size(px(14.))),
            )
            .when(self.open, |d| {
                d.child(
                    deferred(
                        anchored().anchor(Corner::TopLeft).snap_to_window().child(
                            div()
                                .id("select-menu")
                                .occlude()
                                .mt_1()
                                .min_w(px(220.))
                                .max_h(px(320.))
                                .overflow_y_scroll()
                                .rounded_lg()
                                .border_1()
                                .border_color(t.border)
                                .bg(t.popover)
                                .shadow_lg()
                                .py_1()
                                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                    this.open = false;
                                    cx.notify();
                                }))
                                .children(self.options.iter().enumerate().map(|(i, o)| {
                                    let selected = Some(i) == self.selected;
                                    let disabled = o.disabled;
                                    let value = o.value.clone();
                                    div()
                                        .id(i)
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .px_3()
                                        .py(px(7.))
                                        .when(!disabled, |d| d.cursor_pointer().hover(|s| s.bg(alpha(t.muted, 0.8))))
                                        .when(disabled, |d| d.opacity(0.45))
                                        .when(selected, |d| d.bg(alpha(t.primary, 0.08)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if disabled {
                                                return;
                                            }
                                            this.selected = Some(i);
                                            this.open = false;
                                            cx.emit(SelectEvent::Changed(value.clone()));
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex()
                                                .flex_col()
                                                .child(
                                                    div()
                                                        .text_size(px(13.))
                                                        .text_color(t.foreground)
                                                        .child(o.label.clone()),
                                                )
                                                .children(o.description.clone().map(|desc| {
                                                    div()
                                                        .text_size(px(11.))
                                                        .text_color(t.muted_foreground)
                                                        .child(desc)
                                                })),
                                        )
                                        .when(selected, |d| {
                                            d.child(icon("check").size(px(14.)).text_color(t.primary))
                                        })
                                })),
                        ),
                    )
                    .with_priority(5),
                )
            })
    }
}
