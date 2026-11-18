//! `pages/local/RecoveryFlow.tsx` — wallet recovery wizard. Same slot
//! model as creation, entered from the other end:
//!
//!   • From a descriptor — paste it (or open a wallet backup file). The
//!     backend parses it into spending paths and the keys it names; the
//!     user then attaches private keys to whichever slots they hold
//!     (seed, hardware wallet, key file, xprv). Every attached key must
//!     reproduce the descriptor's key exactly. Slots left alone stay
//!     watch-only and sign elsewhere.
//!   • From keys — no descriptor. Pick the policy the wallet used and
//!     fill every slot; this rebuilds the descriptor. Works for
//!     single-key wallets from a seed, or multi-key wallets when every
//!     cosigner's key is at hand.

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Subscription, Window};

use super::setup::api::{
    self, CreateFromSlots, CreateWatchOnly, DescriptorAnalysis, SlotKey, SlotSource, WirePolicy,
    WireSlot,
};
use super::setup::files::open_text_file;
use super::setup::finish::{DoneEvent, DoneStep, SecureEvent, SecureStep};
use super::setup::policy::{
    describe_timelock, layout_for_analysis, layout_for_policy, policy_problem, policy_summary,
    to_wire_policy, to_wire_slots, PolicyDraft, SlotLayout,
};
use super::setup::policy_picker::{PolicyPicker, PolicyPickerEvent};
use super::setup::slot_board::{slot_board, SlotBoardHost};
use super::setup::slot_editor::{Mode, SlotEditor, SlotEditorEvent};
use super::setup::ui::{
    alert_text, field_label, fingerprint, network_picker, option_card, primary_button,
    secondary_button, seg, segmented, step_header, AlertTone,
};
use crate::router::Page;
use crate::shell::navigate;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{badge, InputEvent, TextInput, Tone};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Source,
    Keys,
    Secure,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Descriptor,
    Policy,
}

const STEP_LABELS: [&str; 4] = ["Source", "Keys", "Secure", "Done"];

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

fn format_thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub struct RecoveryFlowPage {
    step: Step,
    route: Route,
    name: Entity<TextInput>,
    network: &'static str,
    descriptor: Entity<TextInput>,
    analysis: Option<DescriptorAnalysis>,
    reading: bool,
    read_error: Option<SharedString>,
    picker: Entity<PolicyPicker>,
    layout: Option<SlotLayout>,
    editor: Option<Entity<SlotEditor>>,
    watch_only: bool,
    secure: Entity<SecureStep>,
    done: Option<Entity<DoneStep>>,
    submitting: bool,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
    _editor_sub: Option<Subscription>,
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
        let picker = cx.new(|cx| PolicyPicker::new(PolicyDraft::Singlesig, window, cx));
        let secure = cx.new(|cx| SecureStep::new("Recover wallet", window, cx));
        let subs = vec![
            cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&descriptor, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Changed) {
                    this.analysis = None;
                    cx.notify();
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
            step: Step::Source,
            route: Route::Descriptor,
            name,
            network: "regtest",
            descriptor,
            analysis: None,
            reading: false,
            read_error: None,
            picker,
            layout: None,
            editor: None,
            watch_only: false,
            secure,
            done: None,
            submitting: false,
            error: None,
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

    fn attached(&self) -> usize {
        self.layout
            .as_ref()
            .map(|l| l.slots.iter().filter(|s| s.filled.is_some()).count())
            .unwrap_or(0)
    }

    fn hot_attached(&self) -> usize {
        self.layout
            .as_ref()
            .map(|l| {
                l.slots
                    .iter()
                    .filter(|s| s.filled.as_ref().is_some_and(|f| f.secret.is_some()))
                    .count()
            })
            .unwrap_or(0)
    }

    fn keys_ready(&self) -> bool {
        match &self.layout {
            None => false,
            Some(l) => self.route == Route::Descriptor || l.slots.iter().all(|s| s.filled.is_some()),
        }
    }

    fn summary(&self, cx: &gpui::App) -> String {
        match (&self.analysis, self.route) {
            (Some(a), Route::Descriptor) => {
                format!("{} · {}", kind_label(&a.kind), script_label(&a.script))
            }
            _ => policy_summary(&self.policy(cx)),
        }
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
                match res {
                    Ok(a) => this.set_analysis(a, cx),
                    Err(e) => this.read_error = Some(e.into()),
                }
                this.reading = false;
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
                    this.analysis = None;
                    this.read_descriptor(file.contents, cx);
                }
                Ok(None) => {}
                Err(e) => {
                    this.read_error = Some(e.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn source_ready(&self, cx: &gpui::App) -> bool {
        !self.name_text(cx).is_empty()
            && match self.route {
                Route::Descriptor => self.analysis.is_some(),
                Route::Policy => policy_problem(&self.policy(cx)).is_none(),
            }
    }

    fn start_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        match self.route {
            Route::Descriptor => {
                let Some(a) = &self.analysis else {
                    return;
                };
                self.layout = Some(layout_for_analysis(a));
                self.open_editor(None, window, cx);
            }
            Route::Policy => {
                let next = layout_for_policy(&self.policy(cx), self.network);
                let first = next.slots.first().map(|s| s.id.clone());
                self.layout = Some(next);
                self.open_editor(first, window, cx);
            }
        }
        self.watch_only = false;
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
        let editor =
            cx.new(|cx| SlotEditor::new(Mode::Recover, slot, slots, network, wallet_name, window, cx));
        self._editor_sub = Some(cx.subscribe_in(&editor, window, move |this, _, ev: &SlotEditorEvent, window, cx| {
            match ev {
                SlotEditorEvent::Fill(filled) => {
                    let mut next = None;
                    let route = this.route;
                    if let Some(layout) = this.layout.as_mut() {
                        if let Some(s) = layout.slot_mut(&id) {
                            s.filled = Some(filled.clone());
                        }
                        if route == Route::Policy {
                            next = layout
                                .slots
                                .iter()
                                .find(|s| s.id != id && s.filled.is_none())
                                .map(|s| s.id.clone());
                        }
                    }
                    this.open_editor(next, window, cx);
                }
                SlotEditorEvent::Close => this.open_editor(None, window, cx),
            }
        }));
        self.editor = Some(editor);
        cx.notify();
    }

    /// Descriptor route: every slot that has origin info is sent so its
    /// label sticks; only attached ones carry a source other than xpub.
    fn descriptor_slots(layout: &SlotLayout) -> Vec<WireSlot> {
        layout
            .slots
            .iter()
            .filter_map(|s| {
                if let Some(f) = &s.filled {
                    return Some(WireSlot {
                        label: s.label.clone(),
                        source: f.source,
                        key: f.key.clone(),
                        secret: f.secret.clone(),
                    });
                }
                let e = s.expected.as_ref()?;
                let (fp, path) = (e.fingerprint.clone()?, e.derivation_path.clone()?);
                Some(WireSlot {
                    label: s.label.clone(),
                    source: SlotSource::Xpub,
                    key: SlotKey {
                        fingerprint: fp,
                        derivation_path: path,
                        xpub: e.xpub.clone(),
                    },
                    secret: None,
                })
            })
            .collect()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let hot = self.hot_attached() > 0;
        let passphrase = self.secure.read(cx).passphrase(cx);
        let name = self.name_text(cx);
        let network = self.network.to_string();

        let create = match (&self.analysis, self.route) {
            (Some(a), Route::Descriptor) if self.watch_only && !hot => api::create_watch_only(
                cx,
                CreateWatchOnly {
                    name: name.clone(),
                    network,
                    external_descriptor: a.external_descriptor.clone(),
                    internal_descriptor: a.internal_descriptor.clone(),
                    fingerprints: a.slots.iter().filter_map(|s| s.fingerprint.clone()).collect(),
                },
            ),
            _ => {
                let (policy, slots) = match (&self.analysis, self.route) {
                    (Some(a), Route::Descriptor) => (
                        WirePolicy::Descriptor {
                            external_descriptor: a.external_descriptor.clone(),
                            internal_descriptor: a.internal_descriptor.clone(),
                        },
                        Self::descriptor_slots(&layout),
                    ),
                    _ => {
                        let slots = if self.route == Route::Descriptor {
                            Self::descriptor_slots(&layout)
                        } else {
                            match to_wire_slots(&layout.slots) {
                                Ok(s) => s,
                                Err(e) => {
                                    self.error = Some(e.into());
                                    cx.notify();
                                    return;
                                }
                            }
                        };
                        (to_wire_policy(&self.policy(cx), &layout), slots)
                    }
                };
                api::create_from_slots(
                    cx,
                    CreateFromSlots {
                        name: name.clone(),
                        network,
                        policy,
                        slots,
                        encrypt_passphrase: hot.then(|| passphrase.clone()),
                    },
                )
            }
        };

        self.submitting = true;
        self.error = None;
        self.secure.update(cx, |s, cx| s.set_submitting(true, cx));
        cx.notify();
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
                        let done = cx.new(|_| DoneStep::new("Wallet recovered.", id.clone(), name, multi_key));
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

    fn render_analysis(&self, a: &DescriptorAnalysis) -> impl IntoElement {
        let t = theme();
        let n = a.slots.len();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_md()
            .border_1()
            .border_color(alpha(t.success, 0.4))
            .bg(alpha(t.success, 0.04))
            .p_4()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child(kind_label(&a.kind)),
                    )
                    .child(badge(script_label(&a.script), Tone::Outline))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(format!("{n} key{} detected", if n == 1 { "" } else { "s" })),
                    ),
            )
            .child(div().flex().flex_col().gap_2().children(a.paths.iter().map(|p| {
                div()
                    .flex()
                    .flex_col()
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .text_color(t.foreground)
                            .child(p.label.clone())
                            .children(p.timelock_blocks.map(|b| {
                                div().text_color(t.muted_foreground).child(format!(
                                    "· {} blocks ({})",
                                    format_thousands(b),
                                    describe_timelock(b)
                                ))
                            })),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_x_3()
                            .gap_y(px(2.))
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .children(p.slot_ids.iter().filter_map(|id| {
                                let s = a.slots.iter().find(|s| &s.id == id)?;
                                Some(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_1()
                                        .child(match &s.fingerprint {
                                            Some(fp) => fingerprint(fp).into_any_element(),
                                            None => "no origin".into_any_element(),
                                        })
                                        .children(s.derivation_path.clone().map(|dp| {
                                            div().font_family(FONT_MONO).child(dp)
                                        })),
                                )
                            })),
                    )
            })))
            .children(a.warnings.iter().map(|w| alert_text(AlertTone::Warning, w.clone())))
    }

    fn render_source_step(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let ready = self.source_ready(cx);
        let route = self.route;
        let this = cx.entity().downgrade();
        let has_text = !self.descriptor.read(cx).text().trim().is_empty();

        let routes = [
            (
                Route::Descriptor,
                "route-descriptor",
                "I have the descriptor",
                "Paste it or open a wallet backup file. Needed for any multi-key wallet.",
            ),
            (
                Route::Policy,
                "route-policy",
                "I only have keys",
                "Pick the policy and add each key. Typical for a single-key seed.",
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap(px(28.))
            .child(div().flex().flex_row().gap_2().children(routes.into_iter().map(|(r, id, title, hint)| {
                option_card(id, route == r)
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .p_3()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.route = r;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.))
                            .line_height(px(17.))
                            .text_color(t.muted_foreground)
                            .child(hint),
                    )
            })))
            .child(network_picker(self.network, move |id, _, cx| {
                this.update(cx, |this, cx| {
                    this.network = id;
                    this.analysis = None;
                    cx.notify();
                })
                .ok();
            }))
            .child(match route {
                Route::Descriptor => div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_end()
                            .justify_between()
                            .gap_3()
                            .child(div().flex_1().min_w_0().child(field_label(
                                "Descriptor",
                                Some("A multipath descriptor (…/<0;1>/*), a receive + change pair on separate lines, or a SigVault wallet backup.".into()),
                            )))
                            .child(
                                secondary_button("open-file", "Open file…")
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| this.open_file(cx))),
                            ),
                    )
                    .child(self.descriptor.clone())
                    .when(self.analysis.is_none(), |d| {
                        d.child(
                            secondary_button("read-descriptor", if self.reading { "Reading…" } else { "Read descriptor" })
                                .full_width()
                                .disabled(!has_text || self.reading)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let text = this.descriptor.read(cx).text().to_string();
                                    this.read_descriptor(text, cx);
                                })),
                        )
                    })
                    .children(self.read_error.clone().map(|e| alert_text(AlertTone::Error, e)))
                    .children(self.analysis.as_ref().map(|a| self.render_analysis(a)))
                    .into_any_element(),
                Route::Policy => div()
                    .flex()
                    .flex_col()
                    .child(field_label(
                        "Spending policy",
                        Some("Choose the setup the wallet was created with. Default derivation paths apply; you can change a slot's path when adding its key.".into()),
                    ))
                    .child(div().mt_3().child(self.picker.clone()))
                    .into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(field_label("Wallet name", None))
                    .child(self.name.clone()),
            )
            .child(
                primary_button(
                    "source-next",
                    if route == Route::Descriptor { "Continue to keys" } else { "Add keys" },
                )
                .full_width()
                .icon_right("arrow-right")
                .disabled(!ready)
                .on_click(cx.listener(|this, _, w, cx| {
                    if this.source_ready(cx) {
                        this.start_keys(w, cx);
                    }
                })),
            )
    }

    fn render_keys_step(&self, layout: &SlotLayout, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.route;
        let attached = self.attached();
        let hot_attached = self.hot_attached();
        let keys_ready = self.keys_ready();
        let missing = layout.slots.iter().filter(|s| s.filled.is_none()).count();
        let this = cx.entity().downgrade();

        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(alert_text(
                AlertTone::Info,
                if route == Route::Descriptor {
                    "Attach private keys for the slots you hold. Each one is checked against the descriptor. Slots you skip stay watch-only and can still sign later with a hardware wallet or an exported PSBT."
                } else {
                    "Add every key the wallet used. Seeds and key files give you signing keys here; hardware wallets and pasted public keys sign elsewhere."
                },
            ))
            .child(slot_board(layout, self.editor.as_ref(), cx))
            .when(route == Route::Descriptor && hot_attached == 0, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(field_label(
                            "How will this wallet be used?",
                            Some(
                                if attached == 0 { "No private keys attached." } else { "Only hardware keys attached." }
                                    .into(),
                            ),
                        ))
                        .child(segmented(
                            "usage",
                            if self.watch_only { "watch" } else { "spend" },
                            vec![seg("spend", "Spend with hardware / PSBT"), seg("watch", "Watch only")],
                            false,
                            move |v, _, cx| {
                                this.update(cx, |this, cx| {
                                    this.watch_only = v.as_ref() == "watch";
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
                    .child(secondary_button("keys-back", "← Back").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.step = Step::Source;
                            cx.notify();
                        },
                    )))
                    .child(
                        div().flex_1().child(
                            primary_button(
                                "keys-continue",
                                if keys_ready { "Continue".to_string() } else { format!("{missing} key(s) to go") },
                            )
                            .full_width()
                            .when(keys_ready, |b| b.icon_right("arrow-right"))
                            .disabled(!keys_ready)
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.open_editor(None, w, cx);
                                let Some(layout) = this.layout.clone() else {
                                    return;
                                };
                                let summary = this.summary(cx);
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

impl SlotBoardHost for RecoveryFlowPage {
    fn edit_slot(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_editor(Some(slot_id), window, cx);
    }

    fn clear_slot(&mut self, slot_id: String, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.layout.as_mut().and_then(|l| l.slot_mut(&slot_id)) {
            s.filled = None;
        }
        cx.notify();
    }
}

impl Render for RecoveryFlowPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.name_text(cx);
        let title = if self.step == Step::Source || name.is_empty() {
            "Restore a wallet".to_string()
        } else {
            name
        };
        let current = match self.step {
            Step::Source => 0,
            Step::Keys => 1,
            Step::Secure => 2,
            Step::Done => 3,
        };
        let body = match self.step {
            Step::Source => Some(self.render_source_step(cx).into_any_element()),
            Step::Keys => self
                .layout
                .clone()
                .map(|l| self.render_keys_step(&l, cx).into_any_element()),
            Step::Secure => self.layout.as_ref().map(|_| self.secure.clone().into_any_element()),
            Step::Done => self.done.clone().map(|d| d.into_any_element()),
        };

        div().size_full().child(
            div().id("recover-wallet-scroll").size_full().overflow_y_scroll().child(
                div().w_full().flex().flex_col().items_center().child(
                    div()
                        .w_full()
                        .max_w(px(640.))
                        .px_6()
                        .py_10()
                        .flex()
                        .flex_col()
                        .child(step_header(
                            "Recover wallet",
                            title,
                            &STEP_LABELS,
                            current,
                            |_, _, cx| navigate(Page::LocalWallets, cx),
                        ))
                        .children(self.error.clone().map(|e| alert_text(AlertTone::Error, e).mt_6()))
                        .child(div().mt_8().children(body)),
                ),
            ),
        )
    }
}
