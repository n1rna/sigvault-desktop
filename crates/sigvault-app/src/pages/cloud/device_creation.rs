//! `components/DeviceCreationSession.tsx` + `components/DeviceDiscovery.tsx`:
//! the DEVICE_REGISTRATION session workflow — discover / unlock a hardware
//! wallet, extract its xpub and submit it to the session websocket.

use gpui::{div, prelude::*, px, Context, SharedString, Window};
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_get_device_xpub, cmd_submit_device_registration,
    cmd_unlock_device,
};
use sigvault_core::hwi::{DeviceInfo, DiscoveredDevice};
use sigvault_core::State;

use super::common::{main_window, plain_error};
use super::devices::{device_list, fingerprint, is_supported, DeviceListHost, DeviceListProps};
use crate::runtime::backend;
use crate::state::store;
use crate::theme::theme;
use crate::ui::{button, card, heading, spinner};

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
    #[allow(dead_code)]
    network: String,
    derivation_path: String,
    session_id: String,
    wallet_config: Option<WalletConfigInput>,
    submitting: bool,
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
                    Ok(r) if r.success => {}
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

    fn render_discovery(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let d = &self.discovery;
        let can_extract = d.selected.as_ref().is_some_and(is_supported) && !d.extracting;

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(heading("Hardware wallet"))
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("Connect and unlock your device, then discover it."),
                    ),
            )
            .child(
                button("discover", if d.discovering { "Scanning…" } else { "Discover devices" })
                    .loading(d.discovering)
                    .on_click(cx.listener(|this, _, _, cx| this.discover(cx))),
            );

        let list = (!d.devices.is_empty()).then(|| {
            let selected_id = d.selected.as_ref().map(|s| s.id.clone());
            let unlocking_id = d.unlocking_id.clone();
            let devices = d.devices.clone();
            let extracting = d.extracting;
            let empty = Default::default();
            div()
                .flex()
                .flex_col()
                .gap_6()
                .child(device_list(
                    DeviceListProps {
                        devices: &devices,
                        selected_id: selected_id.as_deref(),
                        can_unlock: true,
                        highlighted_fingerprints: None,
                        signer_fingerprints: None,
                        unlocking_id: unlocking_id.as_deref(),
                        unlock_status: Some(&empty),
                    },
                    cx,
                ))
                .when(can_extract, |el| {
                    el.child(
                        button(
                            "extract",
                            if extracting { "Extracting device info…" } else { "Continue with selected device" },
                        )
                        .large()
                        .full_width()
                        .disabled(extracting)
                        .on_click(cx.listener(|this, _, _, cx| this.extract_info(cx))),
                    )
                })
        });

        card()
            .p_6()
            .gap_6()
            .child(header)
            .children(self.discovery.error.clone().map(plain_error))
            .children(list)
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

impl Render for DeviceCreationSession {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let body = if self.submitting {
            card()
                .items_center()
                .justify_center()
                .p_12()
                .child(div().mb_4().child(spinner(px(48.))))
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(t.muted_foreground)
                        .child("Submitting device registration…"),
                )
                .into_any_element()
        } else {
            self.render_discovery(cx).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .children(self.error.clone().map(plain_error))
            .child(body)
    }
}

/// `result.message || fallback`.
pub(super) fn non_empty(msg: String, fallback: &str) -> String {
    if msg.is_empty() {
        fallback.to_string()
    } else {
        msg
    }
}
