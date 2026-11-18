//! `setup/SlotBoard.tsx` — spending paths with their key slots. Clicking
//! a slot opens its editor in place; filled slots show where the key
//! came from and whether its private half will be stored on this
//! computer.

use gpui::{div, prelude::*, px, Context, Div, Entity, FontWeight, SharedString, Window};

use super::policy::{paths_spendable_from_this_device, Slot, SlotLayout};
use super::slot_editor::SlotEditor;
use super::ui::{fingerprint, setup_alert, truncate_middle, AlertTone};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{badge, Tone};

/// The page that owns the board's state.
pub trait SlotBoardHost: Sized + 'static {
    fn edit_slot(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>);
    fn clear_slot(&mut self, slot_id: String, window: &mut Window, cx: &mut Context<Self>);
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

pub fn slot_board<H: SlotBoardHost>(
    layout: &SlotLayout,
    editor: Option<&Entity<SlotEditor>>,
    cx: &mut Context<H>,
) -> Div {
    let t = theme();
    let editing = editor.map(|e| e.read(cx).slot_id().to_string());
    let exposed = paths_spendable_from_this_device(layout);

    let sections: Vec<_> = layout
        .paths
        .iter()
        .map(|path| {
            let filled = path
                .slot_ids
                .iter()
                .filter(|id| layout.slot(id).is_some_and(|s| s.filled.is_some()))
                .count();
            div()
                .flex()
                .flex_col()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .bg(alpha(t.card, 0.4))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .border_b_1()
                        .border_color(t.border)
                        .px_4()
                        .py_3()
                        .child(
                            div()
                                .flex()
                                .flex_col()
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
                                                .child(path.title.clone()),
                                        )
                                        .children(path.timelock_blocks.map(|b| {
                                            badge(format!("{} blocks", format_thousands(b)), Tone::Secondary)
                                        })),
                                )
                                .child(
                                    div()
                                        .mt(px(2.))
                                        .text_size(px(11.))
                                        .text_color(t.muted_foreground)
                                        .child(path.subtitle.clone()),
                                ),
                        )
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(11.))
                                .text_color(t.muted_foreground)
                                .child(format!("{filled}/{}", path.slot_ids.len())),
                        ),
                )
                .children(path.slot_ids.iter().enumerate().filter_map(|(i, id)| {
                    let slot = layout.slot(id)?;
                    let body = if editing.as_deref() == Some(id.as_str()) {
                        editor?.clone().into_any_element()
                    } else {
                        slot_row(&path.id, slot, cx).into_any_element()
                    };
                    Some(
                        div()
                            .px_4()
                            .py_3()
                            .when(i > 0, |d| d.border_t_1().border_color(t.border))
                            .child(body),
                    )
                }))
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_5()
        .children(sections)
        .when(!exposed.is_empty(), |d| {
            let titles = exposed
                .iter()
                .map(|p| p.title.to_lowercase())
                .collect::<Vec<_>>()
                .join(" and ");
            d.child(setup_alert(
                AlertTone::Warning,
                Some("This computer alone could spend these funds.".into()),
                Some(
                    format!(
                        "Private keys stored here meet the threshold for {titles}. Anyone who gets this computer and the wallet passphrase gets the coins, which defeats the point of a multi-key setup. Use a hardware wallet or another person's key for at least one of those slots unless this is intentional (for example, testing)."
                    )
                    .into_any_element(),
                ),
            ))
        })
}

fn row_button(id: SharedString, label: &'static str, destructive: bool) -> gpui::Stateful<Div> {
    let t = theme();
    div()
        .id(id)
        .rounded_md()
        .px(px(10.))
        .py(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.muted_foreground)
        .cursor_pointer()
        .hover(move |s| s.text_color(if destructive { t.destructive } else { t.foreground }))
        .child(label)
}

fn slot_row<H: SlotBoardHost>(path_id: &str, slot: &Slot, cx: &mut Context<H>) -> Div {
    let t = theme();
    let id = slot.id.clone();
    let key = format!("{path_id}-{}", slot.id);

    let Some(f) = &slot.filled else {
        // Recover mode: the public key is already known from the
        // descriptor, so an unfilled slot is "watch-only", not "empty".
        let expected = slot.expected.as_ref();
        let detail = match expected {
            Some(e) => {
                let mut row = div().flex().flex_row().items_center().gap_x(px(4.));
                row = match &e.fingerprint {
                    Some(fp) => row.child(fingerprint(fp)),
                    None => row.child("no origin"),
                };
                if let Some(p) = &e.derivation_path {
                    row = row.child("·").child(div().font_family(FONT_MONO).child(p.clone()));
                }
                row.child("· watch-only")
            }
            None => div().child("Empty"),
        };
        let has_expected = expected.is_some();
        return div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(slot.label.clone()),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .overflow_hidden()
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("add-{key}")))
                    .flex_none()
                    .rounded_md()
                    .px_3()
                    .py(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .map(|d| {
                        if has_expected {
                            d.border_1()
                                .border_color(t.border)
                                .text_color(t.muted_foreground)
                                .hover(move |s| s.text_color(t.foreground))
                        } else {
                            d.bg(t.primary)
                                .text_color(t.primary_foreground)
                                .hover(move |s| s.bg(alpha(t.primary, 0.9)))
                        }
                    })
                    .on_click(cx.listener(move |this, _, w, cx| this.edit_slot(id.clone(), w, cx)))
                    .child(if has_expected { "Attach private key" } else { "Add key" }),
            );
    };

    let tone = if f.secret.is_some() {
        Tone::Warning
    } else if f.source == super::api::SlotSource::Hardware {
        Tone::Success
    } else {
        Tone::Outline
    };
    let id2 = slot.id.clone();
    let mut detail = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_x(px(4.))
        .child(fingerprint(&f.key.fingerprint))
        .child("·")
        .child(div().font_family(FONT_MONO).child(f.key.derivation_path.clone()))
        .child("·")
        .child(div().font_family(FONT_MONO).child(truncate_middle(&f.key.xpub, 10, 6)));
    if let Some(d) = &f.detail {
        detail = detail.child("·").child(d.clone());
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
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
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.foreground)
                                .child(slot.label.clone()),
                        )
                        .child(badge(f.source.label(), tone))
                        .when(f.secret.is_some(), |d| {
                            d.child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(t.muted_foreground)
                                    .child("stored on this computer"),
                            )
                        }),
                )
                .child(
                    div()
                        .mt(px(2.))
                        .overflow_hidden()
                        .text_size(px(11.))
                        .text_color(t.muted_foreground)
                        .child(detail),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap_1()
                .child(
                    row_button(SharedString::from(format!("change-{key}")), "Change", false)
                        .on_click(cx.listener(move |this, _, w, cx| this.edit_slot(id.clone(), w, cx))),
                )
                .child(
                    row_button(SharedString::from(format!("remove-{key}")), "Remove", true)
                        .on_click(cx.listener(move |this, _, w, cx| this.clear_slot(id2.clone(), w, cx))),
                ),
        )
}
