//! Bottom activity log (`ActivityPanel` in App.tsx): shows the latest backend
//! notification with a spinner while work is in progress, expandable to the
//! full log.

use gpui::{div, prelude::*, px, Context, Window};

use crate::e2e::{E2eClick, Probe};
use crate::state::{store, NotificationLevel};
use crate::theme::theme;
use crate::ui::{icon, spinner};

pub struct ActivityPanel {
    expanded: bool,
}

impl ActivityPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe(&store(cx), |_, _, cx| cx.notify()).detach();
        Self { expanded: false }
    }
}

impl Render for ActivityPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let log = store(cx).read(cx).activity_log.clone();
        let Some(latest) = log.last().cloned() else {
            return div().into_any_element();
        };
        let n = log.len();
        let in_progress = latest.level != NotificationLevel::Success;
        let marker = |busy: bool| {
            if busy {
                spinner(px(14.)).into_any_element()
            } else {
                div()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(8.)).rounded_full().bg(t.success))
                    .into_any_element()
            }
        };

        div()
            .flex_none()
            .w_full()
            .border_t_1()
            .border_color(t.border)
            .bg(t.card)
            .child(
                div()
                    .id("activity-toggle")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .px_4()
                    .py(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.muted))
                    .clickable(
                        Probe::new("button", "Activity").selected(self.expanded),
                        cx.listener(|this, _, _, cx| {
                            this.expanded = !this.expanded;
                            cx.notify();
                        }),
                    )
                    .child(marker(in_progress))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(latest.message.clone()),
                    )
                    .child(
                        div()
                            .id("activity-clear")
                            .px_1()
                            .rounded(px(4.))
                            .text_size(px(12.))
                            .text_color(t.ink_3)
                            .hover(|s| s.text_color(t.foreground))
                            .clickable(Probe::new("button", "Clear activity"), |_, _, cx| {
                                cx.stop_propagation();
                                store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
                            })
                            .child("Clear"),
                    )
                    .child(
                        icon(if self.expanded {
                            "chevron-down"
                        } else {
                            "chevron-up"
                        })
                        .size(px(14.)),
                    ),
            )
            .when(self.expanded, |d| {
                d.child(
                    div()
                        .id("activity-log")
                        .max_h(px(192.))
                        .overflow_y_scroll()
                        .border_t_1()
                        .border_color(t.border)
                        .px_4()
                        .py_2()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .children(log.into_iter().enumerate().map(|(i, item)| {
                            let busy = i + 1 == n && item.level != NotificationLevel::Success;
                            div()
                                .flex()
                                .flex_row()
                                .items_start()
                                .gap_2()
                                .child(marker(busy))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .line_height(px(17.))
                                        .text_color(if busy {
                                            t.foreground
                                        } else {
                                            t.muted_foreground
                                        })
                                        .child(format!("{} — {}", item.title, item.message)),
                                )
                        })),
                )
            })
            .into_any_element()
    }
}
