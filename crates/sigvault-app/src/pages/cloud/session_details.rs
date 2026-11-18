//! `pages/SessionDetails.tsx` — the active session workflow: header with
//! connection status and exit, then the DEVICE_REGISTRATION or
//! TRANSACTION_SIGNING component driven by `activeSession.sessionState`.

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, Subscription, Window};
use serde_json::Value;
use sigvault_core::commands::cmd_exit_session;
use sigvault_core::State;

use super::common::{content_skeleton, main_window, short_id, skeleton_pill};
use super::device_creation::DeviceCreationSession;
use super::signing::{TransactionSigning, TransactionSigningData};
use crate::runtime::backend;
use crate::state::{store, ActiveSession};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, dot, Tone};

const DEVICE_REGISTRATION: &str = "DEVICE_REGISTRATION";
const TRANSACTION_SIGNING: &str = "TRANSACTION_SIGNING";

pub struct SessionDetailsPage {
    device_session: Option<Entity<DeviceCreationSession>>,
    signing: Option<Entity<TransactionSigning>>,
    _subs: Vec<Subscription>,
}

fn requirement(session: &ActiveSession, key: &str) -> Option<String> {
    session
        .session_state
        .requirements
        .as_ref()
        .and_then(|r| r.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn signing_data(session: &ActiveSession) -> Option<TransactionSigningData> {
    session
        .session_state
        .data
        .clone()
        .and_then(|d| serde_json::from_value(d).ok())
}

impl SessionDetailsPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let st = store(cx);
        let observe = cx.observe(&st, |this, _, cx| {
            this.sync_children(cx);
            cx.notify();
        });
        // Unmount: clear the activity log, as the React effect cleanup did.
        let release = cx.on_release(|_, cx| {
            store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
        });
        let mut this = Self {
            device_session: None,
            signing: None,
            _subs: vec![observe, release],
        };
        this.sync_children(cx);
        this
    }

    /// Create / update the workflow component for the current session type,
    /// passing the latest props (React re-render semantics: component state
    /// survives prop changes).
    fn sync_children(&mut self, cx: &mut Context<Self>) {
        let session = store(cx).read(cx).active_session.clone();
        let session_id = session.session_id.clone().unwrap_or_default();
        match session.session_state.session_type.as_deref() {
            Some(DEVICE_REGISTRATION) => {
                self.signing = None;
                let network = requirement(&session, "network").unwrap_or_else(|| "testnet".into());
                let path = requirement(&session, "derivation_path").unwrap_or_else(|| "m/84'/0'/0'".into());
                match &self.device_session {
                    Some(e) => e.update(cx, |c, cx| c.set_props(network, path, session_id, cx)),
                    None => {
                        self.device_session =
                            Some(cx.new(|_| DeviceCreationSession::new(network, path, session_id)));
                    }
                }
            }
            Some(TRANSACTION_SIGNING) => {
                self.device_session = None;
                match signing_data(&session) {
                    Some(data) => match &self.signing {
                        Some(e) => e.update(cx, |c, cx| c.set_props(data, session_id, cx)),
                        None => self.signing = Some(cx.new(|_| TransactionSigning::new(data, session_id))),
                    },
                    None => self.signing = None,
                }
            }
            _ => {
                self.device_session = None;
                self.signing = None;
            }
        }
    }

    fn exit(&mut self, cx: &mut Context<Self>) {
        store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
        let task = backend(cx).call(|app, st| async move {
            let window = main_window(&app)?;
            cmd_exit_session(window, State::new(&st)).await
        });
        // Exit is handled by the backend (route change); errors are ignored.
        cx.spawn(async move |_, _| {
            let _ = task.await;
        })
        .detach();
    }
}

impl Render for SessionDetailsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let session = store(cx).read(cx).active_session.clone();
        let st = &session.session_state;
        let session_type = st.session_type.clone();
        let connected = session.is_connected;

        let title = session_type
            .as_ref()
            .map(|ty| {
                let friendly = ty.replace('_', " ").to_lowercase();
                let mut chars = friendly.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => friendly,
                }
            })
            .unwrap_or_else(|| "Session details".into());

        let status_color = if connected { t.success } else { t.destructive };
        // Badge with a leading status dot.
        let status_badge = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded_full()
            .px(px(9.))
            .py(px(3.))
            .text_size(px(11.))
            .line_height(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(status_color)
            .bg(alpha(status_color, 0.14))
            .child(dot(status_color))
            .child(if connected { "Connected" } else { "Disconnected" });

        let chips = div()
            .mt_3()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(status_badge)
            .map(|d| match (&session_type, &session.session_id) {
                (Some(ty), _) => d.child(badge(ty.clone(), Tone::Secondary)),
                (None, Some(_)) => d.child(skeleton_pill("type-skeleton", px(112.))),
                (None, None) => d,
            })
            .map(|d| match &session.session_id {
                Some(id) => d.child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(short_id(id)),
                ),
                None => d.child(skeleton_pill("id-skeleton", px(160.))),
            });

        let header = div()
            .flex_none()
            .border_b_1()
            .border_color(t.border)
            .px_8()
            .py_4()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(768.))
                    .flex()
                    .flex_row()
                    .items_start()
                    .justify_between()
                    .gap_6()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child(title),
                            )
                            .child(chips),
                    )
                    .child(
                        button("exit-session", "Exit session")
                            .outline()
                            .small()
                            .icon("x")
                            .on_click(cx.listener(|this, _, _, cx| this.exit(cx))),
                    ),
            );

        // `!isConnected && !sessionState` → skeleton; the store always holds a
        // (possibly empty) state, so "empty" stands in for "absent".
        let state_empty = st.session_type.is_none() && st.error.is_none();
        let content = if !connected && state_empty {
            Some(content_skeleton("session-skeleton"))
        } else if !connected {
            None
        } else {
            match session_type.as_deref() {
                Some(DEVICE_REGISTRATION) => self.device_session.clone().map(|e| e.into_any_element()),
                Some(TRANSACTION_SIGNING) => Some(match &self.signing {
                    Some(e) => e.clone().into_any_element(),
                    None => content_skeleton("signing-skeleton"),
                }),
                _ => None,
            }
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("session-details-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_8()
                    .py_8()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(768.))
                                    .flex()
                                    .flex_col()
                                    .children(st.error.clone().map(|e| div().mb_6().child(alert(Tone::Destructive, e))))
                                    .children(content),
                            ),
                    ),
            )
    }
}
