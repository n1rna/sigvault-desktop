//! New wallet wizard. Wallets are built from keys in the key library
//! (`crate::keychain`), so creating keys and creating wallets are separate:
//!
//!   1. Policy — name, network, and the spending policy (single key,
//!      multisig, timelocked recovery, or a custom shape).
//!   2. Keys   — one slot per key the policy needs; each is filled with a
//!      library key through the key picker (which can also add a new key).
//!   3. Review — summary, slot → key mapping, and a wallet passphrase when
//!      any chosen key has its private key on this computer (the wallet
//!      encrypts its own copy of those secrets).
//!   4. Done   — prompt to save the wallet backup (descriptor + key info).
//!
//! Everything is persisted in one `cmd_local_create_from_slots` call; the
//! backend re-derives every private key and checks it against its slot.

use std::collections::{HashMap, HashSet};

use gpui::{div, prelude::*, AnyElement, App, Context, Entity, SharedString, Subscription, Window};

use super::key_picker::{fetch_secrets, key_identity, KeyPicker, KeyPickerEvent};
use super::keys::unlock::{KeychainUnlock, UnlockEvent};
use super::setup::api::{self, CreateFromSlots, SlotKey, SlotSource, WireSlot};
use super::setup::finish::{
    network_label, DoneStep, ReviewData, ReviewEvent, ReviewRow, ReviewStep,
};
use super::setup::policy::{
    layout_for_policy, paths_spendable_by, policy_problem, policy_summary, to_wire_policy,
    PolicyDraft, Slot, SlotLayout,
};
use super::setup::policy_picker::{PolicyPicker, PolicyPickerEvent};
use super::setup::slot_board::{slot_board, BoardKey, BoardMode, SlotBoardHost, SlotState};
use super::setup::ui::network_options;
use crate::keychain::{keychain, KeychainEvent};
use crate::router::Page;
use crate::shell::navigate;
use crate::theme::theme;
use crate::ui::{
    alert, button, field, footer_bar, modal, page_frame, section, stepper, InputEvent, TextInput,
    Tone, FORM_WIDTH,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Policy,
    Keys,
    Review,
    Done,
}

const STEP_LABELS: [&str; 4] = ["Policy", "Keys", "Review", "Done"];

pub struct CreateWalletWizardPage {
    step: Step,
    name: Entity<TextInput>,
    network: &'static str,
    picker: Entity<PolicyPicker>,
    layout: Option<SlotLayout>,
    /// Which policy+network the current layout was built for, so going
    /// back and forth without changes keeps the keys already chosen.
    layout_for: Option<(PolicyDraft, &'static str)>,
    /// Slot id → key-library id.
    assigned: HashMap<String, String>,
    key_picker: Option<(String, Entity<KeyPicker>)>,
    unlock: Option<Entity<KeychainUnlock>>,
    review: Entity<ReviewStep>,
    done: Option<(String, Entity<DoneStep>)>,
    submitting: bool,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
    _modal_sub: Option<Subscription>,
}

impl CreateWalletWizardPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dev = dev_preset();
        let name = cx.new(|cx| TextInput::new(window, cx).placeholder("Savings"));
        let initial = if dev.is_some() {
            PolicyDraft::Multisig { threshold: 2, n: 3 }
        } else {
            PolicyDraft::Singlesig
        };
        let picker = cx.new(|cx| PolicyPicker::new(initial, window, cx));
        let review = cx.new(|cx| ReviewStep::new(window, cx));
        let subs = vec![
            cx.subscribe_in(
                &name,
                window,
                |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::Changed => cx.notify(),
                    InputEvent::Submit => this.continue_from_policy(window, cx),
                },
            ),
            cx.subscribe(&picker, |_, _, _: &PolicyPickerEvent, cx| cx.notify()),
            cx.subscribe_in(
                &review,
                window,
                |this, _, ev: &ReviewEvent, window, cx| match ev {
                    ReviewEvent::Submit => this.submit(window, cx),
                },
            ),
            cx.subscribe(&keychain(cx), |this, _, ev: &KeychainEvent, cx| {
                if matches!(ev, KeychainEvent::Changed) {
                    // Drop choices whose key was deleted from the library.
                    let kc = keychain(cx).read(cx);
                    this.assigned.retain(|_, id| kc.get(id).is_some());
                }
                cx.notify();
            }),
        ];
        name.read(cx).focus(window);
        let mut this = Self {
            step: Step::Policy,
            name,
            network: "regtest",
            picker,
            layout: None,
            layout_for: None,
            assigned: HashMap::new(),
            key_picker: None,
            unlock: None,
            review,
            done: None,
            submitting: false,
            error: None,
            _subs: subs,
            _modal_sub: None,
        };
        if let Some(stage) = dev {
            this.apply_dev_preset(&stage, window, cx);
        }
        this
    }

    fn policy(&self, cx: &App) -> PolicyDraft {
        self.picker.read(cx).value().clone()
    }

    fn name_text(&self, cx: &App) -> String {
        self.name.read(cx).text().trim().to_string()
    }

    fn policy_key(&self, cx: &App) -> (PolicyDraft, &'static str) {
        (self.policy(cx), self.network)
    }

    fn policy_ready(&self, cx: &App) -> bool {
        !self.name_text(cx).is_empty() && policy_problem(&self.policy(cx)).is_none()
    }

    fn slot_count(&self) -> usize {
        self.layout.as_ref().map(|l| l.slots.len()).unwrap_or(0)
    }

    fn keys_ready(&self) -> bool {
        self.layout
            .as_ref()
            .is_some_and(|l| l.slots.iter().all(|s| self.assigned.contains_key(&s.id)))
    }

    fn is_hot_slot(&self, s: &Slot, cx: &App) -> bool {
        self.assigned
            .get(&s.id)
            .and_then(|id| keychain(cx).read(cx).get(id).map(|k| k.is_hot()))
            .unwrap_or(false)
    }

    fn continue_from_policy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.policy_ready(cx) {
            return;
        }
        let key = self.policy_key(cx);
        if self.layout.is_none() || self.layout_for.as_ref() != Some(&key) {
            let next = layout_for_policy(&key.0, key.1);
            // Keep choices for slots that survive the change, as long as
            // the key is on the (possibly new) network.
            let kc = keychain(cx).read(cx);
            let ids: HashSet<&str> = next.slots.iter().map(|s| s.id.as_str()).collect();
            self.assigned.retain(|slot, key_id| {
                ids.contains(slot.as_str()) && kc.get(key_id).is_some_and(|k| k.network == key.1)
            });
            self.layout = Some(next);
            self.layout_for = Some(key);
        }
        self.error = None;
        self.step = Step::Keys;
        // Single-slot wallets: open the picker straight away.
        if self.slot_count() == 1 && self.assigned.is_empty() {
            if let Some(id) = self.layout.as_ref().map(|l| l.slots[0].id.clone()) {
                self.pick_key(id, window, cx);
            }
        }
        cx.notify();
    }

    fn continue_from_keys(&mut self, cx: &mut Context<Self>) {
        if !self.keys_ready() {
            return;
        }
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let kc = keychain(cx).read(cx);
        let rows = layout
            .slots
            .iter()
            .map(|s| {
                let key = self
                    .assigned
                    .get(&s.id)
                    .and_then(|id| kc.get(id))
                    .map(BoardKey::from_record);
                ReviewRow {
                    slot_label: s.label.clone(),
                    path_title: layout
                        .paths
                        .iter()
                        .find(|p| p.id == s.path_id)
                        .map(|p| p.title.clone())
                        .unwrap_or_default(),
                    fingerprint: key
                        .as_ref()
                        .map(|k| k.fingerprint.clone())
                        .unwrap_or_default(),
                    derivation_path: key
                        .as_ref()
                        .map(|k| k.derivation_path.clone())
                        .unwrap_or_default(),
                    key,
                }
            })
            .collect::<Vec<_>>();
        let hot = layout
            .slots
            .iter()
            .filter(|s| self.is_hot_slot(s, cx))
            .count();
        let exposed = paths_spendable_by(&layout, |s| self.is_hot_slot(s, cx))
            .into_iter()
            .map(|p| p.title)
            .collect();
        let data = ReviewData {
            name: self.name_text(cx),
            network: self.network.to_string(),
            summary: policy_summary(&self.policy(cx)),
            rows,
            hot,
            exposed,
            watch_only: false,
        };
        self.review.update(cx, |r, cx| r.configure(data, cx));
        self.step = Step::Review;
        cx.notify();
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.key_picker = None;
        self.unlock = None;
        self._modal_sub = None;
        cx.notify();
    }

    fn assign(&mut self, slot_id: String, key_id: String, cx: &mut Context<Self>) {
        self.assigned.insert(slot_id.clone(), key_id);
        self.close_modal(cx);
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitting || !self.keys_ready() || !self.review.read(cx).ready(cx) {
            return;
        }
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let hot: Vec<(String, String)> = layout
            .slots
            .iter()
            .filter(|s| self.is_hot_slot(s, cx))
            .map(|s| (s.id.clone(), self.assigned[&s.id].clone()))
            .collect();

        if !hot.is_empty() && !keychain(cx).read(cx).is_unlocked() {
            let unlock = cx.new(|cx| {
                KeychainUnlock::new("to copy the private keys into this wallet", window, cx)
            });
            self._modal_sub =
                Some(
                    cx.subscribe_in(&unlock, window, |this, _, ev: &UnlockEvent, window, cx| {
                        this.close_modal(cx);
                        if matches!(ev, UnlockEvent::Unlocked) {
                            this.submit(window, cx);
                        }
                    }),
                );
            self.unlock = Some(unlock);
            cx.notify();
            return;
        }

        // Public halves straight from the library; secrets fetched below.
        let kc = keychain(cx).read(cx);
        let mut slots = Vec::with_capacity(layout.slots.len());
        for s in &layout.slots {
            let Some(k) = self.assigned.get(&s.id).and_then(|id| kc.get(id)) else {
                self.error = Some(format!("{} has no key", s.label).into());
                cx.notify();
                return;
            };
            slots.push(WireSlot {
                label: k.label.clone(),
                source: SlotSource::from_key_source(&k.source).unwrap_or(SlotSource::Xpub),
                key: SlotKey {
                    fingerprint: k.fingerprint.clone(),
                    derivation_path: k.derivation_path.clone(),
                    xpub: k.xpub.clone(),
                },
                secret: None,
            });
        }
        let index: HashMap<String, usize> = layout
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();

        self.submitting = true;
        self.error = None;
        self.review.update(cx, |r, cx| r.set_submitting(true, cx));
        cx.notify();

        let has_hot = !hot.is_empty();
        let secrets = fetch_secrets(cx, hot);
        let passphrase = self.review.read(cx).passphrase(cx);
        let name = self.name_text(cx);
        let network = self.network.to_string();
        let policy = to_wire_policy(&self.policy(cx), &layout);
        let multi_key = layout.slots.len() > 1;
        cx.spawn(async move |this, cx| {
            let res: Result<String, String> = async {
                for (slot, secret) in secrets.await? {
                    if let Some(i) = index.get(&slot) {
                        slots[*i].secret = Some(secret);
                    }
                }
                let create = cx
                    .update(|cx| {
                        api::create_from_slots(
                            cx,
                            CreateFromSlots {
                                name: name.clone(),
                                network,
                                policy,
                                slots,
                                encrypt_passphrase: has_hot.then(|| passphrase.clone()),
                            },
                        )
                    })
                    .map_err(|e| e.to_string())?;
                let id = create.await?;
                let unlock = cx
                    .update(|cx| {
                        api::unlock_wallet(
                            cx,
                            id.clone(),
                            if has_hot { passphrase } else { String::new() },
                        )
                    })
                    .map_err(|e| e.to_string())?;
                unlock.await?;
                Ok(id)
            }
            .await;
            this.update(cx, |this, cx| {
                this.submitting = false;
                this.review.update(cx, |r, cx| r.set_submitting(false, cx));
                match res {
                    Ok(id) => {
                        crate::state::local_changed(cx);
                        let done = cx
                            .new(|_| DoneStep::new("Wallet created", id.clone(), name, multi_key));
                        // Its save state drives our footer.
                        cx.observe(&done, |_, _, cx| cx.notify()).detach();
                        this.done = Some((id, done));
                        this.step = Step::Done;
                    }
                    Err(e) => this.error = Some(super::widgets::plain_error(&e).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── rendering ──

    fn render_policy_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let keys_will_reset = !self.assigned.is_empty()
            && self
                .layout_for
                .as_ref()
                .is_some_and(|k| k != &self.policy_key(cx));
        let this = cx.entity().downgrade();
        div()
            .max_w(FORM_WIDTH)
            .flex()
            .flex_col()
            .gap_6()
            .child(field("Wallet name", self.name.clone(), None))
            .child(field(
                "Network",
                network_options(self.network, move |id, _, cx| {
                    this.update(cx, |this, cx| {
                        this.network = id;
                        cx.notify();
                    })
                    .ok();
                }),
                Some("Keys are network-specific: you'll choose from your library's keys for this network.".into()),
            ))
            .child(
                section("Spending policy")
                    .description("Who has to sign, and when. You'll choose the keys next.")
                    .child(self.picker.clone()),
            )
            .when(keys_will_reset, |d| {
                d.child(alert(
                    Tone::Warning,
                    "Changing the policy or network clears keys you chose for slots that no longer fit.",
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_size(gpui::px(12.))
                    .text_color(t.muted_foreground)
                    .child("Restoring a wallet you already have?")
                    .child(
                        button("restore-instead", "Restore it instead")
                            .link()
                            .small()
                            .on_click(|_, _, cx| navigate(Page::LocalWalletRecover, cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_keys_step(&self, layout: &SlotLayout, cx: &mut Context<Self>) -> AnyElement {
        let kc = keychain(cx).read(cx);
        let library = kc.keys_for_network(self.network).count();
        let states: HashMap<String, SlotState> = self
            .assigned
            .iter()
            .filter_map(|(slot, id)| {
                Some((
                    slot.clone(),
                    SlotState::Assigned(BoardKey::from_record(kc.get(id)?)),
                ))
            })
            .collect();
        let net = network_label(self.network);
        div()
            .flex()
            .flex_col()
            .gap_6()
            .when(library == 0, |d| {
                d.child(alert(
                    Tone::Secondary,
                    format!(
                        "Your key library has no {net} keys yet. Choose a slot and pick “Add a new key…” to generate or import one, or add keys from the Key library first."
                    ),
                ))
            })
            .child(slot_board(layout, &states, BoardMode::Create, cx))
            .into_any_element()
    }

    fn render_modal(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dismiss = cx.listener(|this, _, _, cx| this.close_modal(cx));
        if let Some(u) = &self.unlock {
            return Some(
                modal("keychain-unlock", 420.)
                    .on_dismiss(dismiss)
                    .child(u.clone())
                    .into_any_element(),
            );
        }
        let (_, picker) = self.key_picker.as_ref()?;
        let w = picker.read(cx).modal_width();
        Some(
            modal("key-picker", w)
                .on_dismiss(dismiss)
                .child(picker.clone())
                .into_any_element(),
        )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let back = |step: Step| {
            button("wizard-back", "Back")
                .outline()
                .icon("arrow-left")
                .disabled(self.submitting)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.step = step;
                    this.error = None;
                    cx.notify();
                }))
                .into_any_element()
        };
        let cancel = button("wizard-cancel", "Cancel")
            .ghost()
            .disabled(self.submitting)
            .on_click(|_, _, cx| navigate(Page::LocalWallets, cx))
            .into_any_element();
        let (left, right) = match self.step {
            Step::Policy => (
                vec![],
                vec![
                    cancel,
                    button("wizard-continue", "Continue")
                        .icon_right("arrow-right")
                        .disabled(!self.policy_ready(cx))
                        .on_click(cx.listener(|this, _, w, cx| this.continue_from_policy(w, cx)))
                        .into_any_element(),
                ],
            ),
            Step::Keys => {
                let left_n = self
                    .layout
                    .as_ref()
                    .map(|l| {
                        l.slots
                            .iter()
                            .filter(|s| !self.assigned.contains_key(&s.id))
                            .count()
                    })
                    .unwrap_or(0);
                (
                    vec![back(Step::Policy)],
                    vec![
                        cancel,
                        button(
                            "wizard-continue",
                            if left_n == 0 {
                                "Continue".to_string()
                            } else {
                                format!("{left_n} key{} to go", if left_n == 1 { "" } else { "s" })
                            },
                        )
                        .when(left_n == 0, |b| b.icon_right("arrow-right"))
                        .disabled(left_n > 0)
                        .on_click(cx.listener(|this, _, _, cx| this.continue_from_keys(cx)))
                        .into_any_element(),
                    ],
                )
            }
            Step::Review => (
                vec![back(Step::Keys)],
                vec![
                    cancel,
                    button("wizard-create", "Create wallet")
                        .loading(self.submitting)
                        .disabled(!self.review.read(cx).ready(cx))
                        .on_click(cx.listener(|this, _, w, cx| this.submit(w, cx)))
                        .into_any_element(),
                ],
            ),
            Step::Done => (
                vec![button("wizard-all-wallets", "All wallets")
                    .ghost()
                    .on_click(|_, _, cx| navigate(Page::LocalWallets, cx))
                    .into_any_element()],
                self.done
                    .as_ref()
                    .map(|(_, d)| d.update(cx, |d, cx| d.footer_actions(cx)))
                    .unwrap_or_default(),
            ),
        };
        footer_bar(left, right).into_any_element()
    }

    // ── dev hook (debug builds): SIGVAULT_DEV_WIZARD=keys|picker|review ──

    fn apply_dev_preset(&mut self, stage: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.name
            .update(cx, |n, cx| n.set_text("Family vault".to_string(), cx));
        self.continue_from_policy(window, cx);
        let ids: Vec<String> = keychain(cx)
            .read(cx)
            .keys_for_network(self.network)
            .map(|k| k.id.clone())
            .collect();
        let slots: Vec<String> = self
            .layout
            .as_ref()
            .map(|l| l.slots.iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default();
        let fill = if stage == "review" { slots.len() } else { 1 };
        for (slot, key) in slots.iter().zip(ids.iter()).take(fill) {
            self.assigned.insert(slot.clone(), key.clone());
        }
        match stage {
            "picker" => {
                if let Some(s) = slots.get(1) {
                    self.pick_key(s.clone(), window, cx);
                }
            }
            "review" => self.continue_from_keys(cx),
            _ => {}
        }
    }
}

fn dev_preset() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var("SIGVAULT_DEV_WIZARD").ok()
    } else {
        None
    }
}

impl SlotBoardHost for CreateWalletWizardPage {
    fn pick_key(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let Some(slot) = layout.slot(&slot_id) else {
            return;
        };
        let kc = keychain(cx).read(cx);
        let used = self
            .assigned
            .iter()
            .filter(|(s, _)| **s != slot_id)
            .filter_map(|(s, id)| {
                let k = kc.get(id)?;
                let label = layout.slot(s).map(|s| s.label.clone()).unwrap_or_default();
                Some((key_identity(&k.fingerprint, &k.xpub), label))
            })
            .collect();
        let current = self.assigned.get(&slot_id).cloned();
        let (network, label) = (self.network.to_string(), slot.label.clone());
        let picker = cx.new(|cx| KeyPicker::new(network, label, used, current, window, cx));
        let sid = slot_id.clone();
        self._modal_sub = Some(
            cx.subscribe(&picker, move |this, _, ev: &KeyPickerEvent, cx| match ev {
                KeyPickerEvent::Picked(id) => this.assign(sid.clone(), id.clone(), cx),
                KeyPickerEvent::Cancelled => this.close_modal(cx),
            }),
        );
        self.key_picker = Some((slot_id, picker));
        cx.notify();
    }

    fn clear_slot(&mut self, slot_id: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.assigned.remove(&slot_id);
        cx.notify();
    }
}

impl Render for CreateWalletWizardPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = match self.step {
            Step::Policy => 0,
            Step::Keys => 1,
            Step::Review => 2,
            // Past the last step: every step shows a check.
            Step::Done => STEP_LABELS.len(),
        };
        let name = self.name_text(cx);
        let title = if self.step == Step::Policy || name.is_empty() {
            "New wallet".to_string()
        } else {
            name
        };
        let subtitle = match self.step {
            Step::Policy => "Name the wallet and choose who has to sign.",
            Step::Keys => "Choose a key from your library for each slot.",
            Step::Review => "Check the setup, then create the wallet.",
            Step::Done => "The wallet is ready.",
        };
        let body = match self.step {
            Step::Policy => Some(self.render_policy_step(cx)),
            Step::Keys => self.layout.clone().map(|l| self.render_keys_step(&l, cx)),
            Step::Review => Some(self.review.clone().into_any_element()),
            Step::Done => self
                .done
                .as_ref()
                .map(|(_, d)| d.clone().into_any_element()),
        };
        let footer = self.render_footer(cx);

        div()
            .size_full()
            .relative()
            .child(
                page_frame("create-wallet")
                    .crumb("Wallets", Some(Page::LocalWallets))
                    .crumb("New wallet", None)
                    .title(title)
                    .subtitle(subtitle)
                    .header_extra(stepper(&STEP_LABELS, current))
                    .footer(footer)
                    .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
                    .children(body),
            )
            .children(self.render_modal(cx))
    }
}
