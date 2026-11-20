//! `pages/RemoteSessions.tsx` — list signing ceremonies the user was invited
//! to, filter / sort them and join one over the session websocket.

use std::cmp::Ordering;

use gpui::{div, prelude::*, px, Context, FontWeight, SharedString, Subscription, Window};
use sigvault_core::commands::{
    cmd_get_remote_sessions, cmd_start_session_websocket_connection, cmd_update_remote_sessions,
};
use sigvault_core::State;

use super::common::{
    card_message, format_relative, icon_tile, list_row, main_window, parse_time, segmented,
    sep_dot, session_icon, session_title, short_id, skeleton_list,
};
use crate::runtime::backend;
use crate::state::{store, RemoteSession, StoreEvent};
use crate::theme::{theme, FONT_MONO};
use crate::ui::{alert, badge, button, card, divider, icon, page_frame, Tone};

pub struct RemoteSessionsPage {
    sessions: Vec<RemoteSession>,
    loading: bool,
    connecting_to: Option<String>,
    error: Option<SharedString>,
    type_filter: Option<String>,
    newest_first: bool,
    _subs: Vec<Subscription>,
}

impl RemoteSessionsPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Start from what the store already knows (pushed by the backend via
        // `update_remote_sessions`), then refresh.
        let st = store(cx);
        let known = st.read(cx).remote_sessions.clone();
        let sub = cx.subscribe(&st, |this, store, ev: &StoreEvent, cx| {
            if matches!(ev, StoreEvent::SessionUpdated | StoreEvent::AuthChanged) {
                return;
            }
            let latest = store.read(cx).remote_sessions.clone();
            if !latest.is_empty() && this.sessions.is_empty() {
                this.sessions = latest;
                cx.notify();
            }
        });
        let mut this = Self {
            sessions: known,
            loading: true,
            connecting_to: None,
            error: None,
            type_filter: None,
            newest_first: true,
            _subs: vec![sub],
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
                    let previous = std::mem::take(&mut this.sessions);
                    this.sessions = sessions
                        .into_iter()
                        .map(|s| RemoteSession {
                            created_at: previous
                                .iter()
                                .find(|p| p.id == s.id)
                                .and_then(|p| p.created_at.clone()),
                            id: s.id,
                            status: s.status,
                            session_type: s.session_type,
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
                            if r.message.is_empty() {
                                "Failed to connect".to_string()
                            } else {
                                r.message
                            }
                            .into(),
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
        let mut types: Vec<String> = self
            .sessions
            .iter()
            .map(|s| s.session_type.clone())
            .collect();
        types.sort();
        types.dedup();
        types
    }

    fn filtered_sorted(&self) -> Vec<(usize, &RemoteSession)> {
        let mut out: Vec<(usize, &RemoteSession)> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                self.type_filter
                    .as_ref()
                    .is_none_or(|f| &s.session_type == f)
            })
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

impl RemoteSessionsPage {
    fn render_row(
        &self,
        pos: usize,
        ix: usize,
        session: &RemoteSession,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let is_active = session.status.eq_ignore_ascii_case("active");
        let is_connecting = self.connecting_to.as_deref() == Some(session.id.as_str());
        let busy = self.connecting_to.is_some();
        let id = session.id.clone();
        let id2 = session.id.clone();
        let status = {
            let mut chars = session.status.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => "Unknown".into(),
            }
        };
        let join = button(
            ("join", ix),
            if is_connecting { "Joining…" } else { "Join" },
        )
        .small()
        .loading(is_connecting)
        .disabled(busy && !is_connecting)
        .when(!is_active, |b| b.outline())
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.connect(id.clone(), cx)
        }));
        list_row(("session-row", pos))
            .h(px(60.))
            .gap_4()
            .when(!busy, |d| {
                d.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.connect(id2.clone(), cx)))
            })
            .child(icon_tile(
                session_icon(&session.session_type),
                32.,
                Some(if is_active {
                    t.primary
                } else {
                    t.muted_foreground
                }),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(session_title(&session.session_type)),
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
                                d.child(sep_dot())
                                    .child(format!("Created {}", format_relative(Some(&created))))
                            }),
                    ),
            )
            .child(div().w(px(96.)).flex_none().flex().child(badge(
                status,
                if is_active {
                    Tone::Success
                } else {
                    Tone::Secondary
                },
            )))
            .child(
                div()
                    .w(px(84.))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(join),
            )
    }
}

impl Render for RemoteSessionsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let total = self.sessions.len();
        let active = self
            .sessions
            .iter()
            .filter(|s| s.status.eq_ignore_ascii_case("active"))
            .count();
        let rows: Vec<(usize, RemoteSession)> = self
            .filtered_sorted()
            .into_iter()
            .map(|(i, s)| (i, s.clone()))
            .collect();

        // ── Filters: type tabs + sort ──
        let filters = (total > 0).then(|| {
            let mut items = vec![("all".to_string(), format!("All  {total}"))];
            for ty in self.session_types() {
                let n = self
                    .sessions
                    .iter()
                    .filter(|s| s.session_type == ty)
                    .count();
                items.push((ty.clone(), format!("{}  {n}", session_title(&ty))));
            }
            let current = self.type_filter.clone().unwrap_or_else(|| "all".into());
            let this = cx.entity().downgrade();
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .child(segmented(
                    "session-filter",
                    items,
                    &current,
                    move |key, _, cx| {
                        this.update(cx, |this, cx| {
                            this.type_filter = (key != "all").then_some(key);
                            cx.notify();
                        })
                        .ok();
                    },
                ))
                .child(
                    button(
                        "sort-order",
                        if self.newest_first {
                            "Newest first"
                        } else {
                            "Oldest first"
                        },
                    )
                    .ghost()
                    .small()
                    .icon("chevrons-up-down")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.newest_first = !this.newest_first;
                        cx.notify();
                    })),
                )
        });

        // ── List ──
        let list = if self.loading && total == 0 {
            skeleton_list("session-skeleton", 4).into_any_element()
        } else if total == 0 {
            card_message(
                "users",
                Some(t.primary),
                "No sessions yet",
                "When a co-signer invites you to register a device or sign a transaction, the session shows up here.",
            )
            .child(
                div().mt_4().child(
                    button("empty-refresh", "Check again")
                        .outline()
                        .small()
                        .icon("refresh")
                        .on_click(cx.listener(|this, _, _, cx| this.fetch_sessions(cx))),
                ),
            )
            .into_any_element()
        } else if rows.is_empty() {
            card()
                .items_center()
                .py_10()
                .gap_1()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child("No sessions match this filter"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Show all sessions or refresh the list."),
                )
                .into_any_element()
        } else {
            let mut list = card().overflow_hidden();
            for (pos, (ix, session)) in rows.iter().enumerate() {
                if pos > 0 {
                    list = list.child(divider());
                }
                list = list.child(self.render_row(pos, *ix, session, cx));
            }
            list.into_any_element()
        };

        let summary = (total > 0).then(|| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(icon("info").size(px(14.)))
                .child(format!(
                    "{active} active of {total}. Joining opens a live connection; you'll be asked to connect your hardware wallet."
                ))
        });

        page_frame("sessions")
            .title("Sessions")
            .subtitle("Signing sessions you've been invited to: register a device or approve a team transaction.")
            .action(
                button("refresh-sessions", "Refresh")
                    .outline()
                    .small()
                    .icon("refresh")
                    .loading(self.loading)
                    .on_click(cx.listener(|this, _, _, cx| this.fetch_sessions(cx))),
            )
            .when_some(filters, |f, el| f.header_extra(el))
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .child(list)
            .children(summary)
    }
}
