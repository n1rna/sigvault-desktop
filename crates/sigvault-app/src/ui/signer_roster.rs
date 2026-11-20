//! Who has signed, who is still needed, and which key is yours.
//!
//! A [`Roster`] is filled from whatever the caller actually knows (the
//! cloud signing session: signature slots, signatures already in the PSBT,
//! the device you picked, the threshold when the wallet descriptor makes it
//! unambiguous). Unknown facts stay unknown: a key with no signature in
//! view reads "No signature seen", not "Not signed".

use gpui::{div, prelude::*, px, Div, FontWeight, SharedString};

use super::common::{badge, card, divider, icon, Tone};
use crate::theme::{theme, FONT_MONO};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Signer {
    /// Master fingerprint (8 hex chars).
    pub fingerprint: String,
    /// Device or key name, when known.
    pub name: Option<String>,
    /// Secondary detail, e.g. the derivation path.
    pub detail: Option<String>,
    /// A signature from this key is in view.
    pub signed: bool,
    /// The key on the device you're signing with.
    pub is_you: bool,
    /// A SigVault-held co-signer key.
    pub sigvault: bool,
}

impl Signer {
    /// "Ledger Nano X (3f8a91c2)" or the bare fingerprint.
    pub fn display(&self) -> String {
        match &self.name {
            Some(n) => format!("{n} ({})", self.fingerprint),
            None => self.fingerprint.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Roster {
    pub signers: Vec<Signer>,
    /// Signatures needed, when known.
    pub threshold: Option<usize>,
    /// Shown under the list (e.g. what this view can't see).
    pub note: Option<String>,
}

impl Roster {
    pub fn signed_count(&self) -> usize {
        self.signers.iter().filter(|s| s.signed).count()
    }

    /// Signatures still needed, when the threshold is known.
    pub fn still_needed(&self) -> Option<usize> {
        self.threshold
            .map(|k| k.saturating_sub(self.signed_count()))
    }

    /// Keys with no signature in view.
    pub fn unsigned(&self) -> impl Iterator<Item = &Signer> {
        self.signers.iter().filter(|s| !s.signed)
    }

    /// "1 of 2 signed" / "1 signed so far" / "No signatures yet".
    pub fn headline(&self) -> String {
        let n = self.signed_count();
        match self.threshold {
            Some(k) => format!("{n} of {k} signed"),
            None if n == 0 => "No signatures yet".into(),
            None => format!("{n} signed so far"),
        }
    }

    /// "Waiting on b71d04e9 or 5c2e88a0" / "Waiting on 2 more: a, b or c".
    /// `None` when nothing more is needed or nothing is known.
    pub fn waiting_on(&self) -> Option<String> {
        let open: Vec<String> = self.unsigned().map(Signer::display).collect();
        if open.is_empty() {
            return None;
        }
        let list = join_or(&open);
        match self.still_needed() {
            Some(0) => None,
            Some(1) => Some(format!("waiting on {list}")),
            Some(more) if more >= open.len() => Some(format!("waiting on {}", join_and(&open))),
            Some(more) => Some(format!("waiting on {more} more from {list}")),
            None => Some(format!("no signature seen yet from {}", join_and(&open))),
        }
    }
}

fn join_with(items: &[String], last: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., tail] => format!("{} {last} {tail}", init.join(", ")),
    }
}

fn join_or(items: &[String]) -> String {
    join_with(items, "or")
}

fn join_and(items: &[String]) -> String {
    join_with(items, "and")
}

fn signer_row(s: &Signer, you_label: &str) -> Div {
    let t = theme();
    let (status_icon, status_color) = if s.signed {
        ("circle-check", t.success)
    } else {
        ("circle", t.ink_3)
    };
    let status_text: SharedString = if s.signed {
        "Signed".into()
    } else if s.is_you {
        "Not signed yet".into()
    } else {
        "No signature seen".into()
    };
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_3()
        .px_4()
        .py(px(10.))
        .child(
            icon(status_icon)
                .size(px(16.))
                .mt(px(1.))
                .flex_none()
                .text_color(status_color),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .children(s.name.clone().map(|n| {
                            div()
                                .text_size(px(13.))
                                .line_height(px(18.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.ink)
                                .child(n)
                        }))
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(12.))
                                .line_height(px(18.))
                                .text_color(if s.name.is_some() { t.ink_2 } else { t.ink })
                                .child(s.fingerprint.clone()),
                        )
                        .when(s.is_you, |d| {
                            d.child(badge(you_label.to_string(), Tone::Default))
                        })
                        .when(s.sigvault, |d| d.child(badge("SigVault key", Tone::Accent))),
                )
                .children(s.detail.clone().map(|d| {
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(t.ink_2)
                        .child(d)
                })),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(12.))
                .line_height(px(18.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if s.signed {
                    Tone::Success.ink()
                } else {
                    t.ink_2
                })
                .child(status_text),
        )
}

/// Render the roster card: headline with the quorum, one row per key.
pub fn signer_roster(r: &Roster) -> Div {
    let t = theme();
    let total = r.signers.len();
    let needs = match r.threshold {
        Some(k) => format!("Needs {k} of {total} keys"),
        None => format!(
            "{total} {} can sign",
            if total == 1 { "key" } else { "keys" }
        ),
    };
    let mut c = card().child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .px_4()
            .py_3()
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.ink)
                    .child(r.headline()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(t.ink_2)
                    .child(needs),
            ),
    );
    for s in &r.signers {
        c = c.child(divider()).child(signer_row(s, "You"));
    }
    if let Some(note) = &r.note {
        c = c.child(divider()).child(
            div()
                .px_4()
                .py(px(10.))
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(t.ink_2)
                .child(note.clone()),
        );
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(fp: &str, signed: bool) -> Signer {
        Signer {
            fingerprint: fp.into(),
            signed,
            ..Default::default()
        }
    }

    #[test]
    fn headline_and_waiting_on() {
        let mut r = Roster {
            signers: vec![
                key("aaaa0001", true),
                key("bbbb0002", false),
                key("cccc0003", false),
            ],
            threshold: Some(2),
            note: None,
        };
        assert_eq!(r.headline(), "1 of 2 signed");
        assert_eq!(r.still_needed(), Some(1));
        assert_eq!(
            r.waiting_on().as_deref(),
            Some("waiting on bbbb0002 or cccc0003")
        );

        r.threshold = Some(3);
        assert_eq!(
            r.waiting_on().as_deref(),
            Some("waiting on bbbb0002 and cccc0003")
        );

        r.signers[1].signed = true;
        r.threshold = Some(2);
        assert_eq!(r.headline(), "2 of 2 signed");
        assert_eq!(r.waiting_on(), None);

        r.threshold = None;
        assert_eq!(r.headline(), "2 signed so far");
        assert_eq!(
            r.waiting_on().as_deref(),
            Some("no signature seen yet from cccc0003")
        );
    }

    #[test]
    fn display_names_the_device() {
        let mut s = key("3f8a91c2", false);
        assert_eq!(s.display(), "3f8a91c2");
        s.name = Some("Ledger Nano X".into());
        assert_eq!(s.display(), "Ledger Nano X (3f8a91c2)");
    }
}
