//! Shared hardware-device UI for cloud sessions: `components/DeviceCard.tsx`,
//! `components/DeviceList.tsx` and the helpers from `types/hardware.ts`.
//! The stateful components live in `device_creation.rs` (DeviceDiscovery +
//! DeviceCreationSession) and `signing.rs` (TransactionSigning).

use std::collections::{HashMap, HashSet};

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, ElementId, FontWeight};
use sigvault_core::hwi::{DeviceState, DiscoveredDevice, UnsupportedReason};

use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, dot, icon, spinner, Tone};

// ── types/hardware.ts helpers ──

pub fn is_locked(d: &DiscoveredDevice) -> bool {
    matches!(d.state, DeviceState::Locked { .. })
}

pub fn is_supported(d: &DiscoveredDevice) -> bool {
    matches!(d.state, DeviceState::Supported { .. })
}

pub fn is_unsupported(d: &DiscoveredDevice) -> bool {
    matches!(d.state, DeviceState::Unsupported { .. })
}

pub fn fingerprint(d: &DiscoveredDevice) -> Option<String> {
    match &d.state {
        DeviceState::Supported { fingerprint, .. } => Some(fingerprint.clone()),
        _ => None,
    }
}

pub fn pairing_code(d: &DiscoveredDevice) -> Option<String> {
    match &d.state {
        DeviceState::Locked { pairing_code: Some(c) } if !c.is_empty() => Some(c.clone()),
        _ => None,
    }
}

fn unsupported_reason_text(reason: &UnsupportedReason) -> String {
    match reason {
        UnsupportedReason::Version { minimal_supported_version } => format!(
            "Firmware version too old. Minimum required: {minimal_supported_version}"
        ),
        UnsupportedReason::Method(m) => format!("Device doesn't support required feature: {m}"),
        UnsupportedReason::NotPartOfWallet { fingerprint } => {
            format!("Device fingerprint {fingerprint} is not part of this wallet")
        }
        UnsupportedReason::WrongNetwork => "Device is configured for a different network".into(),
        UnsupportedReason::AppNotOpen => "Bitcoin app not open on device".into(),
        UnsupportedReason::InitializationError(e) => format!("Initialization error: {e}"),
    }
}

/// Parent of a [`device_list`]: receives card select / unlock clicks.
pub trait DeviceListHost: 'static + Sized {
    fn select_device(&mut self, device_id: &str, cx: &mut Context<Self>);
    fn unlock_device(&mut self, device_id: &str, cx: &mut Context<Self>);
}

/// Props of `DeviceList.tsx`.
pub struct DeviceListProps<'a> {
    pub devices: &'a [DiscoveredDevice],
    pub selected_id: Option<&'a str>,
    /// Whether an `onUnlockDevice` handler is wired (shows Unlock buttons).
    pub can_unlock: bool,
    pub highlighted_fingerprints: Option<&'a HashSet<String>>,
    pub signer_fingerprints: Option<&'a HashSet<String>>,
    pub unlocking_id: Option<&'a str>,
    pub unlock_status: Option<&'a HashMap<String, String>>,
}

/// `DeviceList.tsx`: responsive grid of [`DeviceCard`]s (auto-fill, min 300px).
pub fn device_list<V: DeviceListHost>(props: DeviceListProps<'_>, cx: &mut Context<V>) -> AnyElement {
    let t = theme();
    if props.devices.is_empty() {
        return div()
            .py_12()
            .flex()
            .flex_col()
            .items_center()
            .text_color(t.muted_foreground)
            .child(div().text_size(px(14.)).child("No hardware wallets detected."))
            .child(
                div()
                    .mt_2()
                    .text_size(px(13.))
                    .child("Please connect your hardware wallet and try again."),
            )
            .into_any_element();
    }

    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_3()
        .children(props.devices.iter().enumerate().map(|(ix, device)| {
            let fp = fingerprint(device);
            let is_highlighted = match (&fp, props.highlighted_fingerprints) {
                (Some(fp), Some(set)) => set.contains(fp),
                _ => false,
            };
            let is_non_signer = match (props.signer_fingerprints, &fp) {
                (Some(set), Some(fp)) if is_supported(device) => !set.contains(fp),
                _ => false,
            };
            let card = DeviceCardProps {
                device,
                is_selected: props.selected_id == Some(device.id.as_str()),
                is_highlighted,
                is_unlocking: props.unlocking_id == Some(device.id.as_str()),
                is_non_signer,
                can_unlock: props.can_unlock,
                unlock_status_message: props.unlock_status.and_then(|m| m.get(&device.id)).cloned(),
            };
            div()
                .flex_grow()
                .flex_basis(px(300.))
                .min_w(px(300.))
                .child(device_card(("device-card", ix), card, cx))
        }))
        .into_any_element()
}

struct DeviceCardProps<'a> {
    device: &'a DiscoveredDevice,
    is_selected: bool,
    is_highlighted: bool,
    is_unlocking: bool,
    is_non_signer: bool,
    can_unlock: bool,
    unlock_status_message: Option<String>,
}

/// `DeviceCard.tsx`.
fn device_card<V: DeviceListHost>(
    id: impl Into<ElementId>,
    p: DeviceCardProps<'_>,
    cx: &mut Context<V>,
) -> AnyElement {
    let t = theme();
    let device = p.device;
    let locked = is_locked(device);
    let supported = is_supported(device);
    let unsupported = is_unsupported(device);
    let fp = fingerprint(device);
    let pairing = pairing_code(device);

    let rail = if p.is_selected {
        t.primary
    } else if p.is_highlighted {
        t.success
    } else if locked {
        alpha(t.primary, 0.6)
    } else if unsupported {
        alpha(t.destructive, 0.4)
    } else {
        t.border
    };
    let (border, bg, hover_border) = if p.is_selected {
        (alpha(t.primary, 0.6), alpha(t.primary, 0.06), None)
    } else if p.is_highlighted {
        (alpha(t.success, 0.5), alpha(t.success, 0.05), None)
    } else if locked {
        (alpha(t.primary, 0.4), alpha(t.primary, 0.03), Some(alpha(t.primary, 0.6)))
    } else if unsupported {
        (t.border, alpha(t.muted, 0.3), None)
    } else {
        (t.border, t.card, Some(alpha(t.primary, 0.4)))
    };

    let (status_label, status_tone, dot_color) = if supported {
        ("Ready", Tone::Success, t.success)
    } else if locked {
        ("Locked", Tone::Default, t.primary)
    } else {
        ("Unsupported", Tone::Secondary, alpha(t.muted_foreground, 0.4))
    };

    let device_id = device.id.clone();
    let select = {
        let device_id = device_id.clone();
        cx.listener(move |this: &mut V, _: &ClickEvent, _, cx| this.select_device(&device_id, cx))
    };
    let select_btn = {
        let device_id = device_id.clone();
        cx.listener(move |this: &mut V, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.select_device(&device_id, cx)
        })
    };
    let unlock = {
        let device_id = device_id.clone();
        cx.listener(move |this: &mut V, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.unlock_device(&device_id, cx)
        })
    };

    let meta_row = |key: &'static str, value: AnyElement| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .bg(t.card)
            .px(px(14.))
            .py_2()
            .child(div().text_size(px(12.)).text_color(t.muted_foreground).child(key))
            .child(value)
    };

    let (version, registered) = match &device.state {
        DeviceState::Supported { version, registered, .. } => (version.clone(), *registered),
        _ => (None, None),
    };

    let mut meta = div()
        .flex()
        .flex_col()
        .gap(px(1.))
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(alpha(t.border, 0.7))
        .bg(t.border);
    let mut has_meta = false;
    if let Some(fp) = fp.clone() {
        has_meta = true;
        meta = meta.child(meta_row(
            "Fingerprint",
            div()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.primary)
                .child(fp)
                .into_any_element(),
        ));
    }
    if let Some(v) = version.filter(|v| !v.is_empty()) {
        has_meta = true;
        meta = meta.child(meta_row(
            "Firmware",
            div()
                .font_family(FONT_MONO)
                .text_size(px(11.))
                .text_color(t.foreground)
                .child(v)
                .into_any_element(),
        ));
    }
    if let Some(reg) = registered {
        has_meta = true;
        meta = meta.child(meta_row(
            "Policy",
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(t.foreground)
                .child(dot(if reg { t.success } else { t.warning }))
                .child(if reg { "Registered" } else { "Not registered" })
                .into_any_element(),
        ));
    }

    // Pairing code split over two lines (first two words / the rest).
    let pairing_el = pairing.filter(|_| p.is_unlocking).map(|code| {
        let parts: Vec<&str> = code.split_whitespace().collect();
        let top = parts.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
        let bottom = parts.iter().skip(2).cloned().collect::<Vec<_>>().join(" ");
        let code_line = |s: String| {
            div()
                .font_family(FONT_MONO)
                .text_size(px(22.))
                .line_height(px(30.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.primary)
                .child(s)
        };
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
                    .child(code_line(top))
                    .when(!bottom.is_empty(), |d| d.child(code_line(bottom))),
            )
            .child(
                div()
                    .mt_2()
                    .text_center()
                    .text_size(px(11.))
                    .text_color(t.muted_foreground)
                    .child("Confirm on device display"),
            )
    });

    let status_msg_el = p
        .unlock_status_message
        .clone()
        .filter(|_| p.is_unlocking)
        .map(|msg| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .rounded_md()
                .border_1()
                .border_color(alpha(t.primary, 0.2))
                .bg(alpha(t.primary, 0.04))
                .px_3()
                .py_2()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(spinner(px(12.)))
                .child(msg)
        });

    let unsupported_el = match &device.state {
        DeviceState::Unsupported { reason, .. } => {
            Some(alert(Tone::Destructive, unsupported_reason_text(reason)).px_3().py(px(10.)))
        }
        _ => None,
    };

    let mut actions = div()
        .flex()
        .flex_row()
        .justify_end()
        .border_t_1()
        .border_color(alpha(t.border, 0.6))
        .pt_3();
    if supported {
        let b = if p.is_selected {
            button("select", "Selected").secondary().icon("check")
        } else {
            button("select", "Select").icon_right("arrow-right")
        };
        actions = actions.child(b.small().on_click(select_btn));
    }
    if locked && p.can_unlock {
        actions = actions.child(
            button("unlock", if p.is_unlocking { "Unlocking" } else { "Unlock" })
                .small()
                .icon("unlock")
                .loading(p.is_unlocking)
                .on_click(unlock),
        );
    }
    if unsupported {
        actions = actions.child(
            div()
                .flex()
                .items_center()
                .h(px(32.))
                .rounded_md()
                .border_1()
                .border_color(t.border)
                .bg(alpha(t.muted, 0.4))
                .px_3()
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child("Unavailable"),
        );
    }

    let mut card = div()
        .id(id)
        .relative()
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(border)
        .bg(bg);
    if unsupported {
        card = card.opacity(0.8).cursor_not_allowed();
    } else if supported {
        card = card.cursor_pointer().on_click(select);
    }
    if let Some(hb) = hover_border {
        card = card.hover(move |s| s.border_color(hb));
    }

    card.child(div().absolute().top_0().bottom_0().left_0().w(px(3.)).bg(rail))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .py_5()
                .pl_6()
                .pr_5()
                // Header: model + status
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
                                .min_w_0()
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
                                        .child(icon("cloud-device").size(px(16.))),
                                )
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
                                                .child(dot(dot_color))
                                                .child(
                                                    div()
                                                        .text_size(px(15.))
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
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .when(p.is_non_signer, |d| d.child(badge("Not a signer", Tone::Destructive)))
                                .child(badge(status_label, status_tone)),
                        ),
                )
                .when(has_meta, |d| d.child(meta))
                .children(status_msg_el)
                .children(pairing_el)
                .children(unsupported_el)
                .child(actions),
        )
        .into_any_element()
}
