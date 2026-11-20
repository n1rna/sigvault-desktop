//! Restore wallet. Wallets are restored from their *wallet backup* (or the
//! descriptor inside it); private keys come from the key library:
//!
//!   1. Backup — open a `.wallet-backup.json` or paste a descriptor; the
//!      backend parses it into spending paths and the keys it names.
//!   2. Keys   — every key is matched against the library by fingerprint +
//!      xpub. Unmatched keys can be added to the library in place (seed,
//!      key file, hardware wallet…) or left watch-only.
//!   3. Review — summary, slot → key mapping and, when matched keys hold
//!      private keys here, the wallet passphrase.
//!   4. Done   — save a fresh wallet backup.
//!
//! Having only keys (no backup) isn't a separate route any more: restoring
//! keys means importing them into the library, then creating the wallet
//! with the same policy through "New wallet". The backup step explains this.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FontWeight, SharedString, Subscription,
    Window,
};

use super::key_picker::fetch_secrets;
use super::keys::add_flow::{AddKeyEvent, AddKeyFlow};
use super::keys::unlock::{KeychainUnlock, UnlockEvent};
use super::setup::api::{
    self, CreateFromSlots, CreateWatchOnly, DescriptorAnalysis, SlotKey, SlotSource, WirePolicy,
    WireSlot,
};
use super::setup::files::open_text_file;
use super::setup::finish::{
    network_label, DoneStep, ReviewData, ReviewEvent, ReviewRow, ReviewStep,
};
use super::setup::policy::{
    describe_timelock, layout_for_analysis, paths_spendable_by, SlotLayout,
};
use super::setup::slot_board::{
    meta_line, slot_board, BoardKey, BoardMode, SlotBoardHost, SlotState,
};
use super::setup::ui::network_options;
use crate::keychain::{keychain, KeyRecord, KeychainEvent};
use crate::router::Page;
use crate::shell::navigate;
use crate::theme::{theme, FONT_MONO};
use crate::ui::{
    alert, badge, button, card, divider, field, footer_bar, icon, modal, page_frame, section,
    stepper, InputEvent, TextInput, Tone, FORM_WIDTH,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Backup,
    Keys,
    Review,
    Done,
}

const STEP_LABELS: [&str; 4] = ["Backup", "Keys", "Review", "Done"];

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "singlesig" => "Single key",
        "multisig" => "Multisig",
        "timelocked" => "Timelocked recovery",
        "recovery_only" => "Recovery-only vault",
        _ => "Custom script",
    }
}

fn script_label(script: &str) -> &'static str {
    match script {
        "wpkh" => "P2WPKH",
        "sh_wpkh" => "P2SH-P2WPKH",
        "wsh" => "P2WSH",
        "tr" => "Taproot",
        _ => "Other",
    }
}

pub struct RecoveryFlowPage {
    step: Step,
    name: Entity<TextInput>,
    network: &'static str,
    descriptor: Entity<TextInput>,
    file_name: Option<String>,
    analysis: Option<DescriptorAnalysis>,
    reading: bool,
    read_error: Option<SharedString>,
    layout: Option<SlotLayout>,
    /// Slots whose matched library key the user chose not to use.
    skipped: HashSet<String>,
    /// Slot the add-key modal is filling, and the flow itself.
    adding: Option<(String, Entity<AddKeyFlow>)>,
    add_notice: Option<(Tone, SharedString)>,
    unlock: Option<Entity<KeychainUnlock>>,
    review: Entity<ReviewStep>,
    done: Option<(String, Entity<DoneStep>)>,
    submitting: bool,
    error: Option<SharedString>,
    dev_stage: Option<String>,
    _subs: Vec<Subscription>,
    _modal_sub: Option<Subscription>,
}

impl RecoveryFlowPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| TextInput::new(window, cx).placeholder("Recovered savings"));
        let descriptor = cx.new(|cx| {
            TextInput::new(window, cx)
                .mono()
                .multiline(5)
                .placeholder("wsh(sortedmulti(2,[a1b2c3d4/48'/1'/0'/2']tpub…/<0;1>/*,…))#checksum")
        });
        let review = cx.new(|cx| ReviewStep::new(window, cx));
        let subs = vec![
            cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&descriptor, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Changed) {
                    this.analysis = None;
                    this.file_name = None;
                    this.read_error = None;
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &review,
                window,
                |this, _, ev: &ReviewEvent, window, cx| match ev {
                    ReviewEvent::Submit => this.submit(window, cx),
                },
            ),
            cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()),
        ];
        let mut this = Self {
            step: Step::Backup,
            name,
            network: "regtest",
            descriptor,
            file_name: None,
            analysis: None,
            reading: false,
            read_error: None,
            layout: None,
            skipped: HashSet::new(),
            adding: None,
            add_notice: None,
            unlock: None,
            review,
            done: None,
            submitting: false,
            error: None,
            dev_stage: None,
            _subs: subs,
            _modal_sub: None,
        };
        // Debug builds: SIGVAULT_DEV_DESCRIPTOR=<text> SIGVAULT_DEV_WIZARD=keys|review
        if cfg!(debug_assertions) {
            if let Ok(text) = std::env::var("SIGVAULT_DEV_DESCRIPTOR") {
                this.dev_stage = std::env::var("SIGVAULT_DEV_WIZARD").ok();
                this.descriptor
                    .update(cx, |d, cx| d.set_text(text.clone(), cx));
                this.read_descriptor(text, cx);
            }
        }
        this
    }

    fn name_text(&self, cx: &App) -> String {
        self.name.read(cx).text().trim().to_string()
    }

    fn backup_ready(&self, cx: &App) -> bool {
        self.analysis.is_some() && !self.name_text(cx).is_empty()
    }

    /// The library key matching a slot's descriptor key, on this network.
    fn matched<'a>(
        &self,
        slot_id: &str,
        kc: &'a crate::keychain::Keychain,
    ) -> Option<&'a KeyRecord> {
        let e = self.layout.as_ref()?.slot(slot_id)?.expected.as_ref()?;
        kc.find_match(e.fingerprint.as_deref(), &e.xpub)
            .filter(|k| k.network == self.network)
    }

    /// Slot → library key actually used (matched and not skipped).
    fn used_keys(&self, cx: &App) -> HashMap<String, KeyRecord> {
        let kc = keychain(cx).read(cx);
        let Some(layout) = &self.layout else {
            return HashMap::new();
        };
        layout
            .slots
            .iter()
            .filter(|s| !self.skipped.contains(&s.id))
            .filter_map(|s| Some((s.id.clone(), self.matched(&s.id, kc)?.clone())))
            .collect()
    }

    fn slot_states(&self, cx: &App) -> HashMap<String, SlotState> {
        let kc = keychain(cx).read(cx);
        let Some(layout) = &self.layout else {
            return HashMap::new();
        };
        layout
            .slots
            .iter()
            .map(|s| {
                let m = self.matched(&s.id, kc).map(BoardKey::from_record);
                let state = match m {
                    Some(k) if !self.skipped.contains(&s.id) => SlotState::Assigned(k),
                    Some(k) => SlotState::WatchOnly { skipped: Some(k) },
                    None => SlotState::WatchOnly { skipped: None },
                };
                (s.id.clone(), state)
            })
            .collect()
    }

    /// No used key can sign anywhere → create a watch-only wallet.
    fn watch_only(&self, cx: &App) -> bool {
        !self
            .used_keys(cx)
            .values()
            .any(|k| k.is_hot() || k.source == "hardware")
    }

    fn set_analysis(&mut self, a: DescriptorAnalysis, cx: &mut Context<Self>) {
        if let Some(hint) = &a.name_hint {
            if self.name_text(cx).is_empty() {
                let hint = hint.clone();
                self.name.update(cx, |n, cx| n.set_text(hint, cx));
            }
        }
        self.analysis = Some(a);
        cx.notify();
    }

    fn read_descriptor(&mut self, text: String, cx: &mut Context<Self>) {
        self.reading = true;
        self.read_error = None;
        cx.notify();
        let task = api::analyze_descriptor(cx, self.network.to_string(), text);
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.reading = false;
                match res {
                    Ok(a) => {
                        this.set_analysis(a, cx);
                        this.run_dev_stage(cx);
                    }
                    Err(e) => this.read_error = Some(super::widgets::plain_error(&e).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_file(&mut self, cx: &mut Context<Self>) {
        self.read_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = open_text_file(cx, "Open wallet backup or descriptor").await;
            this.update(cx, |this, cx| match res {
                Ok(Some(file)) => {
                    let trimmed = file.contents.trim().to_string();
                    this.descriptor.update(cx, |d, cx| d.set_text(trimmed, cx));
                    this.file_name = Some(file.name);
                    this.analysis = None;
                    this.read_descriptor(file.contents, cx);
                }
                Ok(None) => {}
                Err(e) => {
                    this.read_error = Some(super::widgets::plain_error(&e).into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn continue_from_backup(&mut self, cx: &mut Context<Self>) {
        if !self.backup_ready(cx) {
            return;
        }
        let Some(a) = &self.analysis else { return };
        let next = layout_for_analysis(a);
        if self.layout.as_ref() != Some(&next) {
            self.layout = Some(next);
            self.skipped.clear();
        }
        self.add_notice = None;
        self.error = None;
        self.step = Step::Keys;
        cx.notify();
    }

    fn continue_from_keys(&mut self, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let used = self.used_keys(cx);
        let rows = layout
            .slots
            .iter()
            .map(|s| {
                let e = s.expected.as_ref();
                ReviewRow {
                    slot_label: s.label.clone(),
                    path_title: layout
                        .paths
                        .iter()
                        .find(|p| p.id == s.path_id)
                        .map(|p| p.title.clone())
                        .unwrap_or_default(),
                    key: used.get(&s.id).map(BoardKey::from_record),
                    fingerprint: e.and_then(|e| e.fingerprint.clone()).unwrap_or_default(),
                    derivation_path: e
                        .and_then(|e| e.derivation_path.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        let is_hot = |id: &str| used.get(id).is_some_and(|k| k.is_hot());
        let hot = layout.slots.iter().filter(|s| is_hot(&s.id)).count();
        let exposed = paths_spendable_by(&layout, |s| is_hot(&s.id))
            .into_iter()
            .map(|p| p.title)
            .collect();
        let a = self.analysis.as_ref();
        let data = ReviewData {
            name: self.name_text(cx),
            network: self.network.to_string(),
            summary: a
                .map(|a| format!("{} · {}", kind_label(&a.kind), script_label(&a.script)))
                .unwrap_or_default(),
            rows,
            hot,
            exposed,
            watch_only: self.watch_only(cx),
        };
        self.review.update(cx, |r, cx| r.configure(data, cx));
        self.step = Step::Review;
        cx.notify();
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.adding = None;
        self.unlock = None;
        self._modal_sub = None;
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.submitting || !self.review.read(cx).ready(cx) {
            return;
        }
        let (Some(layout), Some(a)) = (self.layout.clone(), self.analysis.clone()) else {
            return;
        };
        let used = self.used_keys(cx);
        let hot: Vec<(String, String)> = used
            .iter()
            .filter(|(_, k)| k.is_hot())
            .map(|(s, k)| (s.clone(), k.id.clone()))
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

        let name = self.name_text(cx);
        let network = self.network.to_string();
        let watch_only = self.watch_only(cx);
        let has_hot = !hot.is_empty();
        let passphrase = self.review.read(cx).passphrase(cx);

        // Every slot with origin info is sent so its label sticks; the key
        // is the descriptor's own, the source and secret come from the
        // matched library key.
        let mut index = HashMap::new();
        let mut slots = Vec::new();
        for s in &layout.slots {
            let Some(e) = s.expected.as_ref() else {
                continue;
            };
            let (Some(fp), Some(path)) = (e.fingerprint.clone(), e.derivation_path.clone()) else {
                continue;
            };
            let k = used.get(&s.id);
            index.insert(s.id.clone(), slots.len());
            slots.push(WireSlot {
                label: k
                    .map(|k| k.label.clone())
                    .unwrap_or_else(|| s.label.clone()),
                source: k
                    .and_then(|k| SlotSource::from_key_source(&k.source))
                    .unwrap_or(SlotSource::Xpub),
                key: SlotKey {
                    fingerprint: fp,
                    derivation_path: path,
                    xpub: e.xpub.clone(),
                },
                secret: None,
            });
        }

        self.submitting = true;
        self.error = None;
        self.review.update(cx, |r, cx| r.set_submitting(true, cx));
        cx.notify();

        let secrets = fetch_secrets(cx, hot);
        let multi_key = layout.slots.len() > 1;
        cx.spawn(async move |this, cx| {
            let res: Result<String, String> = async {
                let create = if watch_only {
                    cx.update(|cx| {
                        api::create_watch_only(
                            cx,
                            CreateWatchOnly {
                                name: name.clone(),
                                network,
                                external_descriptor: a.external_descriptor.clone(),
                                internal_descriptor: a.internal_descriptor.clone(),
                                fingerprints: a
                                    .slots
                                    .iter()
                                    .filter_map(|s| s.fingerprint.clone())
                                    .collect(),
                            },
                        )
                    })
                    .map_err(|e| e.to_string())?
                } else {
                    for (slot, secret) in secrets.await? {
                        if let Some(i) = index.get(&slot) {
                            slots[*i].secret = Some(secret);
                        }
                    }
                    cx.update(|cx| {
                        api::create_from_slots(
                            cx,
                            CreateFromSlots {
                                name: name.clone(),
                                network,
                                policy: WirePolicy::Descriptor {
                                    external_descriptor: a.external_descriptor.clone(),
                                    internal_descriptor: a.internal_descriptor.clone(),
                                },
                                slots,
                                encrypt_passphrase: has_hot.then(|| passphrase.clone()),
                            },
                        )
                    })
                    .map_err(|e| e.to_string())?
                };
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
                            .new(|_| DoneStep::new("Wallet restored", id.clone(), name, multi_key));
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

    fn run_dev_stage(&mut self, cx: &mut Context<Self>) {
        let Some(stage) = self.dev_stage.take() else {
            return;
        };
        if self.name_text(cx).is_empty() {
            self.name
                .update(cx, |n, cx| n.set_text("Family vault".to_string(), cx));
        }
        self.continue_from_backup(cx);
        if stage == "review" {
            self.continue_from_keys(cx);
        }
    }

    // ── rendering ──

    fn render_analysis(&self, a: &DescriptorAnalysis) -> impl IntoElement {
        let t = theme();
        let n = a.slots.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_4()
                .h(px(44.))
                .child(icon("circle-check").size(px(15.)).text_color(t.success))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.foreground)
                        .child(kind_label(&a.kind)),
                )
                .child(badge(script_label(&a.script), Tone::Outline))
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(format!("{n} key{}", if n == 1 { "" } else { "s" })),
                )
                .into_any_element(),
        );
        for p in &a.paths {
            rows.push(divider().into_any_element());
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px_4()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .text_size(px(13.))
                            .text_color(t.foreground)
                            .child(p.label.clone())
                            .children(p.timelock_blocks.map(|b| {
                                div()
                                    .text_color(t.muted_foreground)
                                    .child(format!("· after {}", describe_timelock(b)))
                            })),
                    )
                    .child(div().flex().flex_row().flex_wrap().gap_x_4().children(
                        p.slot_ids.iter().filter_map(|id| {
                            let s = a.slots.iter().find(|s| &s.id == id)?;
                            Some(meta_line(
                                s.fingerprint.as_deref().unwrap_or(""),
                                s.derivation_path.as_deref().unwrap_or(""),
                            ))
                        }),
                    ))
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(card().children(rows))
            .children(a.warnings.iter().map(|w| alert(Tone::Warning, w.clone())))
    }

    fn render_backup_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let has_text = !self.descriptor.read(cx).text().trim().is_empty();
        let this = cx.entity().downgrade();

        let keys_only = card()
            .px_4()
            .py_4()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_3()
                    .child(icon("key").size(px(16.)).mt(px(1.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child("Only have a recovery phrase or keys?"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(19.))
                                    .text_color(t.muted_foreground)
                                    .child("Restoring keys means importing them into your Key library: add each one there (recovery phrase, key file, extended key, hardware wallet or xpub), then create the wallet again with New wallet using the same policy. For a single-key wallet that's all you need. A multi-key wallet also needs its descriptor, so use the backup file if anyone has a copy."),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .pl(px(28.))
                    .child(
                        button("goto-add-key", "Add key to library")
                            .outline()
                            .small()
                            .icon("plus")
                            .on_click(|_, _, cx| navigate(Page::LocalKeyAdd, cx)),
                    )
                    .child(
                        button("goto-new-wallet", "New wallet")
                            .ghost()
                            .small()
                            .icon_right("arrow-right")
                            .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                    ),
            );

        div()
            .max_w(FORM_WIDTH)
            .flex()
            .flex_col()
            .gap_6()
            .child(field(
                "Network",
                network_options(self.network, move |id, _, cx| {
                    this.update(cx, |this, cx| {
                        if this.network != id {
                            this.network = id;
                            this.analysis = None;
                        }
                        cx.notify();
                    })
                    .ok();
                }),
                None,
            ))
            .child(
                section("Wallet backup")
                    .description("Open the .wallet-backup.json saved when the wallet was created, or paste its descriptor.")
                    .action(
                        button("open-file", "Open file…")
                            .outline()
                            .small()
                            .icon("file")
                            .on_click(cx.listener(|this, _, _, cx| this.open_file(cx))),
                    )
                    .child(self.descriptor.clone())
                    .children(self.file_name.clone().map(|f| {
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(f)
                    }))
                    .when(self.analysis.is_none(), |d| {
                        d.child(
                            div().child(
                                button("read-descriptor", "Read backup")
                                    .outline()
                                    .loading(self.reading)
                                    .disabled(!has_text || self.reading)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let text = this.descriptor.read(cx).text().to_string();
                                        this.read_descriptor(text, cx);
                                    })),
                            ),
                        )
                    })
                    .children(self.read_error.clone().map(|e| alert(Tone::Destructive, e)))
                    .children(self.analysis.as_ref().map(|a| self.render_analysis(a))),
            )
            .when(self.analysis.is_some(), |d| d.child(field("Wallet name", self.name.clone(), None)))
            .child(keys_only)
            .into_any_element()
    }

    fn render_keys_step(&self, layout: &SlotLayout, cx: &mut Context<Self>) -> AnyElement {
        let states = self.slot_states(cx);
        let total = layout.slots.len();
        let found = states
            .values()
            .filter(|s| !matches!(s, SlotState::WatchOnly { skipped: None }))
            .count();
        let net = network_label(self.network);
        let summary = if found == total {
            (
                Tone::Success,
                format!(
                    "All {total} key{} found in your key library.",
                    if total == 1 { "" } else { "s" }
                ),
            )
        } else if found == 0 {
            (
                Tone::Secondary,
                format!("None of this wallet's keys are in your {net} key library. Add the ones you hold, or continue to restore it watch-only."),
            )
        } else {
            (
                Tone::Secondary,
                format!("{found} of {total} keys found in your key library. Add the others you hold, or leave them watch-only: they can still sign elsewhere (hardware wallet or another key holder)."),
            )
        };
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(alert(summary.0, summary.1))
            .children(self.add_notice.clone().map(|(tone, msg)| alert(tone, msg)))
            .child(slot_board(layout, &states, BoardMode::Restore, cx))
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
        let (_, flow) = self.adding.as_ref()?;
        Some(
            modal("add-key", 640.)
                .title(
                    "Add a key",
                    Some("It's saved to your key library and used for this slot.".into()),
                )
                .on_dismiss(dismiss)
                .child(flow.clone())
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
            Step::Backup => (
                vec![],
                vec![
                    cancel,
                    button("wizard-continue", "Continue")
                        .icon_right("arrow-right")
                        .disabled(!self.backup_ready(cx))
                        .on_click(cx.listener(|this, _, _, cx| this.continue_from_backup(cx)))
                        .into_any_element(),
                ],
            ),
            Step::Keys => (
                vec![back(Step::Backup)],
                vec![
                    cancel,
                    button("wizard-continue", "Continue")
                        .icon_right("arrow-right")
                        .on_click(cx.listener(|this, _, _, cx| this.continue_from_keys(cx)))
                        .into_any_element(),
                ],
            ),
            Step::Review => (
                vec![back(Step::Keys)],
                vec![
                    cancel,
                    button("wizard-create", "Restore wallet")
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
}

impl SlotBoardHost for RecoveryFlowPage {
    fn pick_key(&mut self, slot_id: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.skipped.remove(&slot_id);
        cx.notify();
    }

    fn clear_slot(&mut self, slot_id: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.skipped.insert(slot_id);
        cx.notify();
    }

    fn add_key(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let network = self.network.to_string();
        let flow = cx.new(|cx| AddKeyFlow::new(Some(network), window, cx));
        let sid = slot_id.clone();
        self._modal_sub = Some(cx.subscribe(&flow, move |this, _, ev: &AddKeyEvent, cx| {
            if let AddKeyEvent::Added(id) = ev {
                let slot_label = this.layout.as_ref().and_then(|l| l.slot(&sid)).map(|s| s.label.clone());
                let kc = keychain(cx).read(cx);
                let added = kc.get(id).map(|k| k.label.clone()).unwrap_or_default();
                let fits = this.matched(&sid, kc).is_some_and(|k| &k.id == id);
                this.skipped.remove(&sid);
                this.add_notice = Some(if fits {
                    (Tone::Success, format!("“{added}” matches {}.", slot_label.unwrap_or_default()).into())
                } else {
                    let fp = this
                        .layout
                        .as_ref()
                        .and_then(|l| l.slot(&sid))
                        .and_then(|s| s.expected.as_ref())
                        .and_then(|e| e.fingerprint.clone())
                        .unwrap_or_else(|| "?".into());
                    (
                        Tone::Warning,
                        format!(
                            "“{added}” was added to your library, but it isn't the key {} needs (fingerprint {fp}, same derivation path). Check the recovery phrase, passphrase and path.",
                            slot_label.unwrap_or_default()
                        )
                        .into(),
                    )
                });
            }
            this.close_modal(cx);
        }));
        self.adding = Some((slot_id, flow));
        cx.notify();
    }
}

impl Render for RecoveryFlowPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = match self.step {
            Step::Backup => 0,
            Step::Keys => 1,
            Step::Review => 2,
            // Past the last step: every step shows a check.
            Step::Done => STEP_LABELS.len(),
        };
        let name = self.name_text(cx);
        let title = if self.step == Step::Backup || name.is_empty() {
            "Restore wallet".to_string()
        } else {
            name
        };
        let subtitle = match self.step {
            Step::Backup => "Bring back a wallet from its backup file or descriptor.",
            Step::Keys => "Keys are matched against your key library.",
            Step::Review => "Check the setup, then restore the wallet.",
            Step::Done => "The wallet is ready.",
        };
        let body = match self.step {
            Step::Backup => Some(self.render_backup_step(cx)),
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
                page_frame("restore-wallet")
                    .crumb("Wallets", Some(Page::LocalWallets))
                    .crumb("Restore wallet", None)
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
