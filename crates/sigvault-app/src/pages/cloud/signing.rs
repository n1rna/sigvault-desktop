//! `components/TransactionSigning.tsx` + `types/transaction.ts`: the
//! TRANSACTION_SIGNING session workflow — review the PSBT, find an
//! authorised hardware wallet, sign on-device and submit the signature.

use std::collections::{HashMap, HashSet};

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};
use serde::Deserialize;
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_get_ledger_hmacs, cmd_sign_psbt,
    cmd_submit_transaction_signature, cmd_unlock_device,
};
use sigvault_core::hwi::{DiscoveredDevice, SignedPsbt};
use sigvault_core::State;

use super::common::{card_message, dev_step, icon_tile, main_window, non_empty};
use super::devices::{device_list, fingerprint, is_supported, DeviceListHost, DeviceListProps};
use super::psbt_summary::{hrp_for, summarize, PsbtSummary};
use crate::runtime::backend;
use crate::theme::{theme, FONT_MONO};
use crate::ui::{
    alert, button, card, copy_button, divider, section, signer_roster, spinner, tx_summary, Roster,
    Signer, Tone, TxSummary,
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

/// Signatures needed, when the wallet descriptor makes it unambiguous: one
/// `multi` / `sortedmulti` (`_a`) fragment and no timelocked paths, with as
/// many keys as the session lists signers. A descriptor without any multi
/// fragment and a single signer is a single-key wallet (1 of 1).
fn threshold_from_descriptor(descriptor: Option<&str>, signers: usize) -> Option<usize> {
    let d = descriptor?;
    let multis = d.matches("multi(").count() + d.matches("multi_a(").count();
    if d.contains("older(") || d.contains("after(") {
        return None;
    }
    if multis == 0 {
        return (signers == 1).then_some(1);
    }
    if multis != 1 {
        return None;
    }
    let start = d
        .find("multi(")
        .map(|i| i + "multi(".len())
        .or_else(|| d.find("multi_a(").map(|i| i + "multi_a(".len()))?;
    // Arguments up to the matching close paren, split on top-level commas.
    let mut depth = 0usize;
    let mut args = vec![String::new()];
    for ch in d[start..].chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => break,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = args.last_mut() {
            last.push(ch);
        }
    }
    let k: usize = args.first()?.trim().parse().ok()?;
    let keys = args.len() - 1;
    (k >= 1 && k <= keys && keys == signers).then_some(k)
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
    /// Fingerprint of the key whose signature the coordinator accepted.
    signed_fp: Option<String>,
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
            signed_fp: None,
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
        let sign_fp_done = fp.clone();
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
                    this.update(cx, |this, _| {
                        this.submitted = true;
                        this.signed_fp = Some(sign_fp_done);
                    })
                    .ok();
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

impl TransactionSigning {
    /// Debug screenshot hook (`SIGVAULT_DEV_CLOUD_STEP=sign|done`).
    pub fn apply_dev_step(&mut self, cx: &mut Context<Self>) {
        match dev_step().as_deref() {
            Some("sign") => {
                self.step = Step::Sign;
                self.discover(cx);
            }
            Some("done") => {
                self.submitted = true;
                self.signed_fp = self
                    .data
                    .signature_slots
                    .first()
                    .map(|s| s.fingerprint.clone());
            }
            _ => {}
        }
    }

    /// Current step for the page's stepper (see [`STEPS`]).
    /// Past the last step once submitted, so every step shows a check.
    pub fn step_index(&self) -> usize {
        if self.submitted {
            STEPS.len()
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

    /// The transaction as the shared summary shows it.
    fn tx_view(&self) -> Option<TxSummary> {
        let tx = &self.data.transaction;
        self.summary.as_ref().map(|s| {
            TxSummary::from_psbt(s)
                .wallet(tx.wallet_name.clone())
                .txid(Some(tx.txid.clone()))
        })
    }

    /// Fingerprint of "your" key: the one you signed with, or the
    /// authorized device you picked.
    fn your_fingerprint(&self) -> Option<String> {
        self.signed_fp.clone().or_else(|| {
            self.selected
                .as_ref()
                .filter(|d| self.can_sign(d))
                .and_then(fingerprint)
        })
    }

    /// Who has signed and who is still needed, from what the session gives
    /// us: the signature slots, signatures already in the PSBT, the key you
    /// signed with and the threshold when the descriptor is unambiguous.
    fn roster(&self) -> Roster {
        let slots = &self.data.signature_slots;
        let in_psbt: Vec<String> = self
            .summary
            .as_ref()
            .map(|s| s.signed_fingerprints.clone())
            .unwrap_or_default();
        let you = self.your_fingerprint().map(|f| f.to_lowercase());
        let signed_by_you = self.signed_fp.as_ref().map(|f| f.to_lowercase());
        let signers = slots
            .iter()
            .map(|slot| {
                let fp = slot.fingerprint.to_lowercase();
                let device = self
                    .devices
                    .iter()
                    .find(|d| fingerprint(d).map(|f| f.to_lowercase()).as_deref() == Some(&fp));
                Signer {
                    fingerprint: slot.fingerprint.clone(),
                    name: device.map(|d| d.model.clone()),
                    detail: Some(slot.derivation_path.clone()),
                    signed: in_psbt.contains(&fp) || signed_by_you.as_deref() == Some(&fp),
                    is_you: you.as_deref() == Some(&fp),
                    sigvault: false,
                }
            })
            .collect();
        Roster {
            signers,
            threshold: threshold_from_descriptor(
                self.data.transaction.multipath_descriptor.as_deref(),
                slots.len(),
            ),
            note: Some(
                "Signatures given in other sessions show here once SigVault adds them to the transaction."
                    .into(),
            ),
        }
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
                let needs = match self.roster().threshold {
                    Some(k) => format!("Needs {k} of {n} signatures"),
                    None => format!(
                        "{n} authorized {}",
                        if n == 1 { "signer" } else { "signers" }
                    ),
                };
                (
                    vec![div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(needs)
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

    fn render_tx(&self, id: &'static str, title: &'static str) -> AnyElement {
        match self.tx_view() {
            Some(s) => section(title).child(tx_summary(id, &s)).into_any_element(),
            None => section(title)
                .child(
                    card().px_4().py_3().child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(theme().ink_2)
                            .child("This transaction couldn't be decoded here. Check the outputs and fee on the device screen before you approve."),
                    ),
                )
                .into_any_element(),
        }
    }

    fn render_roster(&self) -> AnyElement {
        section("Signers")
            .description("Who has signed this transaction and who is still needed.")
            .child(signer_roster(&self.roster()))
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
                    .child(copy_button("copy-psbt", "Copy PSBT", psbt.clone()))
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
                        .bg(t.inset)
                        .px_4()
                        .py_3()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .text_color(t.ink_2)
                        .child(psbt),
                )
            })
            .into_any_element()
    }

    fn render_review(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut v = Vec::new();
        if self.summary.is_none() {
            v.push(
                alert(Tone::Warning, "This PSBT couldn't be decoded for review. Check it in another tool before signing.")
                    .into_any_element(),
            );
        }
        v.push(self.render_tx("review-tx", "What you're signing"));
        v.push(self.render_roster());
        v.push(
            section("Details")
                .child(self.render_psbt(cx))
                .into_any_element(),
        );
        v
    }

    /// "No authorized signers found" with the connected devices named and
    /// a next step.
    fn no_signer_message(&self) -> String {
        let connected: Vec<String> = self
            .devices
            .iter()
            .map(|d| match fingerprint(d) {
                Some(fp) => format!("{} ({fp})", d.model),
                None => d.model.clone(),
            })
            .collect();
        let expected: Vec<String> = self
            .roster()
            .signers
            .iter()
            .map(|s| s.fingerprint.clone())
            .collect();
        let expected = match expected.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [init @ .., last] => format!("{} or {last}", init.join(", ")),
        };
        format!(
            "None of the connected devices holds a key for this transaction. Connected: {}. Needed: a device with key {expected}. Plug in that device, unlock it and open the Bitcoin app, then scan again.",
            connected.join(", ")
        )
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
                                .child("Plug in a hardware wallet holding one of the signing keys below, unlock it and open the Bitcoin app."),
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
                s.child(alert(Tone::Destructive, self.no_signer_message()))
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
                    .border_color(t.brand)
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
                                    "Check that the device shows the same recipient, amount and fee as below, then approve.",
                                ),
                            ),
                    )
                    .into_any_element(),
            );
        }
        v.push(self.render_tx("sign-tx-summary", "You're approving"));
        v.push(self.render_roster());
        v
    }

    fn render_done(&self) -> Vec<AnyElement> {
        let t = theme();
        let roster = self.roster();
        let headline = roster.headline();
        let body = match (roster.still_needed(), roster.waiting_on()) {
            (Some(0), _) => format!(
                "{headline}. SigVault can broadcast the transaction now. You can leave this session."
            ),
            (Some(_), Some(waiting)) => format!(
                "{headline} — {waiting}. SigVault broadcasts the transaction once enough co-signers have signed. You can leave this session."
            ),
            _ => "Your signature is in. SigVault broadcasts the transaction once enough co-signers have signed. You can leave this session.".into(),
        };
        vec![
            card_message("circle-check", Some(t.success), "Signature submitted", body)
                .into_any_element(),
            self.render_tx("done-tx", "What you signed"),
            self.render_roster(),
        ]
    }
}

impl Render for TransactionSigning {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.submitted {
            self.render_done()
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

#[cfg(test)]
mod tests {
    use super::threshold_from_descriptor as th;

    #[test]
    fn threshold_only_when_unambiguous() {
        let ms = "wsh(sortedmulti(2,[3f8a91c2/48'/1'/0'/2']tpubA/<0;1>/*,[b71d04e9/48'/1'/0'/2']tpubB/<0;1>/*,[5c2e88a0/48'/1'/0'/2']tpubC/<0;1>/*))#abc";
        assert_eq!(th(Some(ms), 3), Some(2));
        // The session lists a different number of signers: don't guess.
        assert_eq!(th(Some(ms), 2), None);
        assert_eq!(th(Some("tr(K,multi_a(1,[aa/1]A,[bb/2]B))"), 2), Some(1));
        assert_eq!(
            th(Some("wpkh([3f8a91c2/84'/1'/0']tpubA/<0;1>/*)"), 1),
            Some(1)
        );
        // Timelocked recovery paths: which path is spent isn't known here.
        let liana = "wsh(or_d(multi(2,A,B),and_v(v:pk(C),older(52560))))";
        assert_eq!(th(Some(liana), 2), None);
        assert_eq!(th(Some("wsh(or_i(multi(2,A,B),multi(1,C,D)))"), 2), None);
        assert_eq!(th(None, 2), None);
    }
}
