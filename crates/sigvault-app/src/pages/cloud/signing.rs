//! `components/TransactionSigning.tsx` + `types/transaction.ts`: the
//! TRANSACTION_SIGNING session workflow — review the PSBT, find an
//! authorised hardware wallet, sign on-device and submit the signature.

use std::collections::{HashMap, HashSet};

use gpui::{div, prelude::*, px, Context, FontWeight, SharedString, Window};
use serde::Deserialize;
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_get_ledger_hmacs, cmd_sign_psbt,
    cmd_submit_transaction_signature, cmd_unlock_device,
};
use sigvault_core::hwi::{DiscoveredDevice, SignedPsbt};
use sigvault_core::State;

use super::common::main_window;
use super::device_creation::non_empty;
use super::devices::{device_list, fingerprint, is_supported, DeviceListHost, DeviceListProps};
use crate::runtime::backend;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, button, icon, spinner, Tone};

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
    pub fn new(data: TransactionSigningData, session_id: String) -> Self {
        Self {
            data,
            session_id,
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
    pub fn set_props(&mut self, data: TransactionSigningData, session_id: String, cx: &mut Context<Self>) {
        if self.data != data || self.session_id != session_id {
            self.data = data;
            self.session_id = session_id;
            cx.notify();
        }
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
        self.data.signature_slots.iter().map(|s| s.fingerprint.clone()).collect()
    }

    fn can_sign(&self, device: &DiscoveredDevice) -> bool {
        is_supported(device)
            && fingerprint(device).is_some_and(|fp| self.data.signature_slots.iter().any(|s| s.fingerprint == fp))
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
                        match serde_json::from_value::<Vec<DiscoveredDevice>>(r.data.unwrap_or_default()) {
                            Ok(devices) => {
                                let matching: Vec<_> =
                                    devices.iter().filter(|d| this.can_sign(d)).cloned().collect();
                                if matching.len() == 1 {
                                    this.selected = matching.into_iter().next();
                                }
                                this.devices = devices;
                            }
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => this.error = Some(non_empty(r.message, "Failed to discover devices").into()),
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
        let Some(slot) = self.data.signature_slots.iter().find(|s| s.fingerprint == fp).cloned() else {
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
                            this.update(cx, |this, cx| finish(this, Some(e.to_string()), cx)).ok();
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
                                if let Some(m) = r.data.and_then(|d| serde_json::from_value(d).ok()) {
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
                Ok(r) if r.success => None,
                Ok(r) => Some(non_empty(r.message, "Failed to submit signature")),
                Err(e) => Some(e),
            };
            this.update(cx, |this, cx| finish(this, err, cx)).ok();
        })
        .detach();
    }

    fn truncated_psbt(&self) -> String {
        let psbt = &self.data.transaction.psbt;
        let n = psbt.chars().count();
        if n > 100 {
            let start: String = psbt.chars().take(50).collect();
            let end: String = psbt.chars().skip(n - 50).collect();
            format!("{start}…{end}")
        } else {
            psbt.clone()
        }
    }

    fn render_transaction_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let tx = &self.data.transaction;
        let slots = &self.data.signature_slots;
        let small_label = |s: &'static str| div().text_size(px(11.)).text_color(t.muted_foreground).child(s);

        let header = div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap_4()
            .border_b_1()
            .border_color(alpha(t.border, 0.6))
            .px_6()
            .py_5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.muted_foreground)
                            .child("Transaction"),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(20.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child("Review and approve"),
                    )
                    .children(tx.wallet_name.clone().filter(|n| !n.is_empty()).map(|n| {
                        div()
                            .mt_1()
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(format!("wallet · {n}"))
                    })),
            )
            .child(
                div()
                    .size(px(44.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .border_1()
                    .border_color(alpha(t.primary, 0.3))
                    .bg(alpha(t.primary, 0.08))
                    .child(icon("cloud-file-text").size(px(20.)).text_color(t.primary)),
            );

        let body = div()
            .px_6()
            .py_5()
            .child(small_label("transaction id"))
            .child(
                div()
                    .mt_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(13.))
                            .text_color(t.foreground)
                            .child(short_txid(&tx.txid)),
                    )
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(10.))
                            .text_color(t.muted_foreground)
                            .child(format!("{} chars", tx.txid.chars().count())),
                    ),
            )
            .child(
                div()
                    .mt_5()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(small_label("psbt payload"))
                            .child(
                                div()
                                    .id("psbt-toggle")
                                    .cursor_pointer()
                                    .text_size(px(11.))
                                    .text_color(t.muted_foreground)
                                    .hover(move |s| s.text_color(t.foreground))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.psbt_expanded = !this.psbt_expanded;
                                        cx.notify();
                                    }))
                                    .child(if self.psbt_expanded { "collapse" } else { "expand" }),
                            ),
                    )
                    .child(
                        div()
                            .id("psbt-payload")
                            .mt_2()
                            .max_h(px(180.))
                            .overflow_y_scroll()
                            .rounded_md()
                            .border_1()
                            .border_color(alpha(t.border, 0.7))
                            .bg(t.background)
                            .p_3()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .line_height(px(18.))
                            .text_color(t.muted_foreground)
                            .child(if self.psbt_expanded { tx.psbt.clone() } else { self.truncated_psbt() }),
                    ),
            );

        let slots_el = div()
            .border_t_1()
            .border_color(alpha(t.border, 0.6))
            .px_6()
            .pt_5()
            .pb_6()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.muted_foreground)
                            .child("Required signatures"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(format!(
                                "{} {}",
                                slots.len(),
                                if slots.len() == 1 { "signer" } else { "signers" }
                            )),
                    ),
            )
            .child(
                div()
                    .mt_3()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.border)
                    .children(slots.iter().map(|slot| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .bg(t.card)
                            .px_4()
                            .py_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .child(div().size(px(8.)).rounded_full().bg(t.primary))
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t.foreground)
                                            .child(slot.fingerprint.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.))
                                    .text_color(t.muted_foreground)
                                    .child(slot.derivation_path.clone()),
                            )
                    })),
            );

        div()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .child(header)
            .child(body)
            .child(slots_el)
    }

    fn render_device_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let busy = self.discovering || self.signing || self.submitting;
        let signers = self.signing_fingerprints();
        let has_matching = self.devices.iter().any(|d| self.can_sign(d));
        let selected_can_sign = self.selected.as_ref().is_some_and(|d| self.can_sign(d));

        let (scan_icon, scan_label): (Option<&'static str>, &'static str) = if self.discovering {
            (None, "scanning")
        } else if self.devices.is_empty() {
            (Some("search"), "discover")
        } else {
            (Some("refresh"), "rescan")
        };
        let mut scan_btn = div()
            .id("scan")
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap_2()
            .h(px(40.))
            .px_4()
            .rounded_md()
            .border_1()
            .border_color(alpha(t.primary, 0.3))
            .bg(alpha(t.primary, 0.06))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.foreground)
            .map(|d| match scan_icon {
                None => d.child(spinner(px(14.))),
                Some(name) => d.child(icon(name).size(px(14.)).text_color(t.foreground)),
            })
            .child(scan_label);
        if busy {
            scan_btn = scan_btn.opacity(0.6).cursor_not_allowed();
        } else {
            scan_btn = scan_btn
                .cursor_pointer()
                .hover(move |s| s.border_color(alpha(t.primary, 0.6)).bg(alpha(t.primary, 0.12)))
                .on_click(cx.listener(|this, _, _, cx| this.discover(cx)));
        }

        let header = div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap_4()
            .border_b_1()
            .border_color(alpha(t.border, 0.6))
            .px_6()
            .py_5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.muted_foreground)
                            .child("Hardware wallet"),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(20.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child("Sign with device"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("Connect your hardware wallet and approve on-device."),
                    ),
            )
            .child(scan_btn);

        let mut body = div().px_6().py_5();
        if self.devices.is_empty() && !self.discovering {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .py_10()
                    .child(
                        div()
                            .size(px(56.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_dashed()
                            .border_color(t.border)
                            .bg(t.background)
                            .child(
                                icon("cloud-hw-wallet")
                                    .size(px(24.))
                                    .text_color(alpha(t.muted_foreground, 0.5)),
                            ),
                    )
                    .child(
                        div()
                            .mt_5()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child("No devices scanned yet"),
                    )
                    .child(
                        div()
                            .mt(px(6.))
                            .max_w(px(280.))
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .justify_center()
                            .gap_1()
                            .text_size(px(12.))
                            .line_height(px(19.))
                            .text_color(t.muted_foreground)
                            .child("Plug in your hardware wallet and click")
                            .child(div().font_family(FONT_MONO).child("DISCOVER."))
                    ),
            );
        }
        if !self.devices.is_empty() {
            if !has_matching {
                let expected = self
                    .data
                    .signature_slots
                    .iter()
                    .map(|s| s.fingerprint.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                body = body.child(
                    div()
                        .mb_4()
                        .child(alert(Tone::Destructive, format!("No authorized signers found. Expected: {expected}"))),
                );
            }
            let devices = self.devices.clone();
            let selected_id = self.selected.as_ref().map(|d| d.id.clone());
            let unlocking_id = self.unlocking_id.clone();
            body = body.child(device_list(
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
            ));
        }

        let footer = selected_can_sign.then(|| {
            let label = if self.signing {
                "Awaiting device approval…"
            } else if self.submitting {
                "Submitting signature…"
            } else {
                "Sign transaction"
            };
            let busy = self.signing || self.submitting;
            div()
                .border_t_1()
                .border_color(alpha(t.border, 0.6))
                .bg(alpha(t.primary, 0.03))
                .px_6()
                .py_6()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_4()
                        .child(
                            div().px_6().child(
                                button("sign-tx", label)
                                    .large()
                                    .loading(busy)
                                    .when(!busy, |b| b.icon_right("arrow-right"))
                                    .on_click(cx.listener(|this, _, _, cx| this.sign(cx))),
                            ),
                        )
                        .child(
                            div()
                                .text_center()
                                .text_size(px(11.))
                                .text_color(t.muted_foreground)
                                .child("confirm on device display"),
                        ),
                )
        });

        div()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .child(header)
            .child(body)
            .children(footer)
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
                        match serde_json::from_value::<DiscoveredDevice>(r.data.unwrap_or_default()) {
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
                    Ok(r) => this.error = Some(non_empty(r.message, "Failed to unlock device").into()),
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

impl Render for TransactionSigning {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(self.render_transaction_card(cx))
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .child(self.render_device_card(cx))
    }
}
