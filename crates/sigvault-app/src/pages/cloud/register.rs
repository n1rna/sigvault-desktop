//! `pages/MachineRegistration.tsx` — name this machine for the account.
//!
//! The React page invokes `cmd_register_new_machine`, which neither the
//! Tauri backend nor `sigvault-core` defines; the invoke always rejected
//! with a string, so the page showed its fallback error. That behaviour is
//! reproduced here without a backend call. `machine_id` / `machine_type`
//! came from URL query params nothing ever set, so those rows stay hidden
//! unless populated.

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Subscription, Window};

use super::common::icon_tile;
use crate::theme::{theme, FONT_MONO};
use crate::ui::{alert, button, card, field, footer_bar, page_frame, section, InputEvent, TextInput, Tone, FORM_WIDTH};

/// (icon, text) points explaining the registration.
const VALUE_POINTS: [(&str, &str); 3] = [
    ("users", "Co-signers see this name when you join their sessions."),
    ("hard-drive", "Bound to this computer's identifier, not to a wallet."),
    ("pen", "You can rename it later in the web dashboard."),
];

pub struct MachineRegistrationPage {
    name_input: Entity<TextInput>,
    machine_id: String,
    machine_type: String,
    loading: bool,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl MachineRegistrationPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| TextInput::new(window, cx).placeholder("e.g. Nima's MacBook Pro"));
        let sub = cx.subscribe(&name_input, |this, _, ev: &InputEvent, cx| match ev {
            InputEvent::Changed => cx.notify(),
            InputEvent::Submit => this.submit(cx),
        });
        Self {
            name_input,
            machine_id: String::new(),
            machine_type: String::new(),
            loading: false,
            error: None,
            _subs: vec![sub],
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).text().to_string();
        if self.loading || name.is_empty() {
            return;
        }
        // `cmd_register_new_machine` does not exist in the backend; the
        // React invoke rejected immediately and fell through to this message.
        self.error = Some("Failed to register machine. Please try again.".into());
        cx.notify();
    }

}

impl Render for MachineRegistrationPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let name_empty = self.name_input.read(cx).text().is_empty();
        let read_only = |label: &'static str, value: String| {
            field(
                label,
                div()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.muted)
                    .px_3()
                    .font_family(FONT_MONO)
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .child(value),
                None,
            )
        };

        let footer = footer_bar(
            vec![],
            vec![button("register-machine", if self.loading { "Registering…" } else { "Register computer" })
                .loading(self.loading)
                .disabled(name_empty)
                .when(!self.loading, |b| b.icon_right("arrow-right"))
                .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
                .into_any_element()],
        );

        page_frame("register")
            .title("Register this computer")
            .subtitle("SigVault ties signing to the computer running the desktop app. Name it so your co-signers recognize it.")
            .footer(footer)
            .children(self.error.clone().map(|e| div().max_w(FORM_WIDTH).child(alert(Tone::Destructive, e))))
            .child(
                div()
                    .max_w(FORM_WIDTH)
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        section("Computer").child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_4()
                                .child(field(
                                    "Name",
                                    self.name_input.clone(),
                                    Some("For example the computer's owner and model.".into()),
                                ))
                                .when(!self.machine_id.is_empty(), |d| {
                                    d.child(read_only("Machine ID", self.machine_id.clone()))
                                })
                                .when(!self.machine_type.is_empty(), |d| {
                                    d.child(read_only("Machine type", self.machine_type.clone()))
                                }),
                        ),
                    )
                    .child(
                        card().p_4().gap_3().children(VALUE_POINTS.iter().map(|(icon_name, text)| {
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(icon_tile(icon_name, 28., Some(t.primary)))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(t.muted_foreground)
                                        .child(*text),
                                )
                        })),
                    ),
            )
    }
}
