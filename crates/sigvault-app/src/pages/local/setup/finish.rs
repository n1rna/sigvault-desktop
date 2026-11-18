//! `setup/Finish.tsx` — the last two steps both wizards share: set the
//! wallet passphrase and review, then the done screen that pushes a
//! wallet backup file. For any wallet with more than one key, the
//! descriptor is what makes the keys usable, so the backup gets top
//! billing.

use gpui::{
    div, prelude::*, px, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription,
    Window,
};

use super::api;
use super::files::{file_safe, save_text_file};
use super::policy::{hot_slot_count, paths_spendable_from_this_device, SlotLayout};
use super::ui::{
    alert_text, error_line, field_label, fingerprint, primary_button, secondary_button, AlertTone,
};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{checkbox, icon, InputEvent, TextInput};

pub enum SecureEvent {
    Back,
    Submit,
}

pub struct SecureStep {
    layout: SlotLayout,
    summary: String,
    name: String,
    network: String,
    submitting: bool,
    submit_label: &'static str,
    passphrase: Entity<TextInput>,
    confirm: Entity<TextInput>,
    accept_exposure: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<SecureEvent> for SecureStep {}

impl SecureStep {
    pub fn new(submit_label: &'static str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let passphrase = cx.new(|cx| TextInput::new(window, cx).masked().placeholder("Passphrase"));
        let confirm =
            cx.new(|cx| TextInput::new(window, cx).masked().placeholder("Confirm passphrase"));
        let on_input = |this: &mut Self, _: Entity<TextInput>, ev: &InputEvent, cx: &mut Context<Self>| {
            match ev {
                InputEvent::Changed => cx.notify(),
                InputEvent::Submit => {
                    if this.ready(cx) && !this.submitting {
                        cx.emit(SecureEvent::Submit);
                    }
                }
            }
        };
        let subs = vec![cx.subscribe(&passphrase, on_input), cx.subscribe(&confirm, on_input)];
        Self {
            layout: SlotLayout::default(),
            summary: String::new(),
            name: String::new(),
            network: String::new(),
            submitting: false,
            submit_label,
            passphrase,
            confirm,
            accept_exposure: false,
            _subs: subs,
        }
    }

    /// Refresh the review data before showing the step.
    pub fn configure(
        &mut self,
        layout: SlotLayout,
        summary: String,
        name: String,
        network: String,
        cx: &mut Context<Self>,
    ) {
        self.layout = layout;
        self.summary = summary;
        self.name = name;
        self.network = network;
        cx.notify();
    }

    pub fn set_submitting(&mut self, submitting: bool, cx: &mut Context<Self>) {
        self.submitting = submitting;
        cx.notify();
    }

    pub fn passphrase(&self, cx: &gpui::App) -> String {
        self.passphrase.read(cx).text().to_string()
    }

    /// `secureReady`.
    pub fn ready(&self, cx: &gpui::App) -> bool {
        let pass = self.passphrase(cx);
        let confirm = self.confirm.read(cx).text();
        let needs_passphrase = hot_slot_count(&self.layout.slots) > 0;
        let pass_ok = !needs_passphrase || (pass.chars().count() >= 8 && pass == confirm);
        let exposure_ok =
            paths_spendable_from_this_device(&self.layout).is_empty() || self.accept_exposure;
        pass_ok && exposure_ok
    }
}

impl Render for SecureStep {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let hot = hot_slot_count(&self.layout.slots);
        let exposed = paths_spendable_from_this_device(&self.layout);
        let ready = self.ready(cx);
        let pass = self.passphrase(cx);
        let confirm = self.confirm.read(cx).text().to_string();
        let submitting = self.submitting;

        let review = div()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(t.border)
            .bg(alpha(t.card, 0.4))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(t.border)
                    .px_4()
                    .py_3()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child(self.name.clone()),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(format!("{} · {}", self.summary, self.network)),
                    ),
            )
            .children(self.layout.slots.iter().enumerate().map(|(i, s)| {
                let fp = s
                    .filled
                    .as_ref()
                    .map(|f| f.key.fingerprint.clone())
                    .or_else(|| s.expected.as_ref().and_then(|e| e.fingerprint.clone()));
                let is_hot = s.filled.as_ref().is_some_and(|f| f.secret.is_some());
                let tag = if is_hot {
                    "key on this computer"
                } else if s.filled.as_ref().is_some_and(|f| f.source == api::SlotSource::Hardware) {
                    "hardware"
                } else {
                    "signs elsewhere"
                };
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_4()
                    .py_2()
                    .text_size(px(12.))
                    .when(i > 0, |d| d.border_t_1().border_color(t.border))
                    .child(div().text_color(t.foreground).child(s.label.clone()))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .children(fp.map(|f| fingerprint(&f)))
                            .child(
                                div()
                                    .rounded(px(3.))
                                    .px(px(6.))
                                    .py(px(2.))
                                    .map(|d| {
                                        if is_hot {
                                            d.bg(alpha(t.warning, 0.15)).text_color(t.foreground)
                                        } else {
                                            d.bg(t.muted).text_color(t.muted_foreground)
                                        }
                                    })
                                    .child(tag),
                            ),
                    )
            }));

        let passphrase_section = if hot > 0 {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(field_label(
                    "Wallet passphrase",
                    Some(format!(
                        "Encrypts the {} stored on this computer. You'll type it to unlock the wallet and to sign. At least 8 characters. It is not part of your seed backups, and losing it only locks this copy.",
                        if hot == 1 { "private key".to_string() } else { format!("{hot} private keys") }
                    ).into()),
                ))
                .child(self.passphrase.clone())
                .child(self.confirm.clone())
                .when(!confirm.is_empty() && pass != confirm, |d| {
                    d.child(error_line("Passphrases do not match"))
                })
                .into_any_element()
        } else {
            alert_text(
                AlertTone::Info,
                "No private keys will be stored on this computer, so the wallet doesn't need a passphrase. Signing happens on hardware wallets or by exporting PSBTs to the other key holders.",
            )
            .into_any_element()
        };

        let this = cx.entity().downgrade();
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(review)
            .child(passphrase_section)
            .when(!exposed.is_empty(), |d| {
                let titles = exposed
                    .iter()
                    .map(|p| p.title.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(", ");
                d.child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(alpha(t.warning, 0.4))
                        .bg(alpha(t.warning, 0.06))
                        .px_4()
                        .py_3()
                        .child(checkbox(
                            "accept-exposure",
                            self.accept_exposure,
                            format!("I understand that keys on this computer can spend without any other signer ({titles})."),
                            move |v, _, cx| {
                                this.update(cx, |this, cx| {
                                    this.accept_exposure = v;
                                    cx.notify();
                                })
                                .ok();
                            },
                        )),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        secondary_button("secure-back", "← Back")
                            .disabled(submitting)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(SecureEvent::Back))),
                    )
                    .child(
                        div().flex_1().child(
                            primary_button("secure-submit", self.submit_label)
                                .full_width()
                                .loading(submitting)
                                .disabled(!ready)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.ready(cx) && !this.submitting {
                                        cx.emit(SecureEvent::Submit);
                                    }
                                })),
                        ),
                    ),
            )
    }
}

pub enum DoneEvent {
    Open,
}

pub struct DoneStep {
    title: &'static str,
    wallet_id: String,
    wallet_name: String,
    multi_key: bool,
    saved: Option<String>,
    error: Option<SharedString>,
}

impl EventEmitter<DoneEvent> for DoneStep {}

impl DoneStep {
    pub fn new(title: &'static str, wallet_id: String, wallet_name: String, multi_key: bool) -> Self {
        Self {
            title,
            wallet_id,
            wallet_name,
            multi_key,
            saved: None,
            error: None,
        }
    }

    fn save_backup(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        let export = api::export_wallet_backup(cx, self.wallet_id.clone());
        let name = format!("{}.wallet-backup.json", file_safe(&self.wallet_name));
        cx.spawn(async move |this, cx| {
            let res = match export.await {
                Ok(contents) => save_text_file(cx, contents, name).await,
                Err(e) => Err(e),
            };
            this.update(cx, |this, cx| {
                match res {
                    Ok(Some(p)) => this.saved = Some(p),
                    Ok(None) => {}
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for DoneStep {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let multi = self.multi_key;
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt_4()
                    .child(
                        div()
                            .size(px(56.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(alpha(t.success, 0.4))
                            .bg(alpha(t.success, 0.08))
                            .child(icon("check").size(px(24.)).text_color(t.success)),
                    )
                    .child(
                        div()
                            .mt_5()
                            .text_size(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(self.title),
                    )
                    .child(
                        div()
                            .mt_2()
                            .max_w(px(384.))
                            .text_center()
                            .text_size(px(13.))
                            .line_height(px(21.))
                            .text_color(t.muted_foreground)
                            .child("The wallet is unlocked and syncing. Balance and history fill in as the sync completes."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .rounded_md()
                    .border_1()
                    .px_4()
                    .py_4()
                    .map(|d| {
                        if multi {
                            d.border_color(alpha(t.warning, 0.4)).bg(alpha(t.warning, 0.06))
                        } else {
                            d.border_color(t.border).bg(alpha(t.card, 0.4))
                        }
                    })
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(if multi { "Save the wallet backup now" } else { "Wallet backup" }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(19.))
                            .text_color(t.muted_foreground)
                            .child(if multi {
                                "Seed phrases alone can't restore a multi-key wallet. You also need the descriptor, which lists every key and the spending rules. This file holds it (no private keys), so give a copy to every key holder."
                            } else {
                                "The descriptor and key details, without private keys. Restoring from the seed works too; this file makes it faster and records the exact setup."
                            }),
                    )
                    .child(
                        secondary_button(
                            "save-wallet-backup",
                            if self.saved.is_some() { "Save another copy" } else { "Save wallet backup (.json)" },
                        )
                        .full_width()
                        .on_click(cx.listener(|this, _, _, cx| this.save_backup(cx))),
                    )
                    .children(self.saved.clone().map(|p| {
                        div()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(format!("✓ {p}"))
                    }))
                    .children(self.error.clone().map(|e| alert_text(AlertTone::Error, e))),
            )
            .child(
                primary_button("open-wallet", "Open wallet")
                    .full_width()
                    .icon_right("arrow-right")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DoneEvent::Open))),
            )
    }
}
