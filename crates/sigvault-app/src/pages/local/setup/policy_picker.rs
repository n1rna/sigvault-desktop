//! `setup/PolicyPicker.tsx` — pick a wallet policy: a few common presets
//! up front, and an advanced builder for custom M-of-N and
//! timelocked-recovery shapes.

use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FontWeight, SharedString,
    Subscription, Window,
};

use super::api::ScriptKind;
use super::policy::{
    common_presets, default_timelocked, describe_timelock, policy_problem, PolicyDraft,
    RecoveryDraft, Threshold, BLOCKS_PER_DAY, MAX_TIMELOCK_BLOCKS, TIMELOCK_PRESETS,
};
use super::ui::{alert_text, field_label, option_card, seg, segmented, AlertTone, SegOption};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{InputEvent, TextInput};

pub enum PolicyPickerEvent {
    Changed,
}

pub struct PolicyPicker {
    value: PolicyDraft,
    advanced: bool,
    timelock_inputs: Vec<Entity<TextInput>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<PolicyPickerEvent> for PolicyPicker {}

fn preset_id(value: &PolicyDraft) -> Option<&'static str> {
    common_presets().into_iter().find(|p| &p.draft == value).map(|p| p.id)
}

/// Which (threshold, n) pair a ThresholdPicker edits.
#[derive(Clone, Copy)]
enum Target {
    Multisig,
    Primary,
    Recovery(usize),
}

impl PolicyPicker {
    pub fn new(value: PolicyDraft, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let advanced = preset_id(&value).is_none();
        let mut this = Self {
            value,
            advanced,
            timelock_inputs: vec![],
            _subs: vec![],
        };
        this.sync_inputs(window, cx);
        this
    }

    pub fn value(&self) -> &PolicyDraft {
        &self.value
    }

    fn recoveries(&self) -> &[RecoveryDraft] {
        match &self.value {
            PolicyDraft::Timelocked { recoveries, .. } => recoveries,
            _ => &[],
        }
    }

    /// Keep one timelock input per recovery path, showing its value.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let blocks: Vec<u32> = self.recoveries().iter().map(|r| r.timelock_blocks).collect();
        while self.timelock_inputs.len() < blocks.len() {
            let i = self.timelock_inputs.len();
            let input = cx.new(|cx| TextInput::new(window, cx).max_len(5));
            self._subs.push(cx.subscribe(&input, move |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Changed) {
                    this.on_timelock_typed(i, cx);
                }
            }));
            self.timelock_inputs.push(input);
        }
        for (i, b) in blocks.iter().enumerate() {
            let s = b.to_string();
            if self.timelock_inputs[i].read(cx).text() != s {
                self.timelock_inputs[i].update(cx, |inp, cx| inp.set_text(s, cx));
            }
        }
    }

    fn on_timelock_typed(&mut self, i: usize, cx: &mut Context<Self>) {
        let text = self.timelock_inputs[i].read(cx).text().to_string();
        let n = text.trim().parse::<u64>().ok().filter(|n| *n > 0).unwrap_or(1);
        let clamped = n.clamp(1, MAX_TIMELOCK_BLOCKS as u64) as u32;
        if let PolicyDraft::Timelocked { recoveries, .. } = &mut self.value {
            if let Some(r) = recoveries.get_mut(i) {
                r.timelock_blocks = clamped;
            }
        }
        let s = clamped.to_string();
        if text != s {
            self.timelock_inputs[i].update(cx, |inp, cx| inp.set_text(s, cx));
        }
        cx.emit(PolicyPickerEvent::Changed);
        cx.notify();
    }

    fn set(&mut self, value: PolicyDraft, window: &mut Window, cx: &mut Context<Self>) {
        self.value = value;
        self.sync_inputs(window, cx);
        cx.emit(PolicyPickerEvent::Changed);
        cx.notify();
    }

    fn edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut PolicyDraft),
    ) {
        let mut v = self.value.clone();
        f(&mut v);
        self.set(v, window, cx);
    }

    fn set_threshold(&mut self, target: Target, threshold: u32, n: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(window, cx, |v| match (target, v) {
            (Target::Multisig, PolicyDraft::Multisig { threshold: t, n: nn }) => {
                *t = threshold;
                *nn = n;
            }
            (Target::Primary, PolicyDraft::Timelocked { primary, .. }) => {
                *primary = Some(Threshold { threshold, n });
            }
            (Target::Recovery(i), PolicyDraft::Timelocked { recoveries, .. }) => {
                if let Some(r) = recoveries.get_mut(i) {
                    r.threshold = threshold;
                    r.n = n;
                }
            }
            _ => {}
        });
    }

    fn next_timelock(recoveries: &[RecoveryDraft]) -> u32 {
        let used: Vec<u32> = recoveries.iter().map(|r| r.timelock_blocks).collect();
        TIMELOCK_PRESETS
            .iter()
            .map(|(_, b)| *b)
            .find(|b| !used.contains(b))
            .unwrap_or_else(|| {
                let max = used.iter().copied().max().unwrap_or(0);
                MAX_TIMELOCK_BLOCKS.min(max + BLOCKS_PER_DAY * 30)
            })
    }

    // ── render pieces ──

    fn threshold_picker(
        &self,
        id: &str,
        label: &'static str,
        threshold: u32,
        n: u32,
        min_n: u32,
        target: Target,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let clamp_n = move |v: u32| v.clamp(min_n, 15);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .text_size(px(12.))
            .text_color(t.muted_foreground)
            .child(div().w(px(112.)).flex_none().child(label))
            .child(self.stepper(
                SharedString::from(format!("{id}-t")),
                threshold,
                1,
                n,
                cx,
                move |this, v, w, cx| this.set_threshold(target, v, n, w, cx),
            ))
            .child("of")
            .child(self.stepper(
                SharedString::from(format!("{id}-n")),
                n,
                min_n,
                15,
                cx,
                move |this, v, w, cx| {
                    let next = clamp_n(v);
                    this.set_threshold(target, threshold.min(next), next, w, cx)
                },
            ))
            .child("keys")
    }

    fn stepper(
        &self,
        id: SharedString,
        value: u32,
        min: u32,
        max: u32,
        cx: &mut Context<Self>,
        on_change: impl Fn(&mut Self, u32, &mut Window, &mut Context<Self>) + Clone + 'static,
    ) -> impl IntoElement {
        let t = theme();
        let btn = |key: &str, label: &'static str, enabled: bool| {
            div()
                .id(SharedString::from(format!("{id}-{key}")))
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(t.muted_foreground)
                .when(!enabled, |d| d.opacity(0.3))
                .when(enabled, |d| d.cursor_pointer().hover(move |s| s.text_color(t.foreground)))
                .child(label)
        };
        let dec = on_change.clone();
        let inc = on_change;
        div()
            .flex()
            .flex_row()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(t.border)
            .bg(t.background)
            .child(btn("dec", "−", value > min).when(value > min, |d| {
                d.on_click(cx.listener(move |this, _, w, cx| dec(this, value - 1, w, cx)))
            }))
            .child(
                div()
                    .w(px(24.))
                    .text_center()
                    .font_family(FONT_MONO)
                    .text_size(px(12.))
                    .text_color(t.foreground)
                    .child(value.to_string()),
            )
            .child(btn("inc", "+", value < max).when(value < max, |d| {
                d.on_click(cx.listener(move |this, _, w, cx| inc(this, value + 1, w, cx)))
            }))
    }

    fn timelocked_builder(
        &self,
        script: ScriptKind,
        primary: Option<Threshold>,
        recoveries: &[RecoveryDraft],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let this = cx.entity().downgrade();
        let this2 = this.clone();

        let script_section = div()
            .flex()
            .flex_col()
            .child(field_label(
                "Script type",
                Some("Taproot keeps unused paths private and is cheaper. Use SegWit v0 if a signer can't sign Taproot scripts.".into()),
            ))
            .child(div().mt_2().child(segmented(
                "script-type",
                if script == ScriptKind::Taproot { "taproot" } else { "segwit" },
                vec![seg("taproot", "Taproot"), seg("segwit", "SegWit v0 (P2WSH)")],
                false,
                move |v, w, cx| {
                    let script = if v.as_ref() == "segwit" { ScriptKind::Segwit } else { ScriptKind::Taproot };
                    this.update(cx, |this, cx| {
                        this.edit(w, cx, |p| {
                            if let PolicyDraft::Timelocked { script: s, primary, .. } = p {
                                *s = script;
                                if script == ScriptKind::Segwit && primary.is_none() {
                                    *primary = Some(Threshold { threshold: 1, n: 1 });
                                }
                            }
                        })
                    })
                    .ok();
                },
            )));

        let primary_section = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child("Primary path"),
                    )
                    .child(segmented(
                        "primary-kind",
                        if primary.is_some() { "keys" } else { "none" },
                        vec![
                            seg("keys", "Keys"),
                            SegOption {
                                disabled: script == ScriptKind::Segwit,
                                ..seg("none", "Unspendable")
                            },
                        ],
                        true,
                        move |v, w, cx| {
                            let keys = v.as_ref() == "keys";
                            this2
                                .update(cx, |this, cx| {
                                    this.edit(w, cx, |p| {
                                        if let PolicyDraft::Timelocked { primary, .. } = p {
                                            *primary = keys.then_some(Threshold { threshold: 1, n: 1 });
                                        }
                                    })
                                })
                                .ok();
                        },
                    )),
            )
            .child(match primary {
                Some(p) => self
                    .threshold_picker("primary", "Primary signers", p.threshold, p.n, 1, Target::Primary, cx)
                    .into_any_element(),
                None => div()
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(t.muted_foreground)
                    .child("Recovery-only vault: the primary path uses a provably unspendable key, so coins can only move once a recovery timelock has passed. Useful for cold storage and inheritance.")
                    .into_any_element(),
            });

        let count = recoveries.len();
        let rec_sections: Vec<_> = recoveries
            .iter()
            .enumerate()
            .map(|(i, rec)| {
                let rec = *rec;
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .border_t_1()
                    .border_color(t.border)
                    .pt_4()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child(format!("Recovery path {}", i + 1)),
                            )
                            .when(count > 1, |d| {
                                d.child(
                                    div()
                                        .id(("remove-rec", i))
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.muted_foreground)
                                        .cursor_pointer()
                                        .hover(move |s| s.text_color(t.destructive))
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.edit(w, cx, |p| {
                                                if let PolicyDraft::Timelocked { recoveries, .. } = p {
                                                    if i < recoveries.len() {
                                                        recoveries.remove(i);
                                                    }
                                                }
                                            })
                                        }))
                                        .child("Remove"),
                                )
                            }),
                    )
                    .child(div().flex().flex_row().flex_wrap().gap(px(6.)).children(
                        TIMELOCK_PRESETS.iter().map(|(label, blocks)| {
                            let blocks = *blocks;
                            let active = rec.timelock_blocks == blocks;
                            div()
                                .id(SharedString::from(format!("tl-{i}-{blocks}")))
                                .rounded_md()
                                .border_1()
                                .px(px(10.))
                                .py(px(4.))
                                .text_size(px(12.))
                                .cursor_pointer()
                                .map(|d| {
                                    if active {
                                        d.border_color(t.primary)
                                            .bg(alpha(t.primary, 0.08))
                                            .text_color(t.foreground)
                                    } else {
                                        d.border_color(t.border)
                                            .text_color(t.muted_foreground)
                                            .hover(move |s| s.text_color(t.foreground))
                                    }
                                })
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.edit(w, cx, |p| {
                                        if let PolicyDraft::Timelocked { recoveries, .. } = p {
                                            if let Some(r) = recoveries.get_mut(i) {
                                                r.timelock_blocks = blocks;
                                            }
                                        }
                                    })
                                }))
                                .child(*label)
                        }),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .children(self.timelock_inputs.get(i).map(|inp| div().w(px(112.)).child(inp.clone())))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(format!(
                                        "blocks of inactivity · {}",
                                        describe_timelock(rec.timelock_blocks)
                                    )),
                            ),
                    )
                    .child(self.threshold_picker(
                        &format!("rec{i}"),
                        "Recovery signers",
                        rec.threshold,
                        rec.n,
                        1,
                        Target::Recovery(i),
                        cx,
                    ))
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(script_section)
            .child(primary_section)
            .children(rec_sections)
            .when(count < 5, |d| {
                d.child(
                    div()
                        .id("add-recovery")
                        .w_full()
                        .flex()
                        .justify_center()
                        .rounded_md()
                        .border_1()
                        .border_dashed()
                        .border_color(t.border)
                        .py_2()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.muted_foreground)
                        .cursor_pointer()
                        .hover(move |s| s.border_color(alpha(t.primary, 0.6)).text_color(t.foreground))
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.edit(w, cx, |p| {
                                if let PolicyDraft::Timelocked { recoveries, .. } = p {
                                    let tl = Self::next_timelock(recoveries);
                                    recoveries.push(RecoveryDraft {
                                        timelock_blocks: tl,
                                        threshold: 1,
                                        n: 1,
                                    });
                                }
                            })
                        }))
                        .child("+ Add a later recovery path"),
                )
            })
    }

    fn advanced_builder(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let this = cx.entity().downgrade();
        let kind = if matches!(self.value, PolicyDraft::Timelocked { .. }) {
            "timelocked"
        } else {
            "multisig"
        };
        div()
            .flex()
            .flex_col()
            .gap_5()
            .rounded_md()
            .border_1()
            .border_color(t.border)
            .bg(alpha(t.card, 0.4))
            .p_4()
            .child(segmented(
                "advanced-kind",
                kind,
                vec![seg("multisig", "Multisig (M-of-N)"), seg("timelocked", "Timelocked recovery")],
                false,
                move |v, w, cx: &mut App| {
                    let next = if v.as_ref() == "multisig" {
                        PolicyDraft::Multisig { threshold: 2, n: 3 }
                    } else {
                        default_timelocked()
                    };
                    this.update(cx, |this, cx| this.set(next, w, cx)).ok();
                },
            ))
            .map(|d| match &self.value {
                PolicyDraft::Multisig { threshold, n } => d.child(self.threshold_picker(
                    "ms",
                    "Signers",
                    *threshold,
                    *n,
                    2,
                    Target::Multisig,
                    cx,
                )),
                PolicyDraft::Singlesig => d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Pick a shape above."),
                ),
                PolicyDraft::Timelocked {
                    script,
                    primary,
                    recoveries,
                } => d.child(self.timelocked_builder(*script, *primary, &recoveries.clone(), cx)),
            })
    }
}

impl Render for PolicyPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let preset = preset_id(&self.value);
        let advanced = self.advanced;
        let problem = policy_problem(&self.value);

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().flex().flex_row().gap_2().children(common_presets().into_iter().map(|p| {
                let active = !advanced && preset == Some(p.id);
                let draft = p.draft.clone();
                option_card(SharedString::from(format!("preset-{}", p.id)), active)
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .p_3()
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.advanced = false;
                        this.set(draft.clone(), w, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(p.title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .text_color(t.muted_foreground)
                            .child(p.hint),
                    )
                    .child(
                        div()
                            .mt_2()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(alpha(t.muted_foreground, 0.8))
                            .child(p.shape),
                    )
            })))
            .child(
                div()
                    .id("advanced-toggle")
                    .flex()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .rounded_md()
                    .border_1()
                    .px_3()
                    .py(px(10.))
                    .cursor_pointer()
                    .map(|d| {
                        if advanced {
                            d.border_color(t.primary).bg(alpha(t.primary, 0.04))
                        } else {
                            d.border_color(t.border)
                                .hover(move |s| s.border_color(alpha(t.primary, 0.6)))
                        }
                    })
                    .on_click(cx.listener(|this, _, w, cx| {
                        if !this.advanced && this.value == PolicyDraft::Singlesig {
                            this.set(PolicyDraft::Multisig { threshold: 2, n: 3 }, w, cx);
                        }
                        this.advanced = !this.advanced;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_baseline()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child("Advanced"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child("Custom M-of-N, several recovery paths, recovery-only vaults, SegWit v0"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(if advanced { "−" } else { "+" }),
                    ),
            )
            .when(advanced, |d| d.child(self.advanced_builder(cx)))
            .children(problem.map(|p| alert_text(AlertTone::Error, p)))
    }
}
