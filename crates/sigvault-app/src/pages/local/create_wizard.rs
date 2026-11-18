//! `pages/local/CreateWalletWizard.tsx` — wallet creation wizard.
//!
//!   1. Policy — name, network, and the spending policy (single key,
//!      multisig, timelocked recovery, or a custom shape).
//!   2. Keys   — one slot per key the policy needs. Each slot is filled
//!      by generating a key here (with a confirmed backup), reading a
//!      hardware wallet, typing a seed, opening a key file, or pasting
//!      a key.
//!   3. Secure — wallet passphrase for any keys stored on this computer,
//!      plus a review.
//!   4. Done   — prompt to save the wallet backup (descriptor + key info).
//!
//! Everything is persisted in one `cmd_local_create_from_slots` call; the
//! backend re-derives every private key and checks it against its slot.

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Subscription, Window};

use super::setup::api::{self, CreateFromSlots};
use super::setup::finish::{DoneEvent, DoneStep, SecureEvent, SecureStep};
use super::setup::policy::{
    layout_for_policy, policy_problem, policy_summary, to_wire_policy, to_wire_slots, PolicyDraft,
    SlotLayout,
};
use super::setup::policy_picker::{PolicyPicker, PolicyPickerEvent};
use super::setup::slot_board::{slot_board, SlotBoardHost};
use super::setup::slot_editor::{Mode, SlotEditor, SlotEditorEvent};
use super::setup::ui::{
    alert_text, field_label, network_picker, primary_button, secondary_button, setup_alert,
    step_header, AlertTone,
};
use crate::router::Page;
use crate::shell::navigate;
use crate::theme::theme;
use crate::ui::{InputEvent, TextInput};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Policy,
    Keys,
    Secure,
    Done,
}

const STEP_LABELS: [&str; 4] = ["Policy", "Keys", "Secure", "Done"];

pub struct CreateWalletWizardPage {
    step: Step,
    name: Entity<TextInput>,
    network: &'static str,
    picker: Entity<PolicyPicker>,
    layout: Option<SlotLayout>,
    /// Which policy+network the current layout was built for, so going
    /// back and forth without changes keeps the keys already added.
    layout_for: Option<(PolicyDraft, &'static str)>,
    editor: Option<Entity<SlotEditor>>,
    secure: Entity<SecureStep>,
    done: Option<Entity<DoneStep>>,
    submitting: bool,
    error: Option<SharedString>,
    confirm_discard: bool,
    _subs: Vec<Subscription>,
    _editor_sub: Option<Subscription>,
}

impl CreateWalletWizardPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| TextInput::new(window, cx).placeholder("Savings"));
        let picker = cx.new(|cx| PolicyPicker::new(PolicyDraft::Singlesig, window, cx));
        let secure = cx.new(|cx| SecureStep::new("Create wallet", window, cx));
        let subs = vec![
            cx.subscribe_in(&name, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Changed => cx.notify(),
                InputEvent::Submit => {
                    if this.policy_ready(cx) {
                        this.go_to_keys(window, cx);
                    }
                }
            }),
            cx.subscribe(&picker, |_, _, _: &PolicyPickerEvent, cx| cx.notify()),
            cx.subscribe(&secure, |this, _, ev: &SecureEvent, cx| match ev {
                SecureEvent::Back => {
                    this.step = Step::Keys;
                    cx.notify();
                }
                SecureEvent::Submit => this.submit(cx),
            }),
        ];
        Self {
            step: Step::Policy,
            name,
            network: "regtest",
            picker,
            layout: None,
            layout_for: None,
            editor: None,
            secure,
            done: None,
            submitting: false,
            error: None,
            confirm_discard: false,
            _subs: subs,
            _editor_sub: None,
        }
    }

    fn policy(&self, cx: &gpui::App) -> PolicyDraft {
        self.picker.read(cx).value().clone()
    }

    fn name_text(&self, cx: &gpui::App) -> String {
        self.name.read(cx).text().trim().to_string()
    }

    fn policy_key(&self, cx: &gpui::App) -> (PolicyDraft, &'static str) {
        (self.policy(cx), self.network)
    }

    fn filled_count(&self) -> usize {
        self.layout
            .as_ref()
            .map(|l| l.slots.iter().filter(|s| s.filled.is_some()).count())
            .unwrap_or(0)
    }

    fn has_keys(&self) -> bool {
        self.filled_count() > 0
    }

    fn policy_ready(&self, cx: &gpui::App) -> bool {
        !self.name_text(cx).is_empty() && policy_problem(&self.policy(cx)).is_none()
    }

    fn go_to_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.policy_key(cx);
        if self.layout.is_none() || self.layout_for.as_ref() != Some(&key) {
            let mut next = layout_for_policy(&key.0, key.1);
            // Keep keys for slots that survive the change (same id, same path).
            if let Some(prev) = &self.layout {
                for s in next.slots.iter_mut() {
                    if let Some(p) = prev.slot(&s.id) {
                        if p.filled.is_some() && p.derivation_path == s.derivation_path {
                            s.filled = p.filled.clone();
                        }
                    }
                }
            }
            let first_empty = next.slots.iter().find(|s| s.filled.is_none()).map(|s| s.id.clone());
            self.layout = Some(next);
            self.layout_for = Some(key);
            self.open_editor(first_empty, window, cx);
        }
        self.error = None;
        self.step = Step::Keys;
        cx.notify();
    }

    fn open_editor(&mut self, slot_id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        self._editor_sub = None;
        let (Some(id), Some(layout)) = (slot_id, self.layout.as_ref()) else {
            cx.notify();
            return;
        };
        let Some(slot) = layout.slot(&id).cloned() else {
            return;
        };
        let slots = layout.slots.clone();
        let network = self.network.to_string();
        let wallet_name = self.name.read(cx).text().to_string();
        let editor = cx.new(|cx| SlotEditor::new(Mode::Create, slot, slots, network, wallet_name, window, cx));
        self._editor_sub = Some(cx.subscribe_in(&editor, window, move |this, _, ev: &SlotEditorEvent, window, cx| {
            match ev {
                SlotEditorEvent::Fill(filled) => {
                    let mut next_empty = None;
                    if let Some(layout) = this.layout.as_mut() {
                        if let Some(s) = layout.slot_mut(&id) {
                            s.derivation_path = filled.key.derivation_path.clone();
                            s.filled = Some(filled.clone());
                        }
                        // Move straight on to the next empty slot.
                        next_empty = layout
                            .slots
                            .iter()
                            .find(|s| s.id != id && s.filled.is_none())
                            .map(|s| s.id.clone());
                    }
                    this.open_editor(next_empty, window, cx);
                }
                SlotEditorEvent::Close => this.open_editor(None, window, cx),
            }
        }));
        self.editor = Some(editor);
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let slots = match to_wire_slots(&layout.slots) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.into());
                cx.notify();
                return;
            }
        };
        self.submitting = true;
        self.error = None;
        self.secure.update(cx, |s, cx| s.set_submitting(true, cx));
        cx.notify();

        let hot = layout.slots.iter().any(|s| s.filled.as_ref().is_some_and(|f| f.secret.is_some()));
        let passphrase = self.secure.read(cx).passphrase(cx);
        let name = self.name_text(cx);
        let policy = to_wire_policy(&self.policy(cx), &layout);
        let create = api::create_from_slots(
            cx,
            CreateFromSlots {
                name: name.clone(),
                network: self.network.to_string(),
                policy,
                slots,
                encrypt_passphrase: hot.then(|| passphrase.clone()),
            },
        );
        let multi_key = layout.slots.len() > 1;
        cx.spawn(async move |this, cx| {
            let res: Result<String, String> = async {
                let id = create.await?;
                let unlock = cx
                    .update(|cx| api::unlock_wallet(cx, id.clone(), if hot { passphrase } else { String::new() }))
                    .map_err(|e| e.to_string())?;
                unlock.await?;
                Ok(id)
            }
            .await;
            this.update(cx, |this, cx| {
                this.submitting = false;
                this.secure.update(cx, |s, cx| s.set_submitting(false, cx));
                match res {
                    Ok(id) => {
                        let done = cx.new(|_| DoneStep::new("Wallet created.", id.clone(), name, multi_key));
                        this._subs.push(cx.subscribe(&done, move |_, _, ev: &DoneEvent, cx| match ev {
                            DoneEvent::Open => navigate(Page::LocalWallet(id.clone()), cx),
                        }));
                        this.done = Some(done);
                        this.step = Step::Done;
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.has_keys() && self.step != Step::Done {
            self.confirm_discard = true;
            cx.notify();
            return;
        }
        navigate(Page::LocalWallets, cx);
    }

    fn render_policy_step(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let ready = self.policy_ready(cx);
        let keys_will_reset =
            self.has_keys() && self.layout_for.as_ref() != Some(&self.policy_key(cx));
        let this = cx.entity().downgrade();

        div()
            .flex()
            .flex_col()
            .gap(px(28.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(field_label("Wallet name", None))
                    .child(self.name.clone()),
            )
            .child(network_picker(self.network, move |id, _, cx| {
                this.update(cx, |this, cx| {
                    this.network = id;
                    cx.notify();
                })
                .ok();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(field_label(
                        "Spending policy",
                        Some("Who has to sign, and when. You'll add the keys next.".into()),
                    ))
                    .child(div().mt_3().child(self.picker.clone())),
            )
            .when(keys_will_reset, |d| {
                d.child(alert_text(
                    AlertTone::Warning,
                    "Changing the policy or network resets the key slots. Keys you already added (that don't fit the new layout) are dropped; make sure generated keys are backed up.",
                ))
            })
            .child(
                primary_button("continue-keys", "Continue to keys")
                    .full_width()
                    .icon_right("arrow-right")
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, w, cx| {
                        if this.policy_ready(cx) {
                            this.go_to_keys(w, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .child("Restoring a wallet you already have?")
                    .child(
                        div()
                            .id("recover-instead")
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx))
                            .child("Recover it instead"),
                    ),
            )
    }

    fn render_keys_step(&self, layout: &SlotLayout, cx: &mut Context<Self>) -> impl IntoElement {
        let filled = self.filled_count();
        let total = layout.slots.len();
        let all_filled = filled == total;
        let left = total - filled;
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(slot_board(layout, self.editor.as_ref(), cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(secondary_button("keys-back", "← Back").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.step = Step::Policy;
                            cx.notify();
                        },
                    )))
                    .child(
                        div().flex_1().child(
                            primary_button(
                                "keys-continue",
                                if all_filled {
                                    "Continue".to_string()
                                } else {
                                    format!("{left} key{} to go", if left == 1 { "" } else { "s" })
                                },
                            )
                            .full_width()
                            .when(all_filled, |b| b.icon_right("arrow-right"))
                            .disabled(!all_filled)
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.open_editor(None, w, cx);
                                let Some(layout) = this.layout.clone() else {
                                    return;
                                };
                                let summary = policy_summary(&this.policy(cx));
                                let name = this.name_text(cx);
                                let network = this.network.to_string();
                                this.secure.update(cx, |s, cx| s.configure(layout, summary, name, network, cx));
                                this.step = Step::Secure;
                                cx.notify();
                            })),
                        ),
                    ),
            )
    }
}

impl SlotBoardHost for CreateWalletWizardPage {
    fn edit_slot(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_editor(Some(slot_id), window, cx);
    }

    fn clear_slot(&mut self, slot_id: String, _window: &mut Window, cx: &mut Context<Self>) {
        let fresh = layout_for_policy(&self.policy(cx), self.network)
            .slot(&slot_id)
            .map(|s| s.derivation_path.clone())
            .unwrap_or_default();
        if let Some(s) = self.layout.as_mut().and_then(|l| l.slot_mut(&slot_id)) {
            s.filled = None;
            s.derivation_path = fresh;
        }
        cx.notify();
    }
}

impl Render for CreateWalletWizardPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.name_text(cx);
        let title = if self.step == Step::Policy || name.is_empty() {
            "Create a wallet".to_string()
        } else {
            name
        };
        let current = match self.step {
            Step::Policy => 0,
            Step::Keys => 1,
            Step::Secure => 2,
            Step::Done => 3,
        };

        let body = match self.step {
            Step::Policy => Some(self.render_policy_step(cx).into_any_element()),
            Step::Keys => self
                .layout
                .clone()
                .map(|l| self.render_keys_step(&l, cx).into_any_element()),
            Step::Secure => self.layout.as_ref().map(|_| self.secure.clone().into_any_element()),
            Step::Done => self.done.clone().map(|d| d.into_any_element()),
        };

        div().size_full().child(
            div().id("create-wallet-scroll").size_full().overflow_y_scroll().child(
                div().w_full().flex().flex_col().items_center().child(
                    div()
                        .w_full()
                        .max_w(px(640.))
                        .px_6()
                        .py_10()
                        .flex()
                        .flex_col()
                        .child(step_header(
                            "New wallet",
                            title,
                            &STEP_LABELS,
                            current,
                            cx.listener(|this, _, _, cx| this.cancel(cx)),
                        ))
                        .when(self.confirm_discard, |d| {
                            d.child(
                                setup_alert(
                                    AlertTone::Warning,
                                    Some("Discard this wallet setup?".into()),
                                    Some(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child("Keys generated here are lost unless you've backed them up.")
                                            .child(
                                                div()
                                                    .mt_3()
                                                    .flex()
                                                    .flex_row()
                                                    .gap_2()
                                                    .child(
                                                        secondary_button("discard", "Discard")
                                                            .small()
                                                            .on_click(|_, _, cx| navigate(Page::LocalWallets, cx)),
                                                    )
                                                    .child(
                                                        secondary_button("keep-editing", "Keep editing")
                                                            .small()
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.confirm_discard = false;
                                                                cx.notify();
                                                            })),
                                                    ),
                                            )
                                            .into_any_element(),
                                    ),
                                )
                                .mt_6(),
                            )
                        })
                        .children(self.error.clone().map(|e| alert_text(AlertTone::Error, e).mt_6()))
                        .child(div().mt_8().children(body)),
                ),
            ),
        )
    }
}
