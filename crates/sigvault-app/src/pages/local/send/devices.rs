//! Hardware-device picker for the Send wizard's "Connected device" mode.
//! Port of `components/DeviceDiscovery.tsx` + `DeviceList.tsx` +
//! `DeviceCard.tsx` as used by `SendScreen` (no highlight / non-signer
//! markers, no unlock-status messages — SendScreen passes none).

use gpui::{div, prelude::*, px, Context, EventEmitter, FontWeight, SharedString, Window};
use sigvault_core::commands::hwi::WalletConfigInput;
use sigvault_core::commands::{cmd_discover_hardware_wallets, cmd_get_device_xpub, cmd_unlock_device};
use sigvault_core::hwi::{DeviceInfo, DeviceState, DiscoveredDevice, UnsupportedReason};
use sigvault_core::State;

use crate::runtime::backend;
use crate::state::store;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, dot, icon, Tone};

/// Emitted once a device has been discovered, unlocked and its info
/// extracted (`onDeviceSelected(info, device)`).
pub struct DeviceSelected {
    #[allow(dead_code)] // carried for parity with `onDeviceSelected`; Send only needs the id
    pub info: DeviceInfo,
    pub device: DiscoveredDevice,
}

pub struct DevicePicker {
    wallet_config: Option<WalletConfigInput>,
    derivation_path: String,
    discovering: bool,
    devices: Vec<DiscoveredDevice>,
    selected: Option<DiscoveredDevice>,
    extracting: bool,
    unlocking_id: Option<String>,
    error: Option<SharedString>,
}

impl EventEmitter<DeviceSelected> for DevicePicker {}

fn is_locked(d: &DiscoveredDevice) -> bool {
    matches!(d.state, DeviceState::Locked { .. })
}

fn is_supported(d: &DiscoveredDevice) -> bool {
    matches!(d.state, DeviceState::Supported { .. })
}

fn fingerprint_of(d: &DiscoveredDevice) -> Option<String> {
    match &d.state {
        DeviceState::Supported { fingerprint, .. } => Some(fingerprint.clone()),
        _ => None,
    }
}

fn unsupported_reason_text(reason: &UnsupportedReason) -> String {
    match reason {
        UnsupportedReason::Version { minimal_supported_version } => {
            format!("Firmware version too old. Minimum required: {minimal_supported_version}")
        }
        UnsupportedReason::Method(m) => format!("Device doesn't support required feature: {m}"),
        UnsupportedReason::NotPartOfWallet { fingerprint } => {
            format!("Device fingerprint {fingerprint} is not part of this wallet")
        }
        UnsupportedReason::WrongNetwork => "Device is configured for a different network".into(),
        UnsupportedReason::AppNotOpen => "Bitcoin app not open on device".into(),
        UnsupportedReason::InitializationError(e) => format!("Initialization error: {e}"),
    }
}

impl DevicePicker {
    pub fn new(wallet_config: Option<WalletConfigInput>, derivation_path: String) -> Self {
        Self {
            wallet_config,
            derivation_path,
            discovering: false,
            devices: Vec::new(),
            selected: None,
            extracting: false,
            unlocking_id: None,
            error: None,
        }
    }

    fn clear_activity_log(cx: &mut Context<Self>) {
        store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        self.discovering = true;
        self.error = None;
        Self::clear_activity_log(cx);
        cx.notify();
        let cfg = self.wallet_config.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = app.get_webview_window("main").ok_or("no window")?;
            cmd_discover_hardware_wallets(app, window, State::new(&st), cfg).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<Vec<DiscoveredDevice>>(r.data.unwrap_or_default()) {
                            Ok(devices) => {
                                if let Some(sel) = &this.selected {
                                    if !devices.iter().any(|d| d.id == sel.id) {
                                        this.selected = None;
                                    }
                                }
                                this.devices = devices;
                            }
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        this.error = Some(if r.message.is_empty() {
                            "Failed to discover devices".into()
                        } else {
                            r.message.into()
                        })
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.discovering = false;
                Self::clear_activity_log(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select(&mut self, device: DiscoveredDevice, cx: &mut Context<Self>) {
        if is_supported(&device) {
            self.selected = Some(device);
            cx.notify();
        }
    }

    fn unlock(&mut self, device_id: String, cx: &mut Context<Self>) {
        self.unlocking_id = Some(device_id.clone());
        self.error = None;
        cx.notify();
        let cfg = self.wallet_config.clone();
        let id = device_id.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = app.get_webview_window("main").ok_or("no window")?;
            cmd_unlock_device(app, window, State::new(&st), id, cfg).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<DiscoveredDevice>(r.data.unwrap_or_default()) {
                            Ok(dev) => {
                                for d in this.devices.iter_mut() {
                                    if d.id == device_id {
                                        *d = dev.clone();
                                    }
                                }
                                if is_supported(&dev) {
                                    this.selected = Some(dev);
                                }
                            }
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        this.error = Some(if r.message.is_empty() {
                            "Failed to unlock device".into()
                        } else {
                            r.message.into()
                        })
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

    fn extract_info(&mut self, cx: &mut Context<Self>) {
        let Some(device) = self.selected.clone().filter(is_supported) else {
            return;
        };
        let Some(fingerprint) = fingerprint_of(&device) else {
            return;
        };
        self.extracting = true;
        self.error = None;
        cx.notify();
        let path = self.derivation_path.clone();
        let task = backend(cx).call(move |app, st| async move {
            let window = app.get_webview_window("main").ok_or("no window")?;
            cmd_get_device_xpub(window, State::new(&st), fingerprint, path).await
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.extracting = false;
                match res {
                    Ok(r) if r.success && r.data.is_some() => {
                        match serde_json::from_value::<DeviceInfo>(r.data.unwrap_or_default()) {
                            Ok(info) => cx.emit(DeviceSelected { info, device }),
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Ok(r) => {
                        this.error = Some(if r.message.is_empty() {
                            "Failed to extract device information".into()
                        } else {
                            r.message.into()
                        })
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_card(&self, ix: usize, device: &DiscoveredDevice, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let locked = is_locked(device);
        let supported = is_supported(device);
        let unsupported = !locked && !supported;
        let selected = self.selected.as_ref().is_some_and(|s| s.id == device.id);
        let unlocking = self.unlocking_id.as_deref() == Some(device.id.as_str());

        let rail = if selected {
            t.primary
        } else if locked {
            alpha(t.primary, 0.6)
        } else if unsupported {
            alpha(t.destructive, 0.4)
        } else {
            t.border
        };
        let (border, bg) = if selected {
            (alpha(t.primary, 0.6), alpha(t.primary, 0.06))
        } else if locked {
            (alpha(t.primary, 0.4), alpha(t.primary, 0.03))
        } else if unsupported {
            (t.border, alpha(t.muted, 0.3))
        } else {
            (t.border, t.card)
        };
        let (status_label, status_tone, dot_color) = if supported {
            ("Ready", Tone::Success, t.success)
        } else if locked {
            ("Locked", Tone::Default, t.primary)
        } else {
            ("Unsupported", Tone::Secondary, alpha(t.muted_foreground, 0.4))
        };

        let meta_row = |k: &'static str, v: gpui::AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .bg(t.card)
                .px(px(14.))
                .py_2()
                .child(div().text_size(px(12.)).text_color(t.muted_foreground).child(k))
                .child(v)
        };
        let mut meta: Vec<gpui::AnyElement> = Vec::new();
        if let DeviceState::Supported { fingerprint, version, registered } = &device.state {
            meta.push(
                meta_row(
                    "Fingerprint",
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.primary)
                        .child(fingerprint.clone())
                        .into_any_element(),
                )
                .into_any_element(),
            );
            if let Some(v) = version {
                meta.push(
                    meta_row(
                        "Firmware",
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(t.foreground)
                            .child(v.clone())
                            .into_any_element(),
                    )
                    .into_any_element(),
                );
            }
            if let Some(reg) = registered {
                meta.push(
                    meta_row(
                        "Policy",
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(t.foreground)
                            .child(dot(if *reg { t.success } else { t.warning }))
                            .child(if *reg { "Registered" } else { "Not registered" })
                            .into_any_element(),
                    )
                    .into_any_element(),
                );
            }
        }

        let pairing = match &device.state {
            DeviceState::Locked { pairing_code: Some(c) } if unlocking && !c.is_empty() => Some(c.clone()),
            _ => None,
        };
        let reason = match &device.state {
            DeviceState::Unsupported { reason, .. } => Some(unsupported_reason_text(reason)),
            _ => None,
        };

        let action: gpui::AnyElement = if supported {
            let dev = device.clone();
            let b = button(("dev-select", ix), if selected { "Selected" } else { "Select" }).small();
            let b = if selected { b.secondary().icon("check") } else { b.icon_right("arrow-right") };
            b.on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select(dev.clone(), cx);
            }))
            .into_any_element()
        } else if locked {
            let id = device.id.clone();
            button(("dev-unlock", ix), if unlocking { "Unlocking" } else { "Unlock" })
                .small()
                .icon("lock")
                .loading(unlocking)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.unlocking_id.is_none() {
                        this.unlock(id.clone(), cx);
                    }
                }))
                .into_any_element()
        } else {
            div()
                .h(px(32.))
                .flex()
                .items_center()
                .rounded_md()
                .border_1()
                .border_color(t.border)
                .bg(alpha(t.muted, 0.4))
                .px_3()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child("Unavailable")
                .into_any_element()
        };

        let dev = device.clone();
        div()
            .id(("dev-card", ix))
            .relative()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(bg)
            .when(unsupported, |d| d.opacity(0.8))
            .when(supported, |d| d.cursor_pointer())
            .when(locked && !selected, |d| d.hover(move |s| s.border_color(alpha(t.primary, 0.6))))
            .when(supported && !selected, |d| d.hover(move |s| s.border_color(alpha(t.primary, 0.4))))
            .on_click(cx.listener(move |this, _, _, cx| this.select(dev.clone(), cx)))
            .child(div().absolute().top_0().bottom_0().left_0().w(px(3.)).bg(rail))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .py_4()
                    .pl_5()
                    .pr_4()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .size(px(40.))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(t.border)
                                            .bg(t.background)
                                            .child(icon("hw-device").size(px(16.))),
                                    )
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
                                                    .child(dot(dot_color))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(t.foreground)
                                                            .child(device.model.clone()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .mt(px(2.))
                                                    .text_size(px(12.))
                                                    .text_color(t.muted_foreground)
                                                    .child(device.device_type.clone()),
                                            ),
                                    ),
                            )
                            .child(badge(status_label, status_tone)),
                    )
                    .when(!meta.is_empty(), |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(1.))
                                .overflow_hidden()
                                .rounded_md()
                                .border_1()
                                .border_color(alpha(t.border, 0.7))
                                .bg(t.border)
                                .children(meta),
                        )
                    })
                    .children(pairing.map(|code| {
                        let parts: Vec<&str> = code.split_whitespace().collect();
                        let top = parts.iter().take(2).copied().collect::<Vec<_>>().join(" ");
                        let bottom = parts.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(alpha(t.primary, 0.3))
                            .bg(t.background)
                            .p_4()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.muted_foreground)
                                    .child("Pairing code"),
                            )
                            .child(
                                div()
                                    .mt_2()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .font_family(FONT_MONO)
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.primary)
                                    .child(top)
                                    .when(!bottom.is_empty(), |d| d.child(bottom)),
                            )
                            .child(
                                div()
                                    .mt_2()
                                    .flex()
                                    .justify_center()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child("Confirm on device display"),
                            )
                    }))
                    .children(reason.map(|r| alert(Tone::Destructive, r)))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .border_t_1()
                            .border_color(alpha(t.border, 0.6))
                            .pt_3()
                            .child(action),
                    ),
            )
    }
}

impl Render for DevicePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let can_extract = self.selected.as_ref().is_some_and(is_supported);
        let cards: Vec<_> = self
            .devices
            .iter()
            .enumerate()
            .map(|(i, d)| self.render_card(i, d, cx).into_any_element())
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .p_5()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child("Hardware wallet"),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child("Connect and unlock your device, then discover it."),
                            ),
                    )
                    .child(
                        button("discover", if self.discovering { "Scanning…" } else { "Discover devices" })
                            .outline()
                            .small()
                            .icon("usb")
                            .loading(self.discovering)
                            .on_click(cx.listener(|this, _, _, cx| this.discover(cx))),
                    ),
            )
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .when(!self.devices.is_empty(), |d| {
                d.child(div().flex().flex_col().gap_3().children(cards)).when(can_extract, |d| {
                    d.child(
                        button(
                            "extract",
                            if self.extracting { "Extracting device info…" } else { "Continue with selected device" },
                        )
                        .full_width()
                        .loading(self.extracting)
                        .on_click(cx.listener(|this, _, _, cx| this.extract_info(cx))),
                    )
                })
            })
    }
}
