//! Keychain password dialog. Unlocks the keychain, or sets its password the
//! first time a private key is stored. Embed the entity inside a
//! `crate::ui::modal` and listen for [`UnlockEvent`].

use gpui::{div, prelude::*, px, Context, Entity, EventEmitter, SharedString, Window};

use crate::keychain::keychain;
use crate::theme::theme;
use crate::ui::{alert, button, field, InputEvent, TextInput, Tone};

#[derive(Debug, Clone)]
pub enum UnlockEvent {
    Unlocked,
    Cancelled,
}

pub struct KeychainUnlock {
    password: Entity<TextInput>,
    confirm: Entity<TextInput>,
    /// No password set yet → "create" mode with confirmation.
    creating: bool,
    reason: SharedString,
    busy: bool,
    error: Option<SharedString>,
}

impl EventEmitter<UnlockEvent> for KeychainUnlock {}

impl KeychainUnlock {
    /// `reason` explains why the password is needed ("to store this key").
    pub fn new(reason: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let creating = !keychain(cx).read(cx).has_password();
        let password = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder(if creating { "At least 8 characters" } else { "Keychain password" })
        });
        let confirm = cx.new(|cx| TextInput::new(window, cx).masked().placeholder("Repeat password"));
        for input in [&password, &confirm] {
            cx.subscribe(input, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Submit => this.submit(cx),
                InputEvent::Changed => {
                    this.error = None;
                    cx.notify();
                }
            })
            .detach();
        }
        password.read(cx).focus(window);
        Self { password, confirm, creating, reason: reason.into(), busy: false, error: None }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let pw = self.password.read(cx).text().to_string();
        if self.creating {
            if pw.chars().count() < 8 {
                self.error = Some("Use at least 8 characters".into());
                cx.notify();
                return;
            }
            if pw != self.confirm.read(cx).text() {
                self.error = Some("Passwords don't match".into());
                cx.notify();
                return;
            }
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let task = keychain(cx).update(cx, |k, cx| k.unlock(pw, cx));
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match res {
                    Ok(()) => cx.emit(UnlockEvent::Unlocked),
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for KeychainUnlock {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let (title, body) = if self.creating {
            (
                "Create a keychain password",
                "Private keys in your library are encrypted with this password. You'll enter it once per session to use them.",
            )
        } else {
            ("Unlock keychain", "Enter your keychain password")
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(16.)).font_weight(gpui::FontWeight::SEMIBOLD).child(title))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(t.muted_foreground)
                            .child(if self.creating {
                                body.to_string()
                            } else {
                                format!("{body} {}.", self.reason).replace(" .", ".")
                            }),
                    ),
            )
            .child(field("Password", self.password.clone(), None))
            .when(self.creating, |d| d.child(field("Confirm password", self.confirm.clone(), None)))
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(
                        button("unlock-cancel", "Cancel")
                            .ghost()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(UnlockEvent::Cancelled))),
                    )
                    .child(
                        button("unlock-submit", if self.creating { "Create password" } else { "Unlock" })
                            .icon("unlock")
                            .loading(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
