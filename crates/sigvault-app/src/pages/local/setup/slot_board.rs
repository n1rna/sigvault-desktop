//! Spending paths with their key slots, as used by the create and restore
//! wizards. Each path is a `section` holding a bordered card with one row
//! per slot. Slots are filled with keys from the key library (the host
//! decides which); this module only renders the board and routes clicks
//! back to the host through [`SlotBoardHost`].

use std::collections::HashMap;

use gpui::{div, prelude::*, px, AnyElement, Context, Div, FontWeight, SharedString, Window};

use super::policy::{paths_spendable_by, Slot, SlotLayout};
use crate::keychain::{source_label, KeyRecord};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, divider, icon, section, Tone};

/// The page that owns the board's state.
pub trait SlotBoardHost: Sized + 'static {
    /// Create: choose / change the library key for a slot.
    /// Restore: use the matched library key again after "watch-only".
    fn pick_key(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>);
    /// Create: remove the key. Restore: keep the slot watch-only.
    fn clear_slot(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>);
    /// Restore: add the slot's key to the library (opens the add-key flow).
    fn add_key(&mut self, _slot_id: String, _window: &mut Window, _cx: &mut Context<Self>) {}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BoardMode {
    Create,
    Restore,
}

/// A library key as the board shows it.
#[derive(Clone, Debug)]
pub struct BoardKey {
    pub label: SharedString,
    pub source: String,
    pub hot: bool,
    pub fingerprint: String,
    pub derivation_path: String,
    pub device: Option<String>,
}

impl BoardKey {
    pub fn from_record(k: &KeyRecord) -> Self {
        Self {
            label: k.label.clone().into(),
            source: k.source.clone(),
            hot: k.is_hot(),
            fingerprint: k.fingerprint.clone(),
            derivation_path: k.derivation_path.clone(),
            device: k.device.clone(),
        }
    }
}

/// What a slot currently holds.
#[derive(Clone, Debug)]
pub enum SlotState {
    /// Create: nothing chosen yet.
    Empty,
    /// A library key fills the slot.
    Assigned(BoardKey),
    /// Restore: the descriptor's key with no private material here.
    /// `skipped` is the library key the user chose not to use.
    WatchOnly { skipped: Option<BoardKey> },
}

/// Lucide icon for a key source.
pub fn source_icon(source: &str) -> &'static str {
    match source {
        "generated" => "sparkles",
        "seed" => "scroll-text",
        "xprv" => "key-round",
        "hardware" => "hw-device",
        "xpub" => "eye",
        "key_file" => "file-key",
        _ => "key",
    }
}

/// "Private key here" / "Watch-only" pill.
pub fn custody_badge(hot: bool) -> Div {
    if hot {
        badge("Private key", Tone::Warning)
    } else {
        badge("Public only", Tone::Secondary)
    }
}

fn format_thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn slot_board<H: SlotBoardHost>(
    layout: &SlotLayout,
    states: &HashMap<String, SlotState>,
    mode: BoardMode,
    cx: &mut Context<H>,
) -> Div {
    let t = theme();
    let state_of = |id: &str| states.get(id).cloned().unwrap_or(SlotState::Empty);
    let exposed = paths_spendable_by(
        layout,
        |s: &Slot| matches!(states.get(&s.id), Some(SlotState::Assigned(k)) if k.hot),
    );

    let sections: Vec<AnyElement> = layout
        .paths
        .iter()
        .map(|path| {
            let filled = path
                .slot_ids
                .iter()
                .filter(|id| matches!(states.get(id.as_str()), Some(SlotState::Assigned(_))))
                .count();
            let mut rows: Vec<AnyElement> = Vec::new();
            for (i, id) in path.slot_ids.iter().enumerate() {
                let Some(slot) = layout.slot(id) else {
                    continue;
                };
                if i > 0 {
                    rows.push(divider().into_any_element());
                }
                rows.push(slot_row(&path.id, slot, state_of(id), mode, cx).into_any_element());
            }
            let counter = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .children(
                    path.timelock_blocks
                        .map(|b| badge(format!("{} blocks", format_thousands(b)), Tone::Secondary)),
                )
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(format!("{filled}/{}", path.slot_ids.len())),
                );
            section(path.title.clone())
                .description(path.subtitle.clone())
                .action(counter)
                .child(crate::ui::card().overflow_hidden().children(rows))
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_6()
        .children(sections)
        .when(!exposed.is_empty(), |d| {
            let titles = exposed.iter().map(|p| p.title.to_lowercase()).collect::<Vec<_>>().join(" and ");
            d.child(alert(
                Tone::Warning,
                format!(
                    "This computer alone could spend from the {titles}: private keys stored here meet its threshold. Anyone with this computer and the wallet passphrase gets the coins. Use a hardware wallet or someone else's key for at least one of those slots unless this is intentional (for example, testing)."
                ),
            ))
        })
}

pub fn meta_line(fp: &str, path: &str) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(t.muted_foreground)
        .child(div().font_family(FONT_MONO).child(if fp.is_empty() {
            "no origin".to_string()
        } else {
            fp.to_lowercase()
        }))
        .when(!path.is_empty(), |d| {
            d.child("·")
                .child(div().font_family(FONT_MONO).child(path.to_string()))
        })
}

fn key_cell(k: &BoardKey, dim: bool) -> Div {
    let t = theme();
    let extra = k.device.clone();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .min_w_0()
        .when(dim, |d| d.opacity(0.6))
        .child(
            div()
                .size(px(30.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(t.muted)
                .child(
                    icon(source_icon(&k.source))
                        .size(px(15.))
                        .text_color(t.foreground),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.foreground)
                                .child(k.label.clone()),
                        )
                        .child(custody_badge(k.hot)),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(source_label(&k.source))
                        .children(extra.map(|d| format!("({d})")))
                        .child("·")
                        .child(meta_line(&k.fingerprint, &k.derivation_path)),
                ),
        )
}

fn slot_row<H: SlotBoardHost>(
    path_id: &str,
    slot: &Slot,
    state: SlotState,
    mode: BoardMode,
    cx: &mut Context<H>,
) -> Div {
    let t = theme();
    let key = format!("{path_id}-{}", slot.id);
    let id1 = slot.id.clone();
    let id2 = slot.id.clone();

    let (body, actions): (AnyElement, Vec<AnyElement>) = match (&state, mode) {
        (SlotState::Assigned(k), BoardMode::Create) => (
            key_cell(k, false).into_any_element(),
            vec![
                button(SharedString::from(format!("change-{key}")), "Change")
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, w, cx| this.pick_key(id1.clone(), w, cx)))
                    .into_any_element(),
                button(SharedString::from(format!("remove-{key}")), "Remove")
                    .ghost()
                    .small()
                    .on_click(
                        cx.listener(move |this, _, w, cx| this.clear_slot(id2.clone(), w, cx)),
                    )
                    .into_any_element(),
            ],
        ),
        (SlotState::Assigned(k), BoardMode::Restore) => (
            key_cell(k, false).into_any_element(),
            vec![
                button(SharedString::from(format!("skip-{key}")), "Keep watch-only")
                    .ghost()
                    .small()
                    .on_click(
                        cx.listener(move |this, _, w, cx| this.clear_slot(id2.clone(), w, cx)),
                    )
                    .into_any_element(),
            ],
        ),
        (SlotState::WatchOnly { skipped: Some(k) }, _) => (
            key_cell(k, true).into_any_element(),
            vec![
                button(SharedString::from(format!("use-{key}")), "Use this key")
                    .outline()
                    .small()
                    .on_click(cx.listener(move |this, _, w, cx| this.pick_key(id1.clone(), w, cx)))
                    .into_any_element(),
            ],
        ),
        (SlotState::WatchOnly { skipped: None }, _) => {
            let e = slot.expected.as_ref();
            let fp = e.and_then(|e| e.fingerprint.clone()).unwrap_or_default();
            let dp = e
                .and_then(|e| e.derivation_path.clone())
                .unwrap_or_default();
            (
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_w_0()
                    .child(
                        div()
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .border_1()
                            .border_color(t.border)
                            .child(icon("eye").size(px(15.))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(t.muted_foreground)
                                            .child("Not in your key library"),
                                    )
                                    .child(badge("Watch-only", Tone::Outline)),
                            )
                            .child(meta_line(&fp, &dp)),
                    )
                    .into_any_element(),
                vec![button(SharedString::from(format!("add-{key}")), "Add key…")
                    .outline()
                    .small()
                    .icon("plus")
                    .on_click(cx.listener(move |this, _, w, cx| this.add_key(id1.clone(), w, cx)))
                    .into_any_element()],
            )
        }
        (SlotState::Empty, _) => (
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(30.))
                        .flex_none()
                        .rounded_md()
                        .border_1()
                        .border_dashed()
                        .border_color(t.border),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t.muted_foreground)
                        .child("No key chosen"),
                )
                .into_any_element(),
            vec![
                button(SharedString::from(format!("choose-{key}")), "Choose key")
                    .outline()
                    .small()
                    .on_click(cx.listener(move |this, _, w, cx| this.pick_key(id1.clone(), w, cx)))
                    .into_any_element(),
            ],
        ),
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .min_h(px(60.))
        .px_4()
        .py_2()
        .hover(move |s| s.bg(alpha(t.muted, 0.5)))
        .child(
            div()
                .w(px(150.))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.foreground)
                        .child(slot.label.clone()),
                ),
        )
        .child(div().flex_1().min_w_0().child(body))
        .child(
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .children(actions),
        )
}
