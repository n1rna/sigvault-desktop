//! Send wizard (`pages/local/SendScreen.tsx`, QBL-229 + QBL-234).
//!
//! Steps on top of the QBL-219 PSBT pipeline:
//!   1. Details — recipient address, amount, fee rate
//!   2. Spending path — pick one (skipped for single-path descriptors)
//!   3. Review & sign — the built transaction (every output incl. change,
//!      absolute fee, total), then sign: passphrase for a key on this
//!      computer, a connected hardware wallet, or "Sign elsewhere" (PSBT
//!      file / animated QR round-trip)
//!   4. Broadcast — the final, fully signed transaction (txid, outputs,
//!      change, fee); nothing leaves the computer until Broadcast is
//!      pressed, and Discard backs out
//!   5. Done — the broadcast txid with copy
//!
//! Signing never broadcasts: each signer's PSBT goes through
//! `cmd_local_finalize_psbt`, which says whether it is complete. A PSBT that
//! still needs signatures stays on step 3 with the new signatures kept, so
//! another key can add its own.

mod devices;
mod qr_modals;

use std::collections::BTreeMap;

use gpui::{
    div, prelude::*, px, Context, Entity, FocusHandle, FontWeight, KeyDownEvent, PathPromptOptions,
    SharedString, Window,
};
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::local_wallet::commands::{
    cmd_local_broadcast_psbt, cmd_local_build_psbt, cmd_local_finalize_psbt, cmd_local_get_balance,
    cmd_local_get_wallet_details, cmd_local_list_spending_paths, cmd_local_list_wallets,
    cmd_local_sign_psbt_hardware, cmd_local_sign_psbt_software, BroadcastPsbtRequest,
    BuildPsbtRequest, FinalizePsbtRequest, FinalizePsbtResponse, PsbtRecipient,
    SignPsbtHardwareRequest, SignPsbtSoftwareRequest,
};
use sigvault_core::State;

use self::devices::{DevicePicker, DeviceSelected};
use self::qr_modals::{segmented, AnimatedQr, QrImport};
use crate::e2e::{E2eClick, Probe};
use crate::pages::cloud::psbt_summary::{hrp_for, summarize, PsbtSummary};
use crate::psbt::file::{read_psbt_file, write_psbt_file};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::{navigate, toast};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, button, card, divider, field, footer_bar, format_btc, format_sat, icon, page_frame,
    section, stepper, tx_summary, InputEvent, TextInput, Tone, TxSummary, FORM_WIDTH,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Compose,
    Path,
    Confirm,
    Broadcast,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignMode {
    /// Passphrase-protected key stored on this computer.
    ThisComputer,
    Connected,
    External,
}

/// A fully signed transaction waiting for the Broadcast step.
#[derive(Clone)]
struct SignedTx {
    /// Signed (not finalized) PSBT, as `cmd_local_broadcast_psbt` takes it.
    psbt_base64: String,
    info: FinalizePsbtResponse,
}

/// Where a failure happened, for [`plain_error`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Build,
    Sign,
    SignDevice,
    Import,
    Broadcast,
}

/// An error in plain words, with the raw message kept as a secondary
/// detail.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlainError {
    pub(crate) headline: SharedString,
    pub(crate) detail: Option<SharedString>,
}

impl PlainError {
    fn plain(headline: impl Into<SharedString>) -> Self {
        Self {
            headline: headline.into(),
            detail: None,
        }
    }
}

/// Turn a core error string (`build_tx: …`, `hw sign: …`, …) into a
/// sentence a person can act on.
pub(crate) fn plain_error(stage: Stage, raw: &str) -> PlainError {
    let r = raw.to_lowercase();
    let headline = if r.contains("not unlocked") || r.contains("notunlocked") {
        "This wallet is locked. Unlock it from the wallet page, then try again."
    } else {
        match stage {
            Stage::Build if r.contains("insufficient") => {
                "Not enough funds to send this amount plus the network fee."
            }
            Stage::Build if r.contains("invalid address") => "That recipient address isn't valid.",
            Stage::Build if r.contains("not on") && r.contains("network") => {
                "That address is for a different network than this wallet."
            }
            Stage::Build if r.contains("dust") => "This amount is too small to send.",
            Stage::Build => "The transaction couldn't be built.",
            Stage::Sign if r.contains("wrong passphrase") || r.contains("decryption failed") => {
                "That passphrase didn't unlock this wallet's key."
            }
            Stage::Sign if r.contains("no on-device seed") => {
                "This wallet has no key on this computer. Sign with a hardware wallet or elsewhere."
            }
            Stage::Sign => "The transaction couldn't be signed on this computer.",
            Stage::SignDevice if r.contains("not part of the wallet") => {
                "This device doesn't hold a key for this wallet. Connect the right device and scan again."
            }
            Stage::SignDevice if r.contains("not connected") => {
                "The device was disconnected. Reconnect it and scan again."
            }
            Stage::SignDevice => {
                "The device didn't sign. Check it's unlocked with the Bitcoin app open, then approve on the device."
            }
            Stage::Import => "That isn't a signed PSBT this wallet can read.",
            Stage::Broadcast if r.contains("couldn't reach") || r.contains("electrum connect") => {
                "Couldn't reach the server. Check the server in Settings and your connection, then try again."
            }
            Stage::Broadcast
                if r.contains("min relay fee") || r.contains("insufficient fee") =>
            {
                "The network rejected the fee as too low. Discard and send again with a higher fee rate."
            }
            Stage::Broadcast
                if r.contains("missingorspent")
                    || r.contains("missing-inputs")
                    || r.contains("mempool-conflict") =>
            {
                "Some coins in this transaction were already spent. Discard it and send again."
            }
            Stage::Broadcast => "The network didn't accept the transaction. Nothing was sent.",
        }
    };
    PlainError {
        headline: headline.into(),
        detail: (!raw.trim().is_empty()).then(|| raw.trim().to_string().into()),
    }
}

/// What the wizard needs from `LocalWalletSummary`.
struct WalletInfo {
    name: String,
    network: String,
    has_hot_keys: bool,
}

/// `SpendingPathDto` (the core type is serialize-only).
#[derive(Clone)]
struct SpendingPath {
    id: String,
    label: String,
    description: Option<String>,
    threshold: u32,
    fingerprints: Vec<String>,
    policy_path: Option<BTreeMap<String, Vec<u32>>>,
}

/// Wallet descriptor info used to build a `WalletConfig` for HW discovery
/// / unlock, so policy registration (BitBox, Jade) and HMAC capture
/// (Ledger) happen on the right device the first time the user signs.
struct Details {
    name: String,
    external_descriptor: String,
    /// Receive + change as one `<0;1>` descriptor: the form devices
    /// register as a wallet policy (as cloud signing already sends).
    multipath_descriptor: Option<String>,
}

const SAT_PER_BTC: f64 = 100_000_000.0;

/// `btcToSat`: `None` for blank / non-numeric / non-positive input.
fn btc_to_sat(btc: &str) -> Option<i64> {
    let trimmed = btc.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: f64 = trimmed.parse().ok()?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return None;
    }
    Some((parsed * SAT_PER_BTC).round() as i64)
}

fn parse_fee_rate(s: &str) -> Option<f32> {
    let t = s.trim();
    // JS `Number("")` is 0, which the `> 0` check rejects anyway.
    let v: f32 = if t.is_empty() { 0.0 } else { t.parse().ok()? };
    (v.is_finite() && v > 0.0).then_some(v)
}

fn primary_derivation_path(network: &str) -> String {
    let coin = if network == "bitcoin" { "0'" } else { "1'" };
    format!("m/84'/{coin}/0'")
}

pub struct SendPage {
    wallet_id: String,
    step: Step,
    wallet: Option<WalletInfo>,
    balance_confirmed: Option<u64>,
    paths: Option<Vec<SpendingPath>>,
    selected_path_id: Option<String>,
    details: Option<Details>,

    recipient: Entity<TextInput>,
    amount: Entity<TextInput>,
    fee_rate: Entity<TextInput>,
    passphrase: Entity<TextInput>,

    /// The transaction being signed; collects signatures as signers add
    /// them.
    psbt_base64: Option<String>,
    /// Decoded `psbt_base64`, for the review summary.
    review: Option<PsbtSummary>,
    /// Fully signed, waiting for Broadcast.
    signed: Option<SignedTx>,
    busy: bool,
    error: Option<PlainError>,
    /// Non-error status on the sign step (e.g. more signatures needed).
    notice: Option<SharedString>,

    txid: Option<String>,

    sign_mode: SignMode,
    device_picker: Option<Entity<DevicePicker>>,

    show_qr: Option<AnimatedQr>,
    scan_qr: Option<QrImport>,
    focus: FocusHandle,
}

impl SendPage {
    pub fn new(wallet_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let recipient = cx.new(|cx| TextInput::new(window, cx).mono().placeholder("bcrt1q…"));
        let amount = cx.new(|cx| TextInput::new(window, cx).mono().placeholder("0.001"));
        let fee_rate = cx.new(|cx| {
            TextInput::new(window, cx)
                .mono()
                .placeholder("2")
                .with_text("2")
        });
        let passphrase = cx.new(|cx| TextInput::new(window, cx).masked().label("Passphrase"));

        for input in [&recipient, &amount, &fee_rate] {
            cx.subscribe(input, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Changed => cx.notify(),
                InputEvent::Submit => {
                    if this.compose_ready(cx) && !this.busy {
                        this.advance_from_compose(cx);
                    }
                }
            })
            .detach();
        }
        cx.subscribe(&passphrase, |this, input, ev: &InputEvent, cx| match ev {
            InputEvent::Changed => cx.notify(),
            InputEvent::Submit => {
                if !input.read(cx).text().is_empty() && !this.busy {
                    this.sign_software(cx);
                }
            }
        })
        .detach();

        #[allow(unused_mut)]
        let mut this = Self {
            wallet_id,
            step: Step::Compose,
            wallet: None,
            balance_confirmed: None,
            paths: None,
            selected_path_id: None,
            details: None,
            recipient,
            amount,
            fee_rate,
            passphrase,
            psbt_base64: None,
            review: None,
            signed: None,
            busy: false,
            error: None,
            notice: None,
            txid: None,
            sign_mode: SignMode::ThisComputer,
            device_picker: None,
            show_qr: None,
            scan_qr: None,
            focus: cx.focus_handle(),
        };
        this.load(cx);
        this.recipient.read(cx).focus(window);
        #[cfg(debug_assertions)]
        if let Ok(stage) = std::env::var("SIGVAULT_DEV_SEND") {
            this.apply_dev_send(&stage, cx);
        }
        this
    }

    /// Debug screenshot hook: `SIGVAULT_DEV_SEND=review|partial|broadcast|done`
    /// shows that step with the screenshot fixture PSBT (2 inputs → a
    /// recipient + change), no funded wallet needed.
    #[cfg(debug_assertions)]
    fn apply_dev_send(&mut self, stage: &str, cx: &mut Context<Self>) {
        log::warn!("SIGVAULT_DEV_SEND={stage}: fixture send state (debug build)");
        if self.wallet.is_none() {
            self.wallet = Some(WalletInfo {
                name: "Family vault".into(),
                network: "regtest".into(),
                has_hot_keys: true,
            });
        }
        self.recipient.update(cx, |r, cx| {
            r.set_text(
                "bcrt1qtfl6fj0wzvu9mq48enc3vwmqsk4vlaqe8e3c3twj7uwyze5tkr2sqzvjqh".to_string(),
                cx,
            )
        });
        self.amount
            .update(cx, |a, cx| a.set_text("0.25".to_string(), cx));
        self.set_psbt(crate::dev_fixtures::fixture_psbt());
        self.step = Step::Confirm;
        match stage {
            "partial" => {
                self.notice = Some(Self::more_signatures_notice());
            }
            "broadcast" | "done" => {
                let fee = self.review.as_ref().and_then(|r| r.fee);
                self.signed = Some(SignedTx {
                    psbt_base64: self.psbt_base64.clone().unwrap_or_default(),
                    info: FinalizePsbtResponse {
                        complete: true,
                        txid: Some(
                            "6a1f0c9e4b7d2a8f3e5c1b9d7a0e4f2c8b6d3a1e9f7c5b2d0a8e6c4f1b3d5a79"
                                .into(),
                        ),
                        vsize: Some(253),
                        fee_sat: fee,
                        detail: None,
                    },
                });
                self.step = Step::Broadcast;
                if stage == "done" {
                    self.txid = self.signed.as_ref().and_then(|s| s.info.txid.clone());
                    self.step = Step::Done;
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Mount effect: wallets + balance + spending paths + details, all or
    /// nothing (`Promise.all`); failure is non-fatal — the wizard still
    /// works, just shows fewer hints.
    fn load(&self, cx: &mut Context<Self>) {
        let id = self.wallet_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let s = State::new(&st);
            let wallets = cmd_local_list_wallets(app.clone(), s).await?;
            let bal = cmd_local_get_balance(app.clone(), State::new(&st), id.clone()).await?;
            let sp =
                cmd_local_list_spending_paths(app.clone(), State::new(&st), id.clone()).await?;
            let dt = cmd_local_get_wallet_details(app, State::new(&st), id.clone()).await?;
            let wallet = wallets
                .into_iter()
                .find(|w| w.id.as_str() == id)
                .map(|w| WalletInfo {
                    name: w.name,
                    network: w.network,
                    has_hot_keys: w.has_hot_keys,
                });
            let paths: Vec<SpendingPath> = sp
                .into_iter()
                .map(|p| SpendingPath {
                    id: p.id,
                    label: p.label,
                    description: p.description,
                    threshold: p.threshold,
                    fingerprints: p.fingerprints,
                    policy_path: p.policy_path,
                })
                .collect();
            let details = Details {
                name: dt.name,
                external_descriptor: dt.external_descriptor,
                multipath_descriptor: dt.multipath_descriptor,
            };
            Ok((wallet, bal.confirmed_sat, paths, details))
        });
        cx.spawn(async move |this, cx| {
            let Ok((wallet, confirmed, paths, details)) = task.await else {
                return;
            };
            this.update(cx, |this, cx| {
                this.wallet = wallet;
                this.balance_confirmed = Some(confirmed);
                if paths.len() == 1 {
                    this.selected_path_id = Some(paths[0].id.clone());
                }
                this.paths = Some(paths);
                this.details = Some(details);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn wallet_config(&self) -> Option<WalletConfigInput> {
        self.details.as_ref().map(|d| WalletConfigInput {
            name: Some(d.name.clone()),
            descriptor: Some(
                d.multipath_descriptor
                    .clone()
                    .unwrap_or_else(|| d.external_descriptor.clone()),
            ),
            hmac: None,
            ledger_hmacs: None,
        })
    }

    fn show_path_step(&self) -> bool {
        self.paths.as_ref().map_or(0, Vec::len) > 1
    }

    fn sat(&self, cx: &Context<Self>) -> Option<i64> {
        btc_to_sat(self.amount.read(cx).text())
    }

    fn compose_ready(&self, cx: &Context<Self>) -> bool {
        !self.recipient.read(cx).text().trim().is_empty()
            && self.sat(cx).is_some()
            && parse_fee_rate(self.fee_rate.read(cx).text()).is_some()
    }

    fn set_error(&mut self, e: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.error = Some(PlainError::plain(e));
        cx.notify();
    }

    /// Network of the wallet being spent from, for address encoding.
    fn network(&self) -> String {
        self.wallet
            .as_ref()
            .map_or("regtest".to_string(), |w| w.network.clone())
    }

    /// Replace the working PSBT and its decoded summary.
    fn set_psbt(&mut self, psbt: String) {
        let network = self.network();
        self.review = summarize(&psbt, hrp_for(Some(&network), None));
        self.psbt_base64 = Some(psbt);
    }

    fn summary_view(&self, psbt: Option<&PsbtSummary>) -> Option<TxSummary> {
        psbt.map(|s| TxSummary::from_psbt(s).wallet(self.wallet.as_ref().map(|w| w.name.clone())))
    }

    fn validate_compose(&mut self, cx: &mut Context<Self>) -> Option<(u64, f32)> {
        self.error = None;
        let sat = match self.sat(cx) {
            Some(s) if s > 0 => s as u64,
            _ => {
                self.set_error("Enter a valid amount in BTC.", cx);
                return None;
            }
        };
        let Some(fr) = parse_fee_rate(self.fee_rate.read(cx).text()) else {
            self.set_error("Fee rate must be a positive number.", cx);
            return None;
        };
        if self.recipient.read(cx).text().trim().is_empty() {
            self.set_error("Recipient address is required.", cx);
            return None;
        }
        Some((sat, fr))
    }

    /// Compose → next: skip the path step for single-path wallets. The PSBT
    /// is built lazily once a path is known.
    fn advance_from_compose(&mut self, cx: &mut Context<Self>) {
        if self.validate_compose(cx).is_none() {
            return;
        }
        if self.show_path_step() {
            self.step = Step::Path;
            cx.notify();
        } else {
            let pp = self
                .paths
                .as_ref()
                .and_then(|p| p.first())
                .and_then(|p| p.policy_path.clone());
            self.build_psbt_and_advance(pp, cx);
        }
    }

    fn submit_path(&mut self, cx: &mut Context<Self>) {
        let chosen = self
            .paths
            .as_ref()
            .and_then(|ps| {
                ps.iter()
                    .find(|p| Some(&p.id) == self.selected_path_id.as_ref())
            })
            .cloned();
        match chosen {
            Some(p) => self.build_psbt_and_advance(p.policy_path, cx),
            None => self.set_error("Pick a spending path to continue.", cx),
        }
    }

    fn build_psbt_and_advance(
        &mut self,
        policy_path: Option<BTreeMap<String, Vec<u32>>>,
        cx: &mut Context<Self>,
    ) {
        let Some((sat, fee_rate)) = self.validate_compose(cx) else {
            return;
        };
        self.busy = true;
        cx.notify();
        let request = BuildPsbtRequest {
            wallet_id: self.wallet_id.clone(),
            recipients: vec![PsbtRecipient {
                address: self.recipient.read(cx).text().trim().to_string(),
                amount_sat: sat,
            }],
            fee_rate_sat_vb: fee_rate,
            policy_path,
        };
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_build_psbt(app, State::new(&st), request).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) => {
                        this.set_psbt(r.psbt_base64);
                        this.enter_confirm(cx);
                    }
                    Err(e) => this.error = Some(plain_error(Stage::Build, &e)),
                }
                this.busy = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn enter_confirm(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Confirm;
        self.signed = None;
        self.notice = None;
        self.device_picker = None;
        if self.is_hardware_only() {
            self.sign_mode = SignMode::Connected;
            self.ensure_device_picker(cx);
        } else {
            self.sign_mode = SignMode::ThisComputer;
        }
    }

    fn is_hardware_only(&self) -> bool {
        self.wallet.as_ref().is_some_and(|w| !w.has_hot_keys)
    }

    fn ensure_device_picker(&mut self, cx: &mut Context<Self>) {
        if self.device_picker.is_some() {
            return;
        }
        let network = self
            .wallet
            .as_ref()
            .map_or("regtest".to_string(), |w| w.network.clone());
        let cfg = self.wallet_config();
        let picker = cx.new(|_| DevicePicker::new(cfg, primary_derivation_path(&network)));
        cx.subscribe(&picker, |this, _, ev: &DeviceSelected, cx| {
            this.sign_hardware(ev.device.id.clone(), cx);
        })
        .detach();
        self.device_picker = Some(picker);
    }

    fn set_sign_mode(&mut self, mode: SignMode, cx: &mut Context<Self>) {
        self.sign_mode = mode;
        // The TSX unmounts DeviceDiscovery when switching away → fresh state.
        self.device_picker = None;
        if mode == SignMode::Connected {
            self.ensure_device_picker(cx);
        }
        cx.notify();
    }

    fn back_from_confirm(&mut self, cx: &mut Context<Self>) {
        self.step = if self.show_path_step() {
            Step::Path
        } else {
            Step::Compose
        };
        self.clear_transaction(cx);
        cx.notify();
    }

    /// Forget the built / signed transaction (nothing was broadcast).
    fn clear_transaction(&mut self, cx: &mut Context<Self>) {
        self.psbt_base64 = None;
        self.review = None;
        self.signed = None;
        self.notice = None;
        self.passphrase.update(cx, |p, cx| p.clear(cx));
        self.device_picker = None;
        self.sign_mode = SignMode::ThisComputer;
        self.error = None;
    }

    /// Broadcast step → Discard: drop the signed transaction and go back to
    /// the details, which are kept.
    fn discard_signed(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Compose;
        self.clear_transaction(cx);
        toast(cx, "Signed transaction discarded. Nothing was sent.");
        cx.notify();
    }

    fn more_signatures_notice() -> SharedString {
        "Signature added. This transaction needs more signatures before it can be broadcast: \
         sign it with another key (Sign elsewhere, or another device), then bring it back here."
            .into()
    }

    /// Await a sign (or import) task that yields a signed PSBT plus what
    /// finalizing it would give. Complete → Broadcast step; otherwise the
    /// signatures are kept and the user can add more.
    fn await_signed(
        &mut self,
        task: impl std::future::Future<Output = Result<(String, FinalizePsbtResponse), String>>
            + 'static,
        stage: Stage,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.notice = None;
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((psbt, info)) => this.on_signed(psbt, info),
                    Err(e) => this.error = Some(plain_error(stage, &e)),
                }
                this.busy = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_signed(&mut self, psbt: String, info: FinalizePsbtResponse) {
        if info.complete {
            self.signed = Some(SignedTx {
                psbt_base64: psbt,
                info,
            });
            self.step = Step::Broadcast;
        } else {
            self.set_psbt(psbt);
            self.notice = Some(Self::more_signatures_notice());
        }
    }

    fn sign_software(&mut self, cx: &mut Context<Self>) {
        let passphrase = self.passphrase.read(cx).text().to_string();
        let Some(psbt) = self.psbt_base64.clone() else {
            return;
        };
        if passphrase.is_empty() {
            return;
        }
        let request = SignPsbtSoftwareRequest {
            wallet_id: self.wallet_id.clone(),
            psbt_base64: psbt,
            passphrase,
        };
        let task = backend(cx).call(move |app, st| async move {
            let signed = cmd_local_sign_psbt_software(app, State::new(&st), request).await?;
            finalize(&st, signed.psbt_base64).await
        });
        self.await_signed(task, Stage::Sign, cx);
    }

    fn sign_hardware(&mut self, device_id: String, cx: &mut Context<Self>) {
        let Some(psbt) = self.psbt_base64.clone() else {
            return;
        };
        let request = SignPsbtHardwareRequest {
            wallet_id: self.wallet_id.clone(),
            psbt_base64: psbt,
            device_id,
        };
        let task = backend(cx).call(move |app, st| async move {
            st.ensure_hw_manager_for_local().await;
            let signed = cmd_local_sign_psbt_hardware(app, State::new(&st), request).await?;
            finalize(&st, signed.psbt_base64).await
        });
        self.await_signed(task, Stage::SignDevice, cx);
    }

    /// A PSBT that came back from another signer (file import or QR scan).
    fn check_signed_psbt(&mut self, signed: String, cx: &mut Context<Self>) {
        let task = backend(cx).call(move |_app, st| async move { finalize(&st, signed).await });
        self.await_signed(task, Stage::Import, cx);
    }

    fn on_scanned_signed_psbt(&mut self, signed: String, cx: &mut Context<Self>) {
        self.scan_qr = None;
        self.check_signed_psbt(signed, cx);
    }

    /// Broadcast step → Broadcast.
    fn broadcast(&mut self, cx: &mut Context<Self>) {
        let Some(signed) = self.signed.clone() else {
            return;
        };
        let wallet_id = self.wallet_id.clone();
        self.error = None;
        self.busy = true;
        cx.notify();
        let task = backend(cx).call(move |app, st| async move {
            cmd_local_broadcast_psbt(
                app,
                State::new(&st),
                BroadcastPsbtRequest {
                    wallet_id,
                    psbt_base64: signed.psbt_base64,
                },
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) => {
                        this.txid = Some(r.txid);
                        this.step = Step::Done;
                    }
                    Err(e) => this.error = Some(plain_error(Stage::Broadcast, &e)),
                }
                this.busy = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Export the in-memory PSBT to a binary `.psbt` file for an
    /// air-gapped signer (Coldcard, SeedSigner, Krux, Passport, …).
    fn export_psbt_to_file(&mut self, cx: &mut Context<Self>) {
        let Some(psbt) = self.psbt_base64.clone() else {
            return;
        };
        self.error = None;
        cx.notify();
        let short: String = self.wallet_id.chars().take(8).collect();
        let name = format!(
            "sigvault-{}.psbt",
            if short.is_empty() {
                "wallet".into()
            } else {
                short
            }
        );
        let rx = cx.prompt_for_new_path(&super::widgets::default_save_dir(), Some(&name));
        cx.spawn(async move |this, cx| {
            let path = match rx.await {
                Ok(Ok(Some(p))) => p,
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| {
                        this.error = Some(PlainError {
                            headline: "The PSBT file couldn't be saved.".into(),
                            detail: Some(e.to_string().into()),
                        });
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                _ => return,
            };
            let res = cx
                .background_executor()
                .spawn(async move { write_psbt_file(&path, &psbt).map(|_| path) })
                .await;
            this.update(cx, |this, cx| match res {
                Ok(path) => toast(cx, format!("Saved {}", file_name(&path))),
                Err(e) => {
                    this.error = Some(PlainError {
                        headline: "The PSBT file couldn't be saved.".into(),
                        detail: Some(non_empty(e, "")),
                    });
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Open a `.psbt` file returned by an external signer. Complete → the
    /// Broadcast step; still missing signatures → its signatures are kept
    /// and the user can export again for the next signer.
    fn import_signed(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.busy = true;
        cx.notify();
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open signed PSBT".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| {
                        this.busy = false;
                        this.error = Some(plain_error(Stage::Import, &e.to_string()));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                _ => None,
            };
            let Some(path) = path else {
                this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                })
                .ok();
                return;
            };
            let res = cx
                .background_executor()
                .spawn(async move { read_psbt_file(&path) })
                .await;
            this.update(cx, |this, cx| match res {
                Ok(signed) => this.check_signed_psbt(signed, cx),
                Err(e) => {
                    this.busy = false;
                    this.error = Some(plain_error(Stage::Import, &e));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn back_to_wallet(&self, cx: &mut Context<Self>) {
        navigate(Page::LocalWallet(self.wallet_id.clone()), cx);
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if ev.keystroke.key == "escape" {
            if self.scan_qr.is_some() {
                self.close_scan_qr(cx);
            } else if self.show_qr.is_some() {
                self.close_show_qr(cx);
            }
        }
    }
}

fn non_empty(e: String, fallback: &str) -> SharedString {
    if e.trim().is_empty() {
        fallback.to_string().into()
    } else {
        e.into()
    }
}

/// `cmd_local_finalize_psbt` for a freshly signed PSBT: (psbt, verdict).
async fn finalize(
    st: &sigvault_core::ApplicationState,
    psbt_base64: String,
) -> Result<(String, FinalizePsbtResponse), String> {
    let info = cmd_local_finalize_psbt(
        State::new(st),
        FinalizePsbtRequest {
            psbt_base64: psbt_base64.clone(),
        },
    )
    .await?;
    Ok((psbt_base64, info))
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ───────────────────────── rendering ─────────────────────────

/// Review / Broadcast steps: room for full addresses beside the amounts.
const REVIEW_WIDTH: gpui::Pixels = px(820.);

/// Plain headline in the alert; the raw message underneath, secondary.
fn error_block(e: PlainError) -> gpui::Div {
    let t = theme();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(alert(Tone::Destructive, e.headline))
        .children(e.detail.map(|d| {
            div()
                .px(px(14.))
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(t.ink_2)
                .child(format!("Details: {d}"))
        }))
}

fn info_panel(content: impl IntoElement) -> gpui::Div {
    let t = theme();
    div()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(alpha(t.card, 0.4))
        .px_4()
        .py_3()
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(t.muted_foreground)
        .child(content)
}

/// Text with one emphasised (foreground, medium) span in the middle.
fn emphasised(before: &str, strong: &str, after: &str) -> gpui::StyledText {
    let t = theme();
    let start = before.len();
    gpui::StyledText::new(format!("{before}{strong}{after}")).with_highlights([(
        start..start + strong.len(),
        gpui::HighlightStyle {
            color: Some(t.foreground),
            font_weight: Some(FontWeight::MEDIUM),
            ..Default::default()
        },
    )])
}

/// Label/value row inside the review card.
fn review_row(label: &'static str, value: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_4()
        .px_4()
        .py(px(10.))
        .child(
            div()
                .w(px(96.))
                .flex_none()
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .line_height(px(18.))
                .child(value),
        )
}

/// "Sign" section heading with the signing-method switch.
fn sign_section(switch: gpui::AnyElement) -> impl IntoElement {
    section("Sign")
        .description(
            "Choose how to sign. Nothing is sent until you press Broadcast on the next step.",
        )
        .child(switch)
}

/// Small sub-heading inside a step body.
fn sub_label(text: &'static str) -> gpui::Div {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme().foreground)
        .child(text)
}

impl SendPage {
    fn step_labels(&self) -> (&'static [&'static str], usize) {
        let show_path = self.show_path_step();
        let labels: &'static [&'static str] = if show_path {
            &[
                "Details",
                "Spending path",
                "Review & sign",
                "Broadcast",
                "Done",
            ]
        } else {
            &["Details", "Review & sign", "Broadcast", "Done"]
        };
        let base = match self.step {
            Step::Compose | Step::Path => 0,
            Step::Confirm => 1,
            Step::Broadcast => 2,
            Step::Done => 3,
        };
        let ix = match (self.step, show_path) {
            // Past the last step: every step shows a check.
            (Step::Done, _) => labels.len(),
            (Step::Path, true) => 1,
            (Step::Compose | Step::Path, _) => 0,
            (_, true) => base + 1,
            (_, false) => base,
        };
        (labels, ix)
    }

    fn render_compose(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let sat = self.sat(cx);
        let exceeds = matches!((self.balance_confirmed, sat), (Some(b), Some(s)) if s > b as i64);
        let amount_hint = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .text_size(px(12.))
            .text_color(if exceeds {
                t.destructive
            } else {
                t.muted_foreground
            })
            .child(if exceeds {
                "Amount exceeds confirmed balance".to_string()
            } else {
                sat.map_or("Enter an amount in BTC".to_string(), format_sat)
            })
            .children(self.balance_confirmed.map(|b| {
                div()
                    .text_color(t.muted_foreground)
                    .child(format!("Available {}", format_btc(b as i64)))
            }));
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(field("Recipient address", self.recipient.clone(), None))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(field("Amount (BTC)", self.amount.clone(), None))
                    .child(amount_hint),
            )
            .child(field(
                "Fee rate (sat/vB)",
                self.fee_rate.clone(),
                Some("Rounded up to the nearest sat/vB.".into()),
            ))
    }

    fn render_path(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let paths = self.paths.clone().unwrap_or_default();
        let count = paths.len();
        let mut list = card().overflow_hidden();
        for (i, p) in paths.into_iter().enumerate() {
            if i > 0 {
                list = list.child(divider());
            }
            let active = self.selected_path_id.as_deref() == Some(p.id.as_str());
            let id = p.id.clone();
            list = list.child(
                div()
                    .id(("path", i))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .cursor_pointer()
                    .when(active, |d| d.bg(alpha(t.primary, 0.06)))
                    .when(!active, |d| d.hover(move |s| s.bg(alpha(t.muted, 0.5))))
                    .clickable(
                        Probe::new("option", p.label.clone()).selected(active),
                        cx.listener(move |this, _, _, cx| {
                            this.selected_path_id = Some(id.clone());
                            this.error = None;
                            cx.notify();
                        }),
                    )
                    .child(radio(active))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(t.foreground)
                                            .child(p.label.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(12.))
                                            .text_color(t.muted_foreground)
                                            .child(if p.fingerprints.is_empty() {
                                                "—".to_string()
                                            } else {
                                                format!(
                                                    "{}-of-{}",
                                                    p.threshold,
                                                    p.fingerprints.len()
                                                )
                                            }),
                                    ),
                            )
                            .children(p.description.clone().map(|desc| {
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(t.muted_foreground)
                                    .child(desc)
                            }))
                            .when(!p.fingerprints.is_empty(), |d| {
                                d.child(div().flex().flex_row().flex_wrap().gap(px(6.)).children(
                                    p.fingerprints.iter().map(|fp| {
                                        div()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(t.border)
                                            .bg(t.background)
                                            .px(px(6.))
                                            .py(px(1.))
                                            .font_family(FONT_MONO)
                                            .text_size(px(12.))
                                            .text_color(t.muted_foreground)
                                            .child(fp.clone())
                                    }),
                                ))
                            }),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(info_panel(emphasised(
                "This wallet has ",
                &format!("{count} spending paths"),
                ". Each imposes its own conditions (signer set, timelock). Pick the one this transaction should satisfy — the PSBT is built so that path's signers can finalize it.",
            )))
            .child(list)
    }

    /// The built transaction: the shared summary with every output, the
    /// absolute fee and the total, plus the spending path when chosen.
    fn render_review(&self) -> impl IntoElement {
        let t = theme();
        // The rate the built PSBT pays (its fee over the estimated signed
        // size), not the typed one, so Review matches Broadcast.
        let vsize = self
            .psbt_base64
            .as_deref()
            .zip(self.details.as_ref())
            .and_then(|(psbt, d)| {
                sigvault_core::local_wallet::commands::estimate_signed_vsize(
                    psbt,
                    &d.external_descriptor,
                )
            });
        let fee_rate = match (self.review.as_ref().and_then(|r| r.fee), vsize) {
            (Some(fee), Some(vs)) if vs > 0 => Some(fee as f64 / vs as f64),
            _ => None,
        };
        let path_label = self
            .show_path_step()
            .then(|| {
                self.paths
                    .as_ref()
                    .and_then(|ps| {
                        ps.iter()
                            .find(|p| Some(&p.id) == self.selected_path_id.as_ref())
                    })
                    .map(|p| p.label.clone())
            })
            .flatten();
        let body = match self.summary_view(self.review.as_ref()) {
            Some(s) => tx_summary("review-tx", &s.fee_rate(fee_rate)).into_any_element(),
            None => card()
                .px_4()
                .py_3()
                .text_size(px(13.))
                .line_height(px(19.))
                .text_color(t.ink_2)
                .child("This transaction couldn't be decoded for review.")
                .into_any_element(),
        };
        section("Review")
            .description(match path_label {
                Some(l) => {
                    format!("Check every output and the fee before you sign. Spending path: {l}.")
                }
                None => "Check every output and the fee before you sign.".to_string(),
            })
            .child(body)
    }

    fn sign_mode_switch(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mode = self.sign_mode;
        let first = if self.is_hardware_only() {
            ("Connected device", SignMode::Connected)
        } else {
            ("Key on this computer", SignMode::ThisComputer)
        };
        let (first_label, first_mode) = first;
        segmented(
            "sign-mode",
            [
                (first_label, mode == first_mode),
                ("Sign elsewhere", mode == SignMode::External),
            ],
            cx.listener(move |this, _, _, cx| this.set_sign_mode(first_mode, cx)),
            cx.listener(|this, _, _, cx| this.set_sign_mode(SignMode::External, cx)),
        )
        .into_any_element()
    }

    fn render_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.sign_mode;
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_review())
            .child(sign_section(self.sign_mode_switch(cx)))
            .when(mode == SignMode::ThisComputer, |d| {
                d.child(field(
                    "Passphrase",
                    self.passphrase.clone(),
                    Some("Re-enter the wallet passphrase to sign. The key is decrypted only for this transaction.".into()),
                ))
            })
            .when(mode == SignMode::External, |d| d.child(self.render_external(cx)))
    }

    fn render_hardware_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.sign_mode;
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_review())
            .child(sign_section(self.sign_mode_switch(cx)))
            .when(mode == SignMode::Connected, |d| {
                d.child(info_panel(emphasised(
                    "Connect and unlock your hardware wallet, then click ",
                    "Discover devices",
                    ". Check that the device shows the same recipient, amount and fee as above, then approve.",
                )))
                .children(self.device_picker.clone())
            })
            .when(mode == SignMode::External, |d| d.child(self.render_external(cx)))
    }

    /// "Sign elsewhere": PSBT out by file / QR, signed PSBT back in.
    fn render_external(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy;
        let outline = |id: &'static str, label: &'static str| {
            button(id, label).outline().full_width().disabled(busy)
        };
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(info_panel(
                "Send the PSBT to a signer on another machine (Coldcard, SeedSigner, Sparrow, Foundation Passport, Specter, Keystone, an air-gapped laptop, …). When it hands back a signed PSBT, import or scan it here to review it before broadcasting.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(sub_label("1. Send PSBT to signer"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(div().flex_1().child(
                                outline("save-psbt", "Save .psbt file")
                                    .icon("download")
                                    .on_click(cx.listener(|this, _, _, cx| this.export_psbt_to_file(cx))),
                            ))
                            .child(div().flex_1().child(
                                outline("show-qr", "Show QR")
                                    .icon("qr-code")
                                    .on_click(cx.listener(|this, _, window, cx| this.open_show_qr(window, cx))),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(sub_label("2. Receive signed PSBT"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(div().flex_1().child(
                                outline("import-psbt", "Import .psbt file")
                                    .icon("upload")
                                    .on_click(cx.listener(|this, _, _, cx| this.import_signed(cx))),
                            ))
                            .child(div().flex_1().child(
                                outline("scan-qr", "Scan QR")
                                    .icon("scan")
                                    .on_click(cx.listener(|this, _, window, cx| this.open_scan_qr(window, cx))),
                            )),
                    ),
            )
    }

    /// The signed transaction as the summary shows it: outputs from the
    /// PSBT, txid / fee / size from finalizing it.
    fn final_view(&self) -> Option<TxSummary> {
        let signed = self.signed.as_ref();
        let decoded =
            signed.and_then(|s| summarize(&s.psbt_base64, hrp_for(Some(&self.network()), None)));
        self.summary_view(decoded.as_ref()).map(|v| {
            let info = signed.map(|s| &s.info);
            let fee = info.and_then(|i| i.fee_sat).or(v.fee_sat);
            let rate = match (fee, info.and_then(|i| i.vsize)) {
                (Some(f), Some(vs)) if vs > 0 => Some(f as f64 / vs as f64),
                _ => None,
            };
            TxSummary { fee_sat: fee, ..v }
                .fee_rate(rate)
                .txid(info.and_then(|i| i.txid.clone()))
        })
    }

    /// Fully signed, not yet sent: the final transaction and its id.
    fn render_broadcast(&self) -> impl IntoElement {
        let view = self.final_view();
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(info_panel(emphasised(
                "Signed and ready. ",
                "Nothing has been sent yet",
                ": check the final transaction, then broadcast it to the network, or discard it.",
            )))
            .child(section("Final transaction").children(view.map(|v| tx_summary("final-tx", &v))))
    }

    fn render_done(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let txid = self.txid.clone().unwrap_or_default();
        card()
            .p_5()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .size(px(32.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(alpha(t.success, 0.12))
                            .child(icon("check").size(px(16.)).text_color(t.success)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Transaction broadcast"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(19.))
                                    .text_color(t.muted_foreground)
                                    .child("It's in the mempool now and will appear in the wallet's activity after the next sync."),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(px(12.)).text_color(t.muted_foreground).child("Transaction ID"))
                    // Grouped like the Broadcast step, so the two can be compared.
                    .child(crate::ui::tx_summary::data_with_copy(
                        "copy-txid",
                        "Copy transaction ID",
                        txid,
                        t.ink,
                    )),
            )
            .children(self.final_view().map(|v| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_size(px(12.)).text_color(t.muted_foreground).child("What was sent"))
                    .child(tx_summary("done-tx", &TxSummary { txid: None, ..v }))
            }))
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> gpui::Div {
        let busy = self.busy;
        let back = |id: &'static str| button(id, "Back").ghost().icon("arrow-left").disabled(busy);
        let cancel = button("cancel", "Cancel")
            .ghost()
            .disabled(busy)
            .on_click(cx.listener(|this, _, _, cx| this.back_to_wallet(cx)))
            .into_any_element();
        match self.step {
            Step::Compose => footer_bar(
                vec![],
                vec![
                    cancel,
                    button(
                        "compose-continue",
                        if busy { "Building…" } else { "Continue" },
                    )
                    .loading(busy)
                    .disabled(!self.compose_ready(cx))
                    .when(!busy, |b| b.icon_right("arrow-right"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.compose_ready(cx) && !this.busy {
                            this.advance_from_compose(cx);
                        }
                    }))
                    .into_any_element(),
                ],
            ),
            Step::Path => footer_bar(
                vec![back("path-back")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.step = Step::Compose;
                        this.error = None;
                        cx.notify();
                    }))
                    .into_any_element()],
                vec![
                    cancel,
                    button(
                        "path-build",
                        if busy {
                            "Building…"
                        } else {
                            "Build transaction"
                        },
                    )
                    .loading(busy)
                    .disabled(self.selected_path_id.is_none())
                    .when(!busy, |b| b.icon_right("arrow-right"))
                    .on_click(cx.listener(|this, _, _, cx| this.submit_path(cx)))
                    .into_any_element(),
                ],
            ),
            Step::Confirm => {
                let left = vec![back("confirm-back")
                    .on_click(cx.listener(|this, _, _, cx| this.back_from_confirm(cx)))
                    .into_any_element()];
                let right = if self.sign_mode == SignMode::ThisComputer {
                    let has_pass = !self.passphrase.read(cx).text().is_empty();
                    vec![
                        cancel,
                        button(
                            "sign",
                            if busy {
                                "Signing…"
                            } else {
                                "Sign transaction"
                            },
                        )
                        .loading(busy)
                        .disabled(!has_pass)
                        .when(!busy, |b| b.icon("pen"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.busy {
                                this.sign_software(cx);
                            }
                        }))
                        .into_any_element(),
                    ]
                } else {
                    let status = busy.then(|| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .text_color(theme().muted_foreground)
                            .child(crate::ui::spinner(px(12.)))
                            .child(if self.sign_mode == SignMode::Connected {
                                "Confirm on your device…"
                            } else {
                                "Checking the signed PSBT…"
                            })
                            .into_any_element()
                    });
                    status.into_iter().chain([cancel]).collect()
                };
                footer_bar(left, right)
            }
            Step::Broadcast => footer_bar(
                vec![button("discard", "Discard")
                    .ghost()
                    .icon("trash")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.discard_signed(cx)))
                    .into_any_element()],
                vec![button(
                    "broadcast",
                    if busy { "Broadcasting…" } else { "Broadcast" },
                )
                .loading(busy)
                .when(!busy, |b| b.icon("arrow-up-right"))
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.busy {
                        this.broadcast(cx);
                    }
                }))
                .into_any_element()],
            ),
            Step::Done => footer_bar(
                vec![],
                vec![button("finish", "Back to wallet")
                    .on_click(cx.listener(|this, _, _, cx| this.back_to_wallet(cx)))
                    .into_any_element()],
            ),
        }
    }
}

fn radio(selected: bool) -> impl IntoElement {
    let t = theme();
    div()
        .mt(px(2.))
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(if selected { t.primary } else { t.line_control })
        .when(selected, |d| {
            d.child(div().size(px(6.)).rounded_full().bg(t.primary))
        })
}

impl Render for SendPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hardware = self.is_hardware_only();
        if hardware && self.step == Step::Confirm && self.sign_mode == SignMode::Connected {
            // Wallet summary may land after the PSBT was built.
            self.ensure_device_picker(cx);
        }
        let body = match self.step {
            Step::Compose => self.render_compose(cx).into_any_element(),
            Step::Path if self.paths.is_some() => self.render_path(cx).into_any_element(),
            Step::Path => div().into_any_element(),
            Step::Confirm if hardware => self.render_hardware_confirm(cx).into_any_element(),
            Step::Confirm => self.render_confirm(cx).into_any_element(),
            Step::Broadcast => self.render_broadcast().into_any_element(),
            Step::Done if self.txid.is_some() => self.render_done(cx).into_any_element(),
            Step::Done => div().into_any_element(),
        };
        let name = self
            .wallet
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Wallet".into());
        let subtitle = match self.balance_confirmed {
            Some(b) => format!("From {name} · {} available", format_btc(b as i64)),
            None => format!("From {name}"),
        };
        let (labels, step_ix) = self.step_labels();
        let footer = self.render_footer(cx);

        div()
            .id("send-page")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .size_full()
            .child(
                page_frame("send")
                    .crumb("Wallets", Some(Page::LocalWallets))
                    .crumb(name, Some(Page::LocalWallet(self.wallet_id.clone())))
                    .crumb("Send", None)
                    .title("Send bitcoin")
                    .subtitle(subtitle)
                    .header_extra(stepper(labels, step_ix))
                    .footer(footer)
                    .child(
                        div()
                            .w_full()
                            .max_w(
                                if matches!(self.step, Step::Confirm | Step::Broadcast | Step::Done)
                                {
                                    REVIEW_WIDTH
                                } else {
                                    FORM_WIDTH
                                },
                            )
                            .flex()
                            .flex_col()
                            .gap_5()
                            .children(self.error.clone().map(error_block))
                            .children(self.notice.clone().map(|n| alert(Tone::Warning, n)))
                            .child(body),
                    ),
            )
            .children(self.render_show_qr(cx))
            .children(self.render_scan_qr(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btc_to_sat_matches_ts() {
        assert_eq!(btc_to_sat(""), None);
        assert_eq!(btc_to_sat("  "), None);
        assert_eq!(btc_to_sat("abc"), None);
        assert_eq!(btc_to_sat("-1"), None);
        assert_eq!(btc_to_sat("0"), None);
        assert_eq!(btc_to_sat("0.001"), Some(100_000));
        assert_eq!(btc_to_sat(" 1.5 "), Some(150_000_000));
        assert_eq!(btc_to_sat("0.000000001"), Some(0));
        assert_eq!(parse_fee_rate("2"), Some(2.0));
        assert_eq!(parse_fee_rate("0"), None);
        assert_eq!(parse_fee_rate("x"), None);
        assert_eq!(primary_derivation_path("bitcoin"), "m/84'/0'/0'");
        assert_eq!(primary_derivation_path("regtest"), "m/84'/1'/0'");
    }

    #[test]
    fn errors_read_plainly_with_the_raw_text_as_detail() {
        let e = plain_error(
            Stage::Build,
            "build_tx: Insufficient funds: 1000 sat available of 50000 sat needed",
        );
        assert_eq!(
            e.headline.as_ref(),
            "Not enough funds to send this amount plus the network fee."
        );
        assert!(e.detail.is_some_and(|d| d.starts_with("build_tx:")));
        let e = plain_error(
            Stage::Sign,
            "decryption failed (wrong passphrase or tampered file)",
        );
        assert_eq!(
            e.headline.as_ref(),
            "That passphrase didn't unlock this wallet's key."
        );
        let e = plain_error(
            Stage::Broadcast,
            "couldn't reach https://x: esplora https://x: timed out",
        );
        assert!(e.headline.starts_with("Couldn't reach the server"));
        let e = plain_error(Stage::Build, "wallet abc not unlocked");
        assert!(e.headline.starts_with("This wallet is locked"));
        for stage in [
            Stage::Build,
            Stage::Sign,
            Stage::SignDevice,
            Stage::Import,
            Stage::Broadcast,
        ] {
            let e = plain_error(stage, "something odd: xyz");
            assert!(!e.headline.contains(':'), "{stage:?}: {}", e.headline);
        }
    }
}
