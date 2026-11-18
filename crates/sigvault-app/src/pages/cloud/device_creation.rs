//! `components/DeviceCreationSession.tsx` + `components/DeviceDiscovery.tsx`:
//! the DEVICE_REGISTRATION session workflow — discover / unlock a hardware
//! wallet, extract its xpub and submit it to the session websocket.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_get_device_xpub, cmd_submit_device_registration,
    cmd_unlock_device,
};
use sigvault_core::hwi::{DeviceInfo, DiscoveredDevice};
use sigvault_core::State;

use super::common::{card_message, dev_step, main_window, network_badge, non_empty, review_row};
use super::devices::{device_list, fingerprint, is_supported, DeviceListHost, DeviceListProps};
use crate::runtime::backend;
use crate::state::store;
use crate::theme::{theme, FONT_MONO};
use crate::ui::{alert, button, card, divider, section, spinner, Tone};

/// Stepper labels for the registration flow.
pub const STEPS: [&str; 3] = ["Connect device", "Share public key", "Done"];

/// State of the embedded `DeviceDiscovery` component. Reset whenever the
/// React tree would have unmounted it (while submitting).
#[derive(Default)]
struct Discovery {
    discovering: bool,
    devices: Vec<DiscoveredDevice>,
    selected: Option<DiscoveredDevice>,
    extracting: bool,
    unlocking_id: Option<String>,
    error: Option<SharedString>,
}

pub struct DeviceCreationSession {
    network: String,
    derivation_path: String,
    session_id: String,
    wallet_config: Option<WalletConfigInput>,
    submitting: bool,
    /// The registration was accepted; the backend advances the session.
    submitted: bool,
    error: Option<SharedString>,
    discovery: Discovery,
}

fn clear_activity_log(cx: &mut gpui::App) {
    store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
}

impl DeviceCreationSession {
    pub fn new(network: String, derivation_path: String, session_id: String) -> Self {
        Self {
            network,
            derivation_path,
            session_id,
            wallet_config: None,
            submitting: false,
            submitted: false,
            error: None,
            discovery: Discovery::default(),
        }
    }

    /// Prop update from the parent page.
    pub fn set_props(
        &mut self,
        network: String,
        derivation_path: String,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.network != network || self.derivation_path != derivation_path || self.session_id != session_id {
            self.network = network;
            self.derivation_path = derivation_path;
            self.session_id = session_id;
            cx.notify();
        }
    }

    // ── DeviceDiscovery ──

    fn discover(&mut self, cx: &mut Context<Self>) {
        self.discovery.discovering = true;
        self.discovery.error = None;
        clear_activity_log(cx);
        cx.notify();
        let cfg = self.wallet_config.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_discover_hardware_wallets(app, window, State::new(&st), cfg).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                let d = &mut this.discovery;
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<Vec<DiscoveredDevice>>(r.data.unwrap_or_default()) {
                            Ok(devices) => {
                                if let Some(sel) = &d.selected {
                                    if !devices.iter().any(|x| x.id == sel.id) {
                                        d.selected = None;
                                    }
                                }
                                if d.selected.is_none() && dev_step().as_deref() == Some("select") {
                                    d.selected = devices.iter().find(|x| is_supported(x)).cloned();
                                }
                                d.devices = devices;
                            }
                            Err(e) => d.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        d.error = Some(non_empty(r.message, "Failed to discover devices").into())
                    }
                    Err(e) => d.error = Some(e.into()),
                }
                d.discovering = false;
                clear_activity_log(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn extract_info(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.discovery.selected.clone().filter(is_supported) else {
            return;
        };
        let Some(fp) = fingerprint(&selected) else {
            return;
        };
        self.discovery.extracting = true;
        self.discovery.error = None;
        cx.notify();
        let path = self.derivation_path.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_get_device_xpub(window, State::new(&st), fp, path).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.discovery.extracting = false;
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<DeviceInfo>(r.data.unwrap_or_default()) {
                            Ok(info) => this.submit_device(info, cx),
                            Err(e) => this.discovery.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        this.discovery.error =
                            Some(non_empty(r.message, "Failed to extract device information").into())
                    }
                    Err(e) => this.discovery.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── DeviceCreationSession ──

    fn submit_device(&mut self, info: DeviceInfo, cx: &mut Context<Self>) {
        self.submitting = true;
        self.error = None;
        // `<DeviceDiscovery>` unmounts while submitting.
        self.discovery = Discovery::default();
        cx.notify();
        let session_id = self.session_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_submit_device_registration(window, State::new(&st), session_id, info).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    // onDeviceSubmitted(): no-op in SessionDetails; the
                    // backend advances the workflow over the websocket.
                    Ok(r) if r.success => this.submitted = true,
                    Ok(r) => {
                        this.error =
                            Some(non_empty(r.message, "Failed to submit device registration").into())
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.submitting = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

}

impl DeviceListHost for DeviceCreationSession {
    fn select_device(&mut self, device_id: &str, cx: &mut Context<Self>) {
        if let Some(dev) = self.discovery.devices.iter().find(|d| d.id == device_id) {
            if is_supported(dev) {
                self.discovery.selected = Some(dev.clone());
                cx.notify();
            }
        }
    }

    fn unlock_device(&mut self, device_id: &str, cx: &mut Context<Self>) {
        let device_id = device_id.to_string();
        self.discovery.unlocking_id = Some(device_id.clone());
        self.discovery.error = None;
        cx.notify();
        let cfg = self.wallet_config.clone();
        let id = device_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = main_window(&app)?;
            cmd_unlock_device(app, window, State::new(&st), id, cfg).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                let d = &mut this.discovery;
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<DiscoveredDevice>(r.data.unwrap_or_default()) {
                            Ok(dev) => {
                                for x in d.devices.iter_mut() {
                                    if x.id == device_id {
                                        *x = dev.clone();
                                    }
                                }
                                if is_supported(&dev) {
                                    d.selected = Some(dev);
                                }
                            }
                            Err(e) => d.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => d.error = Some(non_empty(r.message, "Failed to unlock device").into()),
                    Err(e) => d.error = Some(e.into()),
                }
                d.unlocking_id = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}


impl DeviceCreationSession {
    /// Debug screenshot hook (`SIGVAULT_DEV_CLOUD_STEP=devices|select|done`).
    pub fn apply_dev_step(&mut self, cx: &mut Context<Self>) {
        match dev_step().as_deref() {
            Some("devices" | "select") => self.discover(cx),
            Some("done") => self.submitted = true,
            _ => {}
        }
    }

    fn selected_ready(&self) -> Option<&DiscoveredDevice> {
        self.discovery.selected.as_ref().filter(|d| is_supported(d))
    }

    /// Current step for the page's stepper (see [`STEPS`]).
    pub fn step_index(&self) -> usize {
        if self.submitted {
            2
        } else if self.submitting || self.discovery.extracting || self.selected_ready().is_some() {
            1
        } else {
            0
        }
    }

    pub fn subtitle(&self) -> &'static str {
        match self.step_index() {
            0 => "Connect the hardware wallet you want to add, unlock it and scan.",
            1 => "Check the device and key, then share the public key with the session.",
            _ => "Your device's public key was shared. The session continues once everyone has joined.",
        }
    }

    /// Footer buttons for the page frame: (left, right).
    pub fn footer(&self, cx: &mut Context<Self>) -> (Vec<AnyElement>, Vec<AnyElement>) {
        let t = theme();
        let d = &self.discovery;
        if self.submitted {
            return (vec![], vec![]);
        }
        let busy = self.submitting || d.extracting;
        let hint = match self.selected_ready() {
            Some(dev) => format!("{} · {}", dev.model, fingerprint(dev).unwrap_or_default()),
            None if d.devices.is_empty() => "Scan for devices to continue.".into(),
            None => "Select a ready device to continue.".into(),
        };
        let label = if self.submitting {
            "Submitting…"
        } else if d.extracting {
            "Reading device…"
        } else {
            "Share public key"
        };
        (
            vec![div().text_size(px(12.)).text_color(t.muted_foreground).child(hint).into_any_element()],
            vec![button("extract", label)
                .loading(busy)
                .when(!busy, |b| b.icon_right("arrow-right"))
                .disabled(self.selected_ready().is_none() && !busy)
                .on_click(cx.listener(|this, _, _, cx| this.extract_info(cx)))
                .into_any_element()],
        )
    }

    fn render_devices(&self, cx: &mut Context<Self>) -> AnyElement {
        let d = &self.discovery;
        let scan = button(
            "discover",
            if d.discovering {
                "Scanning…"
            } else if d.devices.is_empty() {
                "Scan for devices"
            } else {
                "Scan again"
            },
        )
        .outline()
        .small()
        .icon("usb")
        .loading(d.discovering)
        .disabled(self.submitting || d.extracting)
        .on_click(cx.listener(|this, _, _, cx| this.discover(cx)));

        let body: AnyElement = if d.devices.is_empty() {
            let t = theme();
            card()
                .flex_row()
                .items_center()
                .gap_4()
                .px_4()
                .py_4()
                .child(super::common::icon_tile("usb", 32., Some(t.muted_foreground)))
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
                                .child(if d.discovering { "Looking for devices…" } else { "No devices scanned yet" }),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .text_color(t.muted_foreground)
                                .child("Plug in your hardware wallet, unlock it and open the Bitcoin app."),
                        ),
                )
                .when(d.discovering, |c| c.child(spinner(px(16.))))
                .into_any_element()
        } else {
            let selected_id = d.selected.as_ref().map(|s| s.id.clone());
            let empty = Default::default();
            device_list(
                DeviceListProps {
                    devices: &d.devices.clone(),
                    selected_id: selected_id.as_deref(),
                    can_unlock: true,
                    highlighted_fingerprints: None,
                    signer_fingerprints: None,
                    unlocking_id: d.unlocking_id.clone().as_deref(),
                    unlock_status: Some(&empty),
                },
                cx,
            )
        };

        section("Hardware wallet")
            .description("Connect and unlock your device, then scan.")
            .action(scan)
            .children(d.error.clone().map(|e| alert(Tone::Destructive, e)))
            .child(body)
            .into_any_element()
    }

    fn render_key_review(&self, device: &DiscoveredDevice) -> AnyElement {
        let t = theme();
        let mono = |s: String| div().font_family(FONT_MONO).text_size(px(12.)).child(s);
        section("Public key to share")
            .description("SigVault reads the extended public key at this path. Private keys never leave the device.")
            .child(
                card()
                    .child(review_row("Device", div().child(format!("{} · {}", device.model, device.device_type))))
                    .child(divider())
                    .child(review_row("Fingerprint", mono(fingerprint(device).unwrap_or_default())))
                    .child(divider())
                    .child(review_row("Derivation path", mono(self.derivation_path.clone())))
                    .child(divider())
                    .child(review_row("Network", div().flex().child(network_badge(&self.network))))
                    .child(divider())
                    .child(review_row(
                        "Session",
                        div().text_color(t.muted_foreground).child(mono(self.session_id.clone())),
                    )),
            )
            .into_any_element()
    }
}

impl Render for DeviceCreationSession {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let body: Vec<AnyElement> = if self.submitted {
            vec![card_message(
                "circle-check",
                Some(t.success),
                "Public key shared",
                "Your device is registered for this session. The coordinator will continue once every co-signer has added theirs — you can leave this page.",
            )
            .into_any_element()]
        } else if self.submitting {
            vec![card()
                .flex_row()
                .items_center()
                .gap_3()
                .h(px(56.))
                .px_4()
                .text_size(px(13.))
                .text_color(t.muted_foreground)
                .child(spinner(px(14.)))
                .child("Submitting device registration…")
                .into_any_element()]
        } else {
            let mut v = vec![self.render_devices(cx)];
            if let Some(dev) = self.selected_ready().cloned() {
                v.push(self.render_key_review(&dev));
            }
            v
        };
        div()
            .flex()
            .flex_col()
            .gap_6()
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .children(body)
    }
}
