//! `components/TransactionSigning.tsx` + `types/transaction.ts`: the
//! TRANSACTION_SIGNING session workflow — review the PSBT, find an
//! authorised hardware wallet, sign on-device and submit the signature.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::*, px, AnyElement, ClipboardItem, Context, FontWeight, SharedString, Window,
};
use serde::Deserialize;
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_get_ledger_hmacs, cmd_sign_psbt,
    cmd_submit_transaction_signature, cmd_unlock_device,
};
use sigvault_core::hwi::{DiscoveredDevice, SignedPsbt};
use sigvault_core::State;

use super::common::{card_message, dev_step, icon_tile, main_window, non_empty, review_row};
use super::devices::{device_list, fingerprint, is_supported, DeviceListHost, DeviceListProps};
use super::psbt_summary::{hrp_for, summarize, PsbtSummary};
use crate::runtime::backend;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, badge, button, card, divider, format_btc, format_sat, section, spinner, Tone,
};

/// Stepper labels for the signing flow.
pub const STEPS: [&str; 3] = ["Review", "Sign on device", "Done"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Review,
    Sign,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TransactionData {
    pub psbt: String,
    pub txid: String,
    #[serde(default)]
    pub wallet_name: Option<String>,
    #[serde(default)]
    pub multipath_descriptor: Option<String>,
    #[serde(default)]
    pub ledger_hmacs: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SignatureSlot {
    pub fingerprint: String,
    pub derivation_path: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TransactionSigningData {
    pub transaction: TransactionData,
    pub signature_slots: Vec<SignatureSlot>,
}

fn short_txid(txid: &str) -> String {
    let n = txid.chars().count();
    if n <= 18 {
        return txid.to_string();
    }
    let start: String = txid.chars().take(8).collect();
    let end: String = txid.chars().skip(n - 8).collect();
    format!("{start}…{end}")
}

pub struct TransactionSigning {
    data: TransactionSigningData,
    session_id: String,
    network: Option<String>,
    summary: Option<PsbtSummary>,
    step: Step,
    /// The signature was accepted by the coordinator.
    submitted: bool,
    discovering: bool,
    devices: Vec<DiscoveredDevice>,
    selected: Option<DiscoveredDevice>,
    signing: bool,
    submitting: bool,
    unlocking_id: Option<String>,
    error: Option<SharedString>,
    psbt_expanded: bool,
}

impl TransactionSigning {
    pub fn new(data: TransactionSigningData, session_id: String, network: Option<String>) -> Self {
        let summary = Self::summary_for(&data, network.as_deref());
        Self {
            data,
            session_id,
            network,
            summary,
            step: Step::Review,
            submitted: false,
            discovering: false,
            devices: Vec::new(),
            selected: None,
            signing: false,
            submitting: false,
            unlocking_id: None,
            error: None,
            psbt_expanded: false,
        }
    }

    /// Prop update from the parent page; component state is kept, as React
    /// would on a re-render with new props.
    pub fn set_props(
        &mut self,
        data: TransactionSigningData,
        session_id: String,
        network: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.data != data || self.session_id != session_id || self.network != network {
            self.summary = Self::summary_for(&data, network.as_deref());
            self.data = data;
            self.session_id = session_id;
            self.network = network;
            cx.notify();
        }
    }

    fn summary_for(data: &TransactionSigningData, network: Option<&str>) -> Option<PsbtSummary> {
        let hrp = hrp_for(network, data.transaction.multipath_descriptor.as_deref());
        summarize(&data.transaction.psbt, hrp)
    }

    fn wallet_config(&self) -> WalletConfigInput {
        WalletConfigInput {
            name: self.data.transaction.wallet_name.clone(),
            descriptor: self.data.transaction.multipath_descriptor.clone(),
            hmac: None,
            ledger_hmacs: self.data.transaction.ledger_hmacs.clone(),
        }
    }

    fn signing_fingerprints(&self) -> HashSet<String> {
        self.data
            .signature_slots
            .iter()
            .map(|s| s.fingerprint.clone())
            .collect()
    }

    fn can_sign(&self, device: &DiscoveredDevice) -> bool {
        is_supported(device)
            && fingerprint(device).is_some_and(|fp| {
                self.data
                    .signature_slots
                    .iter()
                    .any(|s| s.fingerprint == fp)
            })
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        self.discovering = true;
        self.error = None;
        cx.notify();
        let cfg = self.wallet_config();
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_discover_hardware_wallets(app, window, State::new(&st), Some(cfg)).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<Vec<DiscoveredDevice>>(
                            r.data.unwrap_or_default(),
                        ) {
                            Ok(devices) => {
                                let matching: Vec<_> = devices
                                    .iter()
                                    .filter(|d| this.can_sign(d))
                                    .cloned()
                                    .collect();
                                if matching.len() == 1 {
                                    this.selected = matching.into_iter().next();
                                }
                                this.devices = devices;
                            }
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        this.error = Some(non_empty(r.message, "Failed to discover devices").into())
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.discovering = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn sign(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.selected.clone().filter(is_supported) else {
            return;
        };
        let Some(fp) = fingerprint(&selected) else {
            return;
        };
        let Some(slot) = self
            .data
            .signature_slots
            .iter()
            .find(|s| s.fingerprint == fp)
            .cloned()
        else {
            self.error = Some("Selected device is not authorized to sign this transaction".into());
            cx.notify();
            return;
        };
        self.signing = true;
        self.error = None;
        cx.notify();

        let cfg = self.wallet_config();
        let psbt = self.data.transaction.psbt.clone();
        let txid = self.data.transaction.txid.clone();
        let session_id = self.session_id.clone();
        let device_id = selected.id.clone();
        let sign_fp = fp.clone();
        let sign_task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_sign_psbt(window, State::new(&st), device_id, sign_fp, psbt, Some(cfg)).await
        });

        cx.spawn(async move |this, cx| {
            let finish = |this: &mut Self, err: Option<String>, cx: &mut Context<Self>| {
                if let Some(e) = err {
                    this.error = Some(e.into());
                }
                this.signing = false;
                this.submitting = false;
                cx.notify();
            };

            let signed = match sign_task.await {
                Ok(r) if r.success && r.data.is_some() => {
                    match serde_json::from_value::<SignedPsbt>(r.data.unwrap_or_default()) {
                        Ok(s) => s,
                        Err(e) => {
                            this.update(cx, |this, cx| finish(this, Some(e.to_string()), cx))
                                .ok();
                            return;
                        }
                    }
                }
                Ok(r) => {
                    let msg = non_empty(r.message, "Failed to sign transaction");
                    this.update(cx, |this, cx| finish(this, Some(msg), cx)).ok();
                    return;
                }
                Err(e) => {
                    this.update(cx, |this, cx| finish(this, Some(e), cx)).ok();
                    return;
                }
            };

            let Ok(submit_task) = this.update(cx, |this, cx| {
                this.signing = false;
                this.submitting = true;
                cx.notify();
                backend(cx).call(move |app, st| async move {
                    // Non-critical: HMACs won't be persisted this time.
                    let mut hmacs: HashMap<String, String> = HashMap::new();
                    if let Ok(window) = main_window(&app) {
                        if let Ok(r) = cmd_get_ledger_hmacs(window, State::new(&st)).await {
                            if r.success {
                                if let Some(m) = r.data.and_then(|d| serde_json::from_value(d).ok())
                                {
                                    hmacs = m;
                                }
                            }
                        }
                    }
                    let window = main_window(&app)?;
                    cmd_submit_transaction_signature(
                        window,
                        State::new(&st),
                        session_id,
                        signed.psbt,
                        txid,
                        fp,
                        slot.derivation_path,
                        if hmacs.is_empty() { None } else { Some(hmacs) },
                    )
                    .await
                })
            }) else {
                return;
            };

            let err = match submit_task.await {
                // onSignatureSubmitted(): no-op in SessionDetails.
                Ok(r) if r.success => {
                    this.update(cx, |this, _| this.submitted = true).ok();
                    None
                }
                Ok(r) => Some(non_empty(r.message, "Failed to submit signature")),
                Err(e) => Some(e),
            };
            this.update(cx, |this, cx| finish(this, err, cx)).ok();
        })
        .detach();
    }
}

impl DeviceListHost for TransactionSigning {
    fn select_device(&mut self, device_id: &str, cx: &mut Context<Self>) {
        if let Some(dev) = self.devices.iter().find(|d| d.id == device_id) {
            if is_supported(dev) {
                self.selected = Some(dev.clone());
                cx.notify();
            }
        }
    }

    fn unlock_device(&mut self, device_id: &str, cx: &mut Context<Self>) {
        let device_id = device_id.to_string();
        self.unlocking_id = Some(device_id.clone());
        self.error = None;
        cx.notify();
        let cfg = self.wallet_config();
        let id = device_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_unlock_device(app, window, State::new(&st), id, Some(cfg)).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<DiscoveredDevice>(r.data.unwrap_or_default())
                        {
                            Ok(dev) => {
                                for x in this.devices.iter_mut() {
                                    if x.id == device_id {
                                        *x = dev.clone();
                                    }
                                }
                                if this.can_sign(&dev) {
                                    this.selected = Some(dev);
                                }
                            }
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        this.error = Some(non_empty(r.message, "Failed to unlock device").into())
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.unlocking_id = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn copy_icon(id: &'static str, value: String) -> impl IntoElement {
    let t = theme();
    div()
        .id(id)
        .flex_none()
        .p(px(5.))
        .rounded(px(4.))
        .cursor_pointer()
        .hover(move |s| s.bg(t.muted))
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
            crate::shell::toast(cx, "Copied to clipboard");
        })
        .child(crate::ui::icon("copy").size(px(13.)))
}

fn amount(sat: u64) -> gpui::Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_baseline()
        .gap_2()
        .child(
            div()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .text_color(t.foreground)
                .child(format_btc(sat as i64)),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(format_sat(sat as i64)),
        )
}

impl TransactionSigning {
    /// Debug screenshot hook (`SIGVAULT_DEV_CLOUD_STEP=sign|done`).
    pub fn apply_dev_step(&mut self, cx: &mut Context<Self>) {
        match dev_step().as_deref() {
            Some("sign") => {
                self.step = Step::Sign;
                self.discover(cx);
            }
            Some("done") => self.submitted = true,
            _ => {}
        }
    }

    /// Current step for the page's stepper (see [`STEPS`]).
    pub fn step_index(&self) -> usize {
        if self.submitted {
            2
        } else {
            match self.step {
                Step::Review => 0,
                Step::Sign => 1,
            }
        }
    }

    pub fn subtitle(&self) -> &'static str {
        match self.step_index() {
            0 => "Check where the bitcoin goes and the fee before you sign.",
            1 => "Connect an authorized hardware wallet and approve on the device.",
            _ => "Your signature was submitted to the session.",
        }
    }

    fn busy(&self) -> bool {
        self.discovering || self.signing || self.submitting
    }

    /// Footer buttons for the page frame: (left, right).
    pub fn footer(&self, cx: &mut Context<Self>) -> (Vec<AnyElement>, Vec<AnyElement>) {
        let t = theme();
        if self.submitted {
            return (vec![], vec![]);
        }
        match self.step {
            Step::Review => {
                let n = self.data.signature_slots.len();
                (
                    vec![div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(format!(
                            "{n} authorized {}",
                            if n == 1 { "signer" } else { "signers" }
                        ))
                        .into_any_element()],
                    vec![button("to-sign", "Continue to signing")
                        .icon_right("arrow-right")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.step = Step::Sign;
                            if this.devices.is_empty() && !this.discovering {
                                this.discover(cx);
                            }
                            cx.notify();
                        }))
                        .into_any_element()],
                )
            }
            Step::Sign => {
                let busy = self.signing || self.submitting;
                let can = self.selected.as_ref().is_some_and(|d| self.can_sign(d));
                let label = if self.signing {
                    "Confirm on device…"
                } else if self.submitting {
                    "Submitting signature…"
                } else {
                    "Sign transaction"
                };
                (
                    vec![button("sign-back", "Back")
                        .outline()
                        .icon("arrow-left")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.step = Step::Review;
                            cx.notify();
                        }))
                        .into_any_element()],
                    vec![button("sign-tx", label)
                        .loading(busy)
                        .when(!busy, |b| b.icon("pen"))
                        .disabled(!can && !busy)
                        .on_click(cx.listener(|this, _, _, cx| this.sign(cx)))
                        .into_any_element()],
                )
            }
        }
    }

    fn render_summary(&self) -> AnyElement {
        let t = theme();
        let tx = &self.data.transaction;
        let mut c = card();
        if let Some(name) = tx.wallet_name.clone().filter(|n| !n.is_empty()) {
            c = c
                .child(review_row(
                    "Wallet",
                    div().font_weight(FontWeight::MEDIUM).child(name),
                ))
                .child(divider());
        }
        if let Some(s) = &self.summary {
            c = c
                .child(review_row("Sending", amount(s.sending())))
                .child(divider())
                .child(review_row(
                    "Network fee",
                    match s.fee {
                        Some(fee) => amount(fee).into_any_element(),
                        None => div()
                            .text_color(t.muted_foreground)
                            .child("Unknown — inputs don't include their amounts")
                            .into_any_element(),
                    },
                ))
                .child(divider())
                .child(review_row(
                    "Inputs",
                    div().child(format!(
                        "{} {}",
                        s.input_count,
                        if s.input_count == 1 { "coin" } else { "coins" }
                    )),
                ))
                .child(divider());
        }
        c.child(review_row(
            "Transaction ID",
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .child(short_txid(&tx.txid)),
                )
                .child(copy_icon("copy-txid", tx.txid.clone())),
        ))
        .into_any_element()
    }

    fn render_outputs(&self) -> Option<AnyElement> {
        let t = theme();
        let s = self.summary.as_ref()?;
        let mut list = card().overflow_hidden();
        for (i, o) in s.outputs.iter().enumerate() {
            if i > 0 {
                list = list.child(divider());
            }
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_4()
                    .min_h(px(52.))
                    .px_4()
                    .py_2()
                    .child(icon_tile(
                        if o.is_change {
                            "arrow-down-left"
                        } else {
                            "arrow-up-right"
                        },
                        32.,
                        Some(if o.is_change {
                            t.muted_foreground
                        } else {
                            t.primary
                        }),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.))
                                    .line_height(px(17.))
                                    .text_color(t.foreground)
                                    .child(o.address.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(if o.is_change {
                                        "Back to the wallet"
                                    } else {
                                        "Recipient"
                                    }),
                            ),
                    )
                    .when(o.is_change, |d| d.child(badge("Change", Tone::Secondary)))
                    .child(
                        div()
                            .w(px(150.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(if o.is_change {
                                t.muted_foreground
                            } else {
                                t.foreground
                            })
                            .child(format_btc(o.value as i64)),
                    ),
            );
        }
        Some(
            section("Outputs")
                .description("Where the bitcoin goes. Change returns to the team wallet.")
                .child(list)
                .into_any_element(),
        )
    }

    fn render_signers(&self) -> AnyElement {
        let t = theme();
        let slots = &self.data.signature_slots;
        let mut list = card().overflow_hidden();
        for (i, slot) in slots.iter().enumerate() {
            if i > 0 {
                list = list.child(divider());
            }
            let present = self
                .devices
                .iter()
                .any(|d| fingerprint(d).as_deref() == Some(slot.fingerprint.as_str()));
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_4()
                    .h(px(48.))
                    .px_4()
                    .child(icon_tile(
                        "key",
                        28.,
                        Some(if present {
                            t.success
                        } else {
                            t.muted_foreground
                        }),
                    ))
                    .child(
                        div()
                            .w(px(96.))
                            .flex_none()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(slot.fingerprint.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(slot.derivation_path.clone()),
                    )
                    .when(present, |d| d.child(badge("Connected", Tone::Success))),
            );
        }
        section("Your signing keys")
            .description("Any one of these keys on a connected device can provide your signature.")
            .child(list)
            .into_any_element()
    }

    fn render_psbt(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let psbt = self.data.transaction.psbt.clone();
        let chars = psbt.chars().count();
        card()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(px(48.))
                    .px_4()
                    .child(icon_tile("file", 28., None))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Raw PSBT"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(format!("Base64 · {chars} characters")),
                            ),
                    )
                    .child(copy_icon("copy-psbt", psbt.clone()))
                    .child(
                        button(
                            "psbt-toggle",
                            if self.psbt_expanded { "Hide" } else { "Show" },
                        )
                        .ghost()
                        .small()
                        .icon_right(if self.psbt_expanded {
                            "chevron-up"
                        } else {
                            "chevron-down"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.psbt_expanded = !this.psbt_expanded;
                            cx.notify();
                        })),
                    ),
            )
            .when(self.psbt_expanded, |c| {
                c.child(divider()).child(
                    div()
                        .id("psbt-payload")
                        .max_h(px(200.))
                        .overflow_y_scroll()
                        .bg(alpha(t.background, 0.6))
                        .px_4()
                        .py_3()
                        .font_family(FONT_MONO)
                        .text_size(px(11.))
                        .line_height(px(17.))
                        .text_color(t.muted_foreground)
                        .child(psbt),
                )
            })
            .into_any_element()
    }

    fn render_review(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut v = vec![section("Summary")
            .child(self.render_summary())
            .into_any_element()];
        if self.summary.is_none() {
            v.push(
                alert(Tone::Warning, "This PSBT couldn't be decoded for review. Check it in another tool before signing.")
                    .into_any_element(),
            );
        }
        v.extend(self.render_outputs());
        v.push(self.render_signers());
        v.push(
            section("Details")
                .child(self.render_psbt(cx))
                .into_any_element(),
        );
        v
    }

    fn render_sign(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = theme();
        let signers = self.signing_fingerprints();
        let has_matching = self.devices.iter().any(|d| self.can_sign(d));
        let scan = button(
            "scan",
            if self.discovering {
                "Scanning…"
            } else if self.devices.is_empty() {
                "Scan for devices"
            } else {
                "Scan again"
            },
        )
        .outline()
        .small()
        .icon("usb")
        .loading(self.discovering)
        .disabled(self.busy())
        .on_click(cx.listener(|this, _, _, cx| this.discover(cx)));

        let body: AnyElement = if self.devices.is_empty() {
            card()
                .flex_row()
                .items_center()
                .gap_4()
                .px_4()
                .py_4()
                .child(icon_tile("usb", 32., Some(t.muted_foreground)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(if self.discovering { "Looking for devices…" } else { "No devices found yet" }),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .text_color(t.muted_foreground)
                                .child("Plug in a hardware wallet holding one of your signing keys, unlock it and open the Bitcoin app."),
                        ),
                )
                .when(self.discovering, |c| c.child(spinner(px(16.))))
                .into_any_element()
        } else {
            let devices = self.devices.clone();
            let selected_id = self.selected.as_ref().map(|d| d.id.clone());
            let unlocking_id = self.unlocking_id.clone();
            device_list(
                DeviceListProps {
                    devices: &devices,
                    selected_id: selected_id.as_deref(),
                    can_unlock: true,
                    highlighted_fingerprints: Some(&signers),
                    signer_fingerprints: Some(&signers),
                    unlocking_id: unlocking_id.as_deref(),
                    unlock_status: None,
                },
                cx,
            )
        };

        let mut v = vec![section("Hardware wallet")
            .description("Devices holding an authorized key are marked Signer.")
            .action(scan)
            .when(!self.devices.is_empty() && !has_matching, |s| {
                let expected = self
                    .data
                    .signature_slots
                    .iter()
                    .map(|s| s.fingerprint.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                s.child(alert(
                    Tone::Destructive,
                    format!("No authorized signers found. Expected: {expected}"),
                ))
            })
            .child(body)
            .into_any_element()];

        if self.signing {
            v.push(
                card()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_color(alpha(t.primary, 0.4))
                    .bg(alpha(t.primary, 0.05))
                    .child(spinner(px(16.)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Confirm on your device"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(
                                    "Check the outputs and fee on the device screen, then approve.",
                                ),
                            ),
                    )
                    .into_any_element(),
            );
        }
        if let Some(s) = &self.summary {
            v.push(
                section("You're approving")
                    .child(
                        card()
                            .child(review_row("Sending", amount(s.sending())))
                            .child(divider())
                            .child(review_row(
                                "Network fee",
                                match s.fee {
                                    Some(f) => amount(f).into_any_element(),
                                    None => div()
                                        .text_color(t.muted_foreground)
                                        .child("Unknown")
                                        .into_any_element(),
                                },
                            )),
                    )
                    .into_any_element(),
            );
        }
        v
    }
}

impl Render for TransactionSigning {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let body = if self.submitted {
            vec![card_message(
                "circle-check",
                Some(t.success),
                "Signature submitted",
                "SigVault will broadcast the transaction once enough co-signers have signed. You can leave this session.",
            )
            .into_any_element()]
        } else {
            match self.step {
                Step::Review => self.render_review(cx),
                Step::Sign => self.render_sign(cx),
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_6()
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .children(body)
    }
}
