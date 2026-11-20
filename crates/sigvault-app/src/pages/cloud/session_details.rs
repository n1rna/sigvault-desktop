//! `pages/SessionDetails.tsx` — the active session workflow inside the page
//! frame: crumbs, live/connected badge, Leave session, a stepper and a
//! footer bar owned by the DEVICE_REGISTRATION or TRANSACTION_SIGNING
//! component driven by `activeSession.sessionState`.

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, Subscription, Window};
use serde_json::Value;
use sigvault_core::commands::cmd_exit_session;
use sigvault_core::State;

use super::common::{
    main_window, session_blurb, session_title, short_id, skeleton, skeleton_list, status_badge,
    DEVICE_REGISTRATION, TRANSACTION_SIGNING,
};
use super::device_creation::{self, DeviceCreationSession};
use super::signing::{self, TransactionSigning, TransactionSigningData};
use crate::runtime::backend;
use crate::state::{store, ActiveSession};
use crate::theme::{theme, FONT_MONO};
use crate::ui::{alert, button, footer_bar, page_frame, section, stepper, Tone};

pub struct SessionDetailsPage {
    device_session: Option<Entity<DeviceCreationSession>>,
    signing: Option<Entity<TransactionSigning>>,
    leaving: bool,
    _subs: Vec<Subscription>,
    _child_sub: Option<Subscription>,
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
            leaving: false,
            _subs: vec![observe, release],
            _child_sub: None,
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
                        let e = cx.new(|_| DeviceCreationSession::new(network, path, session_id));
                        // Footer + stepper live in this page's frame.
                        self._child_sub = Some(cx.observe(&e, |_, _, cx| cx.notify()));
                        e.update(cx, |c, cx| c.apply_dev_step(cx));
                        self.device_session = Some(e);
                    }
                }
            }
            Some(TRANSACTION_SIGNING) => {
                self.device_session = None;
                let network = requirement(&session, "network");
                match signing_data(&session) {
                    Some(data) => match &self.signing {
                        Some(e) => e.update(cx, |c, cx| c.set_props(data, session_id, network, cx)),
                        None => {
                            let e = cx.new(|_| TransactionSigning::new(data, session_id, network));
                            self._child_sub = Some(cx.observe(&e, |_, _, cx| cx.notify()));
                            e.update(cx, |c, cx| c.apply_dev_step(cx));
                            self.signing = Some(e);
                        }
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
        self.leaving = true;
        cx.notify();
        store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
        let task = backend(cx).call(|app, st| async move {
            let window = main_window(&app)?;
            cmd_exit_session(window, State::new(&st)).await
        });
        // Exit is handled by the backend (route change); errors are ignored.
        cx.spawn(async move |this, cx| {
            let _ = task.await;
            this.update(cx, |this, cx| {
                this.leaving = false;
                cx.notify();
            })
            .ok();
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

        let title = session_type.as_deref().map(session_title).unwrap_or_else(|| {
            if connected { "Waiting for session…".into() } else { "Joining session…".into() }
        });

        let adornment = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(if connected {
                status_badge("Live", t.success)
            } else if session.session_id.is_some() {
                status_badge("Connecting", t.warning)
            } else {
                status_badge("Disconnected", t.destructive)
            })
            .map(|d| match &session.session_id {
                Some(id) => d.child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(short_id(id)),
                ),
                None => d.child(skeleton("id-skeleton", px(110.), px(12.))),
            });

        // Workflow-specific stepper, subtitle, body and footer.
        let (steps, step, subtitle, footer, content): (
            Option<&[&str]>,
            usize,
            String,
            Option<(Vec<AnyElement>, Vec<AnyElement>)>,
            Option<AnyElement>,
        ) = match (session_type.as_deref(), connected) {
            (Some(DEVICE_REGISTRATION), true) if self.device_session.is_some() => {
                let e = self.device_session.clone().unwrap();
                let (idx, sub, foot) = e.update(cx, |c, cx| (c.step_index(), c.subtitle(), c.footer(cx)));
                (Some(&device_creation::STEPS[..]), idx, sub.into(), Some(foot), Some(e.into_any_element()))
            }
            (Some(TRANSACTION_SIGNING), true) => match self.signing.clone() {
                Some(e) => {
                    let (idx, sub, foot) = e.update(cx, |c, cx| (c.step_index(), c.subtitle(), c.footer(cx)));
                    (Some(&signing::STEPS[..]), idx, sub.into(), Some(foot), Some(e.into_any_element()))
                }
                None => (
                    Some(&signing::STEPS[..]),
                    0,
                    session_blurb(TRANSACTION_SIGNING).into(),
                    None,
                    Some(skeleton_list("signing-skeleton", 3).into_any_element()),
                ),
            },
            (Some(ty), true) => (
                None,
                0,
                session_blurb(ty).into(),
                None,
                Some(
                    section("Waiting")
                        .child(alert(Tone::Secondary, "This session type isn't supported by this version of SigVault."))
                        .into_any_element(),
                ),
            ),
            (None, true) => (
                None,
                0,
                "Connected. Waiting for the coordinator to send the next step…".into(),
                None,
                Some(skeleton_list("session-wait", 2).into_any_element()),
            ),
            // `!isConnected && !sessionState` → skeleton; with a state but
            // no connection, nothing is shown (as in the React page).
            (_, false) => {
                let empty = st.session_type.is_none() && st.error.is_none();
                (
                    None,
                    0,
                    "Opening a live connection to the session…".into(),
                    None,
                    empty.then(|| skeleton_list("session-skeleton", 3).into_any_element()),
                )
            }
        };

        let crumb = session_type.as_deref().map(session_title).unwrap_or_else(|| "Session".into());
        let leave = button("exit-session", if self.leaving { "Leaving…" } else { "Leave session" })
            .outline()
            .small()
            .icon("log-out")
            .loading(self.leaving)
            .on_click(cx.listener(|this, _, _, cx| this.exit(cx)));

        let frame = page_frame("session-details")
            .crumb("Sessions", None)
            .crumb(crumb, None)
            .title(title)
            .title_adornment(adornment)
            .subtitle(subtitle)
            .action(leave)
            .when_some(steps, |f, steps| f.header_extra(stepper(steps, step)))
            .when_some(footer.filter(|(l, r)| !l.is_empty() || !r.is_empty()), |f, (l, r)| {
                f.footer(footer_bar(l, r))
            })
            .children(st.error.clone().map(|e| alert(Tone::Destructive, e)))
            .children(content);

        div().size_full().relative().child(frame)
    }
}
