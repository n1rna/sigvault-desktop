//! The transaction summary shown at every signing moment, in both
//! workspaces: which wallet pays, every output with its full address
//! (recipient or back to this wallet), the absolute network fee and the
//! total leaving the wallet. Amounts in BTC with sats beside them; the fee
//! rate is secondary.
//!
//! Built from a decoded PSBT ([`TxSummary::from_psbt`]) and rendered with
//! [`tx_summary`]. Callers wrap it in their own `section(..)`.

use gpui::{div, prelude::*, px, ClipboardItem, Div, ElementId, FontWeight, SharedString};

use super::common::{card, divider, format_btc, format_sat, icon};
use crate::e2e::{E2eClick, Probe};
use crate::pages::cloud::psbt_summary::PsbtSummary;
use crate::theme::{theme, FONT_MONO};

/// One output of the transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct TxOutputLine {
    pub address: String,
    pub value_sat: u64,
    /// Pays back to the wallet that is spending (change).
    pub is_change: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TxSummary {
    pub wallet_name: Option<String>,
    pub outputs: Vec<TxOutputLine>,
    /// Absolute network fee; `None` when the inputs don't carry amounts.
    pub fee_sat: Option<u64>,
    /// sat/vB, shown next to the fee.
    pub fee_rate: Option<f64>,
    /// The final transaction id, once it's known.
    pub txid: Option<String>,
    pub input_count: Option<usize>,
}

impl TxSummary {
    pub fn from_psbt(s: &PsbtSummary) -> Self {
        Self {
            wallet_name: None,
            outputs: s
                .outputs
                .iter()
                .map(|o| TxOutputLine {
                    address: o.address.clone(),
                    value_sat: o.value,
                    is_change: o.is_change,
                })
                .collect(),
            fee_sat: s.fee,
            fee_rate: None,
            txid: None,
            input_count: Some(s.input_count),
        }
    }

    pub fn wallet(mut self, name: Option<String>) -> Self {
        self.wallet_name = name.filter(|n| !n.trim().is_empty());
        self
    }

    pub fn fee_rate(mut self, rate: Option<f64>) -> Self {
        self.fee_rate = rate.filter(|r| r.is_finite() && *r > 0.0);
        self
    }

    pub fn txid(mut self, txid: Option<String>) -> Self {
        self.txid = txid.filter(|t| !t.is_empty());
        self
    }

    /// Value paid to recipients (everything that isn't change).
    pub fn sending(&self) -> u64 {
        self.outputs
            .iter()
            .filter(|o| !o.is_change)
            .map(|o| o.value_sat)
            .sum()
    }

    /// Recipients plus fee: what the wallet's balance goes down by.
    pub fn total_leaving(&self) -> Option<u64> {
        self.fee_sat.map(|f| self.sending() + f)
    }

    /// Recipients first, then change, each in transaction order.
    fn ordered_outputs(&self) -> impl Iterator<Item = &TxOutputLine> {
        let pay = self.outputs.iter().filter(|o| !o.is_change);
        pay.chain(self.outputs.iter().filter(|o| o.is_change))
    }
}

/// `2.5 sat/vB` / `12 sat/vB`.
pub fn format_fee_rate(rate: f64) -> String {
    if (rate - rate.round()).abs() < 0.05 {
        format!("{:.0} sat/vB", rate.round())
    } else {
        format!("{rate:.1} sat/vB")
    }
}

/// Small copy-to-clipboard button: a tab stop with the focus ring.
pub fn copy_button(
    id: impl Into<ElementId>,
    label: &'static str,
    value: impl Into<SharedString>,
) -> gpui::Stateful<Div> {
    let t = theme();
    let value: SharedString = value.into();
    div()
        .id(id)
        .flex_none()
        .p(px(5.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(t.hover_strong))
        .clickable(Probe::new("button", label), move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
            crate::shell::toast(cx, "Copied to clipboard");
        })
        .child(icon("copy").size(px(14.)).text_color(t.ink_2))
}

const LABEL_W: f32 = 168.;
const AMOUNT_W: f32 = 176.;

/// Ledger row: label | middle | amount column.
fn row(label: impl IntoElement, middle: Option<Div>, amount: Option<Div>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_4()
        .px_4()
        .py(px(10.))
        .child(div().w(px(LABEL_W)).flex_none().child(label))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .text_size(px(13.))
                .line_height(px(18.))
                .children(middle),
        )
        .children(amount.map(|a| {
            div()
                .w(px(AMOUNT_W))
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .child(a)
        }))
}

fn label_text(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(theme().ink_2)
        .child(text.into())
}

/// BTC (mono) over sats (+ an optional secondary note).
fn amount(sat: u64, note: Option<String>, strong: bool) -> Div {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(2.))
        .child(
            div()
                .font_family(FONT_MONO)
                .text_size(px(13.))
                .line_height(px(18.))
                .text_color(t.ink)
                .when(strong, |d| d.font_weight(FontWeight::BOLD))
                .child(format_btc(sat as i64)),
        )
        .child(
            div()
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(t.ink_2)
                .child(match note {
                    Some(n) => format!("{} · {n}", format_sat(sat as i64)),
                    None => format_sat(sat as i64),
                }),
        )
}

/// Groups of four characters separated by spaces: easier to compare
/// against a device screen, and lets the line wrap (GPUI doesn't break
/// inside a word). The copy button always copies the raw value.
pub fn chunked(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    chars
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A long identifier (address, txid) in mono that wraps, with a copy
/// button beside it.
fn data_with_copy(
    id: impl Into<ElementId>,
    copy_label: &'static str,
    value: String,
    color: gpui::Hsla,
) -> Div {
    // One box per 4-character group in a wrapping flex row: the groups
    // wrap to the column width (GPUI text inside nested flex columns
    // doesn't), and the copy button follows the last group.
    let groups: Vec<String> = chunked(&value).split(' ').map(str::to_string).collect();
    div()
        .w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_x(px(7.))
        .children(groups.into_iter().map(move |g| {
            div()
                .flex_none()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .line_height(px(24.))
                .text_color(color)
                .child(g)
        }))
        .child(copy_button(id, copy_label, value))
}

/// Render the summary. `id` keeps element ids unique when two summaries
/// share a page.
pub fn tx_summary(id: &'static str, s: &TxSummary) -> Div {
    let t = theme();
    let mut c = card();
    let mut first = true;
    let mut push = |c: Div, el: Div| -> Div {
        let c = if first { c } else { c.child(divider()) };
        first = false;
        c.child(el)
    };

    if let Some(name) = &s.wallet_name {
        c = push(
            c,
            row(
                label_text("From"),
                Some(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.ink)
                        .child(name.clone()),
                ),
                None,
            ),
        );
    }

    for (i, o) in s.ordered_outputs().enumerate() {
        let label = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                icon(if o.is_change {
                    "arrow-down-left"
                } else {
                    "arrow-up-right"
                })
                .size(px(14.))
                .flex_none()
                .text_color(if o.is_change { t.ink_2 } else { t.brand_ink }),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.ink)
                    .child(if o.is_change {
                        "Back to this wallet"
                    } else {
                        "Recipient"
                    }),
            );
        c = push(
            c,
            row(
                label,
                Some(data_with_copy(
                    (SharedString::from(format!("{id}-addr")), i),
                    "Copy address",
                    o.address.clone(),
                    if o.is_change { t.ink_muted } else { t.ink },
                )),
                Some(amount(o.value_sat, None, false)),
            ),
        );
    }

    let rate = s.fee_rate.map(format_fee_rate);
    c = push(
        c,
        match s.fee_sat {
            Some(fee) => row(
                label_text("Network fee"),
                None,
                Some(amount(fee, rate, false)),
            ),
            None => row(
                label_text("Network fee"),
                Some(
                    div()
                        .text_color(t.ink_2)
                        .child("Unknown: the inputs don't include their amounts"),
                ),
                None,
            ),
        },
    );

    let total_label = div()
        .text_size(px(13.))
        .line_height(px(18.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.ink)
        .child("Total leaving the wallet");
    c = push(
        c,
        match s.total_leaving() {
            Some(total) => row(total_label, None, Some(amount(total, None, true))),
            None => row(
                total_label,
                Some(
                    div()
                        .text_color(t.ink_2)
                        .child("At least the amount sent, plus an unknown fee"),
                ),
                Some(amount(s.sending(), None, true)),
            ),
        },
    );

    if let Some(txid) = &s.txid {
        c = push(
            c,
            row(
                label_text("Transaction ID"),
                Some(data_with_copy(
                    SharedString::from(format!("{id}-txid")),
                    "Copy transaction ID",
                    txid.clone(),
                    t.ink,
                )),
                None,
            ),
        );
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> TxSummary {
        TxSummary {
            outputs: vec![
                TxOutputLine {
                    address: "change".into(),
                    value_sat: 70_000,
                    is_change: true,
                },
                TxOutputLine {
                    address: "dest".into(),
                    value_sat: 25_000,
                    is_change: false,
                },
            ],
            fee_sat: Some(1_500),
            ..Default::default()
        }
    }

    #[test]
    fn totals_exclude_change_and_include_fee() {
        let s = s();
        assert_eq!(s.sending(), 25_000);
        assert_eq!(s.total_leaving(), Some(26_500));
        let order: Vec<_> = s.ordered_outputs().map(|o| o.address.as_str()).collect();
        assert_eq!(order, ["dest", "change"]);
        let unknown = TxSummary {
            fee_sat: None,
            ..s.clone()
        };
        assert_eq!(unknown.total_leaving(), None);
    }

    #[test]
    fn chunks_of_four() {
        assert_eq!(chunked("bcrt1qabcdef"), "bcrt 1qab cdef");
        assert_eq!(chunked("abc"), "abc");
    }

    #[test]
    fn fee_rate_formatting() {
        assert_eq!(format_fee_rate(2.0), "2 sat/vB");
        assert_eq!(format_fee_rate(2.04), "2 sat/vB");
        assert_eq!(format_fee_rate(1.53), "1.5 sat/vB");
    }
}
