//! `pages/MachineRegistration.tsx` — name this machine for the account.
//!
//! The React page invokes `cmd_register_new_machine`, which neither the
//! Tauri backend nor `sigvault-core` defines; the invoke always rejected
//! with a string, so the page showed its fallback error. That behaviour is
//! reproduced here without a backend call. `machine_id` / `machine_type`
//! came from URL query params nothing ever set, so those rows stay hidden
//! unless populated.

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Subscription, Window};

use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, button, icon, InputEvent, TextInput, Tone};

const VALUE_POINTS: [&str; 3] = [
    "Recognizable across multiple machines",
    "Bound to this device's identifier",
    "Editable later in the web dashboard",
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

    fn read_only_row(label: &'static str, tag: &'static str, value: String) -> impl IntoElement {
        let t = theme();
        div()
            .child(
                div()
                    .mb_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .child(label)
                    .child(div().text_color(alpha(t.muted_foreground, 0.5)).child(tag)),
            )
            .child(
                div()
                    .h(px(44.))
                    .w_full()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(alpha(t.muted, 0.3))
                    .px(px(14.))
                    .font_family(FONT_MONO)
                    .text_size(px(11.))
                    .text_color(t.muted_foreground)
                    .child(value),
            )
    }
}

impl Render for MachineRegistrationPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let name_empty = self.name_input.read(cx).text().is_empty();

        let intro = div()
            .flex_1()
            .flex()
            .flex_col()
            .justify_center()
            .px_12()
            .py_10()
            .child(
                div()
                    .max_w(px(440.))
                    .text_size(px(28.))
                    .line_height(px(34.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.foreground)
                    .child("Give this device a name you'll recognize"),
            )
            .child(
                div()
                    .mt_4()
                    .max_w(px(420.))
                    .text_size(px(13.))
                    .line_height(px(21.))
                    .text_color(t.muted_foreground)
                    .child(
                        "SigVault ties signing privileges to the physical machine running the desktop app. Registering this one unlocks the remote signing workflow for your account.",
                    ),
            )
            .child(div().mt(px(28.)).flex().flex_col().gap(px(10.)).children(VALUE_POINTS.iter().map(|p| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .text_color(t.muted_foreground)
                    .child(icon("check").size(px(14.)).text_color(t.primary))
                    .child(*p)
            })));

        let form = div()
            .mt_6()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .child(
                        div()
                            .mb_2()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.muted_foreground)
                            .child("Machine name"),
                    )
                    .child(self.name_input.clone()),
            )
            .when(!self.machine_id.is_empty(), |d| {
                d.child(Self::read_only_row("Machine ID", "read-only", self.machine_id.clone()))
            })
            .when(!self.machine_type.is_empty(), |d| {
                d.child(Self::read_only_row("Machine type", "detected", self.machine_type.clone()))
            })
            .child(
                div().mt_2().child(
                    button("register-machine", if self.loading { "Registering…" } else { "Register machine" })
                        .large()
                        .full_width()
                        .loading(self.loading)
                        .disabled(name_empty)
                        .when(!self.loading, |b| b.icon_right("arrow-right"))
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                ),
            )
            .child(
                div()
                    .text_center()
                    .text_size(px(12.))
                    .text_color(alpha(t.muted_foreground, 0.7))
                    .child("You can rename this later"),
            );

        let aside = div()
            .w(px(460.))
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .border_l_1()
            .border_color(t.border)
            .bg(alpha(t.card, 0.4))
            .px_10()
            .py_10()
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.foreground)
                    .child("Machine details"),
            )
            .children(self.error.clone().map(|e| div().mt_5().child(alert(Tone::Destructive, e))))
            .child(form);

        div().size_full().flex().flex_row().overflow_hidden().child(intro).child(aside)
    }
}
