//! `pages/RemoteSessions.tsx` — list signing ceremonies the user was invited
//! to, filter / sort them and join one over the session websocket.

use std::cmp::Ordering;

use gpui::{div, prelude::*, px, Context, FontWeight, SharedString, Window};
use sigvault_core::commands::{
    cmd_get_remote_sessions, cmd_start_session_websocket_connection, cmd_update_remote_sessions,
};
use sigvault_core::State;

use super::common::{main_window, short_id, skeleton};
use crate::runtime::backend;
use crate::state::{store, RemoteSession};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, icon, icon_button, Tone};

fn parse_time(iso: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(iso).ok()
}

fn format_relative(iso: Option<&str>) -> String {
    let Some(iso) = iso else {
        return "—".into();
    };
    let Some(then) = parse_time(iso) else {
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
    then.with_timezone(&chrono::Local).format("%-m/%-d/%Y").to_string()
}

pub struct RemoteSessionsPage {
    sessions: Vec<RemoteSession>,
    loading: bool,
    connecting_to: Option<String>,
    error: Option<SharedString>,
    type_filter: Option<String>,
    newest_first: bool,
}

impl RemoteSessionsPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            sessions: Vec::new(),
            loading: true,
            connecting_to: None,
            error: None,
            type_filter: None,
            newest_first: true,
        };
        this.fetch_sessions(cx);
        this
    }

    /// `cmd_update_remote_sessions` then `cmd_get_remote_sessions`. Errors
    /// are non-fatal (the list just stays as it was).
    fn fetch_sessions(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();
        let task = backend(cx).call(|app, st| async move {
            let window = main_window(&app)?;
            cmd_update_remote_sessions(window, State::new(&st)).await?;
            Ok(cmd_get_remote_sessions(State::new(&st)).await.ok())
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Ok(Some(sessions)) = res {
                    this.sessions = sessions
                        .into_iter()
                        .map(|s| RemoteSession {
                            id: s.id,
                            status: s.status,
                            session_type: s.session_type,
                            created_at: None,
                        })
                        .collect();
                }
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn connect(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.connecting_to = Some(session_id.clone());
        cx.notify();
        // Start the new session from a clean workflow state (the React
        // reducer effectively dropped `sessionState` on the next
        // `active_session` update).
        store(cx).update(cx, |s, cx| s.reset_session_state(cx));
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_start_session_websocket_connection(window, State::new(&st), session_id).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if !r.success => {
                        this.error = Some(
                            if r.message.is_empty() { "Failed to connect".to_string() } else { r.message }.into(),
                        )
                    }
                    Ok(_) => {}
                    Err(_) => this.error = Some("Failed to connect to session".into()),
                }
                this.connecting_to = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn session_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self.sessions.iter().map(|s| s.session_type.clone()).collect();
        types.sort();
        types.dedup();
        types
    }

    fn filtered_sorted(&self) -> Vec<(usize, &RemoteSession)> {
        let mut out: Vec<(usize, &RemoteSession)> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| self.type_filter.as_ref().is_none_or(|f| &s.session_type == f))
            .collect();
        let newest = self.newest_first;
        out.sort_by(|(ia, a), (ib, b)| {
            if let (Some(ta), Some(tb)) = (
                a.created_at.as_deref().and_then(parse_time),
                b.created_at.as_deref().and_then(parse_time),
            ) {
                let ord = tb.cmp(&ta);
                if ord != Ordering::Equal {
                    return if newest { ord } else { ord.reverse() };
                }
            }
            if newest {
                ib.cmp(ia)
            } else {
                ia.cmp(ib)
            }
        });
        out
    }
}

impl Render for RemoteSessionsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let total = self.sessions.len();
        let active = self.sessions.iter().filter(|s| s.status.eq_ignore_ascii_case("active")).count();
        let rows = self.filtered_sorted();
        let shown = rows.len();

        // ── Header ──
        let header = div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child("Remote sessions"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .max_w(px(420.))
                            .text_size(px(13.))
                            .line_height(px(21.))
                            .text_color(t.muted_foreground)
                            .child(
                                "Join active signing ceremonies or wait for new ones. Connect with your hardware wallet to contribute a signature.",
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child(active.to_string()),
                            )
                            .child(format!("active · {total} total")),
                    )
                    .child(
                        icon_button("refresh-sessions", "refresh")
                            .outline()
                            .loading(self.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.fetch_sessions(cx))),
                    ),
            );

        // ── Filter bar ──
        let filter_bar = (total > 0).then(|| {
            let mut chips: Vec<(Option<String>, String)> = vec![(None, "All".into())];
            chips.extend(self.session_types().into_iter().map(|ty| (Some(ty.clone()), ty)));
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .children(chips.into_iter().enumerate().map(|(ix, (value, label))| {
                            let is_active = self.type_filter == value;
                            div()
                                .id(("type-filter", ix))
                                .flex()
                                .items_center()
                                .h(px(28.))
                                .px_3()
                                .rounded_full()
                                .border_1()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .map(|d| {
                                    if is_active {
                                        d.border_color(alpha(t.primary, 0.5))
                                            .bg(alpha(t.primary, 0.08))
                                            .text_color(t.foreground)
                                    } else {
                                        d.border_color(t.border)
                                            .text_color(t.muted_foreground)
                                            .hover(move |s| s.text_color(t.foreground))
                                    }
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.type_filter = value.clone();
                                    cx.notify();
                                }))
                                .child(label)
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_none()
                        .items_center()
                        .gap_2()
                        .when(self.type_filter.is_some(), |d| {
                            d.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(format!("{shown} / {total}")),
                            )
                        })
                        .child(
                            button("sort-order", if self.newest_first { "Newest" } else { "Oldest" })
                                .ghost()
                                .small()
                                .icon(if self.newest_first { "cloud-arrow-down" } else { "cloud-arrow-up" })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.newest_first = !this.newest_first;
                                    cx.notify();
                                })),
                        ),
                )
        });

        // ── List ──
        let list = if self.loading && total == 0 {
            div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .overflow_hidden()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .bg(t.border)
                .children((0..4usize).map(|i| skeleton(("session-skeleton", i), px(68.))))
                .into_any_element()
        } else if rows.is_empty() {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .rounded_lg()
                .border_1()
                .border_dashed()
                .border_color(t.border)
                .bg(alpha(t.card, 0.3))
                .py_16()
                .child(
                    div()
                        .size(px(48.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(t.border)
                        .bg(t.background)
                        .child(icon("cloud-calendar").size(px(20.)).text_color(alpha(t.muted_foreground, 0.5))),
                )
                .child(
                    div()
                        .mt_4()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.foreground)
                        .child(if total == 0 { "No sessions yet" } else { "Nothing matches that filter" }),
                )
                .child(
                    div()
                        .mt_1()
                        .max_w(px(320.))
                        .text_center()
                        .text_size(px(12.))
                        .line_height(px(19.))
                        .text_color(t.muted_foreground)
                        .child(if total == 0 {
                            "When someone invites you to a signing ceremony it will appear here."
                        } else {
                            "Try clearing the filter or refreshing the list."
                        }),
                )
                .into_any_element()
        } else {
            let last = rows.len() - 1;
            div()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .children(rows.into_iter().enumerate().map(|(pos, (ix, session))| {
                    let is_active = session.status.eq_ignore_ascii_case("active");
                    let is_connecting = self.connecting_to.as_deref() == Some(session.id.as_str());
                    let id = session.id.clone();
                    div()
                        .id(("session-row", ix))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_4()
                        .bg(t.card)
                        .px_4()
                        .py(px(14.))
                        .when(pos != last, |d| d.border_b_1().border_color(t.border))
                        .hover(move |s| s.bg(alpha(t.muted, 0.4)))
                        .child(
                            div()
                                .size(px(36.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .border_1()
                                .border_color(t.border)
                                .bg(t.background)
                                .child(icon("cloud-shield").size(px(16.))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(px(13.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(t.foreground)
                                                .child(session.session_type.clone()),
                                        )
                                        .child(badge(
                                            session.status.clone(),
                                            if is_active { Tone::Success } else { Tone::Secondary },
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .text_size(px(12.))
                                        .text_color(t.muted_foreground)
                                        .child(div().font_family(FONT_MONO).child(short_id(&session.id)))
                                        .when_some(session.created_at.clone(), |d, created| {
                                            d.child(div().text_color(alpha(t.muted_foreground, 0.4)).child("·"))
                                                .child(format_relative(Some(&created)))
                                        }),
                                ),
                        )
                        .child(
                            button(("join", ix), if is_connecting { "Joining" } else { "Join" })
                                .small()
                                .loading(is_connecting)
                                .when(!is_connecting, |b| b.icon_right("arrow-right"))
                                .on_click(cx.listener(move |this, _, _, cx| this.connect(id.clone(), cx))),
                        )
                }))
                .into_any_element()
        };

        div().size_full().child(
            div().id("remote-sessions-scroll").size_full().overflow_y_scroll().child(
                div().w_full().flex().flex_col().items_center().px_10().py(px(36.)).child(
                    div()
                        .w_full()
                        .max_w(px(768.))
                        .flex()
                        .flex_col()
                        .gap_6()
                        .child(header)
                        .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                        .children(filter_bar)
                        .child(list),
                ),
            ),
        )
    }
}
