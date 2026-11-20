//! Shared hardware-device UI for cloud sessions: `components/DeviceCard.tsx`,
//! `components/DeviceList.tsx` (rendered as one list card with a row per
//! device) and the helpers from `types/hardware.ts`.
//! The stateful components live in `device_creation.rs` (DeviceDiscovery +
//! DeviceCreationSession) and `signing.rs` (TransactionSigning).

use std::collections::{HashMap, HashSet};

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, FontWeight};
use sigvault_core::hwi::{DeviceState, DiscoveredDevice, UnsupportedReason};

use super::common::{icon_tile, sep_dot};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, card, divider, dot, spinner, Tone};

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
        DeviceState::Locked {
            pairing_code: Some(c),
        } if !c.is_empty() => Some(c.clone()),
        _ => None,
    }
}

fn unsupported_reason_text(reason: &UnsupportedReason) -> String {
    match reason {
        UnsupportedReason::Version {
            minimal_supported_version,
        } => format!("Firmware version too old. Minimum required: {minimal_supported_version}"),
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

/// `DeviceList.tsx`: one bordered card, a row per discovered device.
pub fn device_list<V: DeviceListHost>(
    props: DeviceListProps<'_>,
    cx: &mut Context<V>,
) -> AnyElement {
    let t = theme();
    if props.devices.is_empty() {
        return card()
            .items_center()
            .gap_1()
            .py_10()
            .child(icon_tile("usb", 36., Some(t.muted_foreground)))
            .child(
                div()
                    .mt_2()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child("No hardware wallets detected"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .child(
                        "Connect and unlock your device, open the Bitcoin app, then scan again.",
                    ),
            )
            .into_any_element();
    }

    let mut list = card().overflow_hidden();
    for (ix, device) in props.devices.iter().enumerate() {
        if ix > 0 {
            list = list.child(divider());
        }
        let fp = fingerprint(device);
        let is_highlighted = match (&fp, props.highlighted_fingerprints) {
            (Some(fp), Some(set)) => set.contains(fp),
            _ => false,
        };
        let is_non_signer = match (props.signer_fingerprints, &fp) {
            (Some(set), Some(fp)) if is_supported(device) => !set.contains(fp),
            _ => false,
        };
        let row = DeviceRowProps {
            device,
            is_selected: props.selected_id == Some(device.id.as_str()),
            is_highlighted,
            is_unlocking: props.unlocking_id == Some(device.id.as_str()),
            is_non_signer,
            can_unlock: props.can_unlock,
            unlock_status_message: props.unlock_status.and_then(|m| m.get(&device.id)).cloned(),
        };
        list = list.child(device_row(ix, row, cx));
    }
    list.into_any_element()
}

struct DeviceRowProps<'a> {
    device: &'a DiscoveredDevice,
    is_selected: bool,
    is_highlighted: bool,
    is_unlocking: bool,
    is_non_signer: bool,
    can_unlock: bool,
    unlock_status_message: Option<String>,
}

/// `DeviceCard.tsx` as a list row: radio, device, fingerprint, status,
/// action; pairing code / unlock progress / unsupported reason expand
/// below the row.
fn device_row<V: DeviceListHost>(
    ix: usize,
    p: DeviceRowProps<'_>,
    cx: &mut Context<V>,
) -> AnyElement {
    let t = theme();
    let device = p.device;
    let locked = is_locked(device);
    let supported = is_supported(device);
    let unsupported = is_unsupported(device);
    let fp = fingerprint(device);
    let pairing = pairing_code(device);
    let selectable = supported && !p.is_non_signer;

    let (status_label, status_tone) = if supported && p.is_highlighted {
        ("Signer", Tone::Success)
    } else if supported {
        ("Ready", Tone::Success)
    } else if locked {
        ("Locked", Tone::Warning)
    } else {
        ("Unsupported", Tone::Secondary)
    };

    let device_id = device.id.clone();
    let select = {
        let device_id = device_id.clone();
        cx.listener(move |this: &mut V, _: &ClickEvent, _, cx| this.select_device(&device_id, cx))
    };
    let unlock = {
        let device_id = device_id.clone();
        cx.listener(move |this: &mut V, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.unlock_device(&device_id, cx)
        })
    };

    let (version, registered) = match &device.state {
        DeviceState::Supported {
            version,
            registered,
            ..
        } => (version.clone(), *registered),
        _ => (None, None),
    };

    // Second line: type · firmware · policy registration.
    let mut meta = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .text_size(px(12.))
        .text_color(t.muted_foreground)
        .child(device.device_type.clone());
    if let Some(v) = version.filter(|v| !v.is_empty()) {
        meta = meta.child(sep_dot()).child(format!("Firmware {v}"));
    }
    if let Some(reg) = registered {
        meta = meta.child(sep_dot()).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(5.))
                .child(dot(if reg { t.success } else { t.warning }))
                .child(if reg {
                    "Wallet registered"
                } else {
                    "Wallet not registered"
                }),
        );
    }

    let radio = div()
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(if p.is_selected { t.primary } else { t.border })
        .when(!selectable, |d| d.opacity(0.4))
        .when(p.is_selected, |d| {
            d.child(div().size(px(8.)).rounded_full().bg(t.primary))
        });

    let action: Option<AnyElement> = if locked && p.can_unlock {
        Some(
            button(
                ("unlock", ix),
                if p.is_unlocking {
                    "Unlocking…"
                } else {
                    "Unlock"
                },
            )
            .small()
            .outline()
            .icon("unlock")
            .loading(p.is_unlocking)
            .on_click(unlock)
            .into_any_element(),
        )
    } else {
        None
    };

    let header = div()
        .id(("device-row", ix))
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .h(px(60.))
        .px_4()
        .when(selectable, |d| {
            d.cursor_pointer()
                .hover(move |s| s.bg(alpha(t.muted, 0.5)))
                .on_click(select)
        })
        .child(radio)
        .child(icon_tile(
            "hw-device",
            32.,
            Some(if p.is_selected {
                t.primary
            } else {
                t.muted_foreground
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
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.foreground)
                        .child(device.model.clone()),
                )
                .child(meta),
        )
        .child(
            div()
                .w(px(80.))
                .flex_none()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .text_color(if fp.is_some() {
                    t.foreground
                } else {
                    t.muted_foreground
                })
                .child(fp.unwrap_or_else(|| "—".into())),
        )
        .child(
            div()
                .w(px(170.))
                .flex_none()
                .flex()
                .flex_row()
                .justify_end()
                .gap_2()
                .when(p.is_non_signer, |d| {
                    d.child(badge("Not a signer", Tone::Destructive))
                })
                .child(badge(status_label, status_tone))
                .children(action),
        );

    // Pairing code split over two lines (first two words / the rest).
    let pairing_el = pairing.filter(|_| p.is_unlocking).map(|code| {
        let parts: Vec<&str> = code.split_whitespace().collect();
        let top = parts.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
        let bottom = parts.iter().skip(2).cloned().collect::<Vec<_>>().join(" ");
        let code_line = |s: String| {
            div()
                .font_family(FONT_MONO)
                .text_size(px(20.))
                .line_height(px(28.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.primary)
                .child(s)
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .rounded_md()
            .border_1()
            .border_color(alpha(t.primary, 0.3))
            .bg(alpha(t.primary, 0.05))
            .px_4()
            .py_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Pairing code"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child("Confirm the same code on the device display."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .child(code_line(top))
                    .when(!bottom.is_empty(), |d| d.child(code_line(bottom))),
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
                .text_size(px(12.))
                .text_color(t.muted_foreground)
                .child(spinner(px(12.)))
                .child(msg)
        });

    let unsupported_el = match &device.state {
        DeviceState::Unsupported { reason, .. } if unsupported => {
            Some(alert(Tone::Destructive, unsupported_reason_text(reason)))
        }
        _ => None,
    };

    let has_extra = pairing_el.is_some() || status_msg_el.is_some() || unsupported_el.is_some();
    div()
        .flex()
        .flex_col()
        .when(p.is_selected, |d| d.bg(alpha(t.primary, 0.06)))
        .child(header)
        .when(has_extra, |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .pl(px(80.))
                    .pr_4()
                    .pb_3()
                    .children(status_msg_el)
                    .children(pairing_el)
                    .children(unsupported_el),
            )
        })
        .into_any_element()
}
