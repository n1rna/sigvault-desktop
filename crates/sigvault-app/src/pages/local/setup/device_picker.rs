//! Hardware-wallet picker for the "Hardware wallet" slot source — a port
//! of `components/DeviceDiscovery.tsx` + `DeviceList` + `DeviceCard`
//! scoped to what the slot editor needs: discover, unlock, select, then
//! read the xpub at the slot's derivation path.

use gpui::{div, prelude::*, px, Context, EventEmitter, FontWeight, SharedString, Window};

use super::api::{self, DeviceInfo, DeviceState, DiscoveredDevice};
use crate::state::store;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{alert, badge, button, dot, icon, Tone};

pub enum DevicePickerEvent {
    Picked(DeviceInfo),
}

pub struct DevicePicker {
    derivation_path: String,
    discovering: bool,
    devices: Vec<DiscoveredDevice>,
    selected: Option<String>,
    extracting: bool,
    unlocking: Option<String>,
    error: Option<SharedString>,
}

impl EventEmitter<DevicePickerEvent> for DevicePicker {}

impl DevicePicker {
    pub fn new(derivation_path: String) -> Self {
        Self {
            derivation_path,
            discovering: false,
            devices: vec![],
            selected: None,
            extracting: false,
            unlocking: None,
            error: None,
        }
    }

    pub fn set_path(&mut self, path: String, cx: &mut Context<Self>) {
        if self.derivation_path != path {
            self.derivation_path = path;
            cx.notify();
        }
    }

    fn selected_device(&self) -> Option<&DiscoveredDevice> {
        let id = self.selected.as_ref()?;
        self.devices.iter().find(|d| &d.id == id)
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        self.discovering = true;
        self.error = None;
        store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
        cx.notify();
        let task = api::discover_devices(cx);
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(devices) => {
                        if let Some(sel) = &this.selected {
                            if !devices.iter().any(|d| &d.id == sel) {
                                this.selected = None;
                            }
                        }
                        this.devices = devices;
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.discovering = false;
                store(cx).update(cx, |s, cx| s.clear_activity_log(cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn unlock(&mut self, device_id: String, cx: &mut Context<Self>) {
        self.unlocking = Some(device_id.clone());
        self.error = None;
        cx.notify();
        let task = api::unlock_device(cx, device_id.clone());
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(dev) => {
                        let supported = dev.is_supported();
                        for d in this.devices.iter_mut() {
                            if d.id == device_id {
                                *d = dev.clone();
                            }
                        }
                        if supported {
                            this.selected = Some(dev.id.clone());
                        }
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                this.unlocking = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn extract(&mut self, cx: &mut Context<Self>) {
        let Some(fp) = self
            .selected_device()
            .and_then(|d| d.fingerprint())
            .map(str::to_string)
        else {
            return;
        };
        self.extracting = true;
        self.error = None;
        cx.notify();
        let task = api::get_device_xpub(cx, fp, self.derivation_path.clone());
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.extracting = false;
                match res {
                    Ok(info) => cx.emit(DevicePickerEvent::Picked(info)),
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_card(
        &self,
        ix: usize,
        d: &DiscoveredDevice,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme();
        let selected = self.selected.as_deref() == Some(d.id.as_str());
        let unlocking = self.unlocking.as_deref() == Some(d.id.as_str());
        let (supported, locked) = (d.is_supported(), d.is_locked());
        let rail = if selected {
            t.primary
        } else if locked {
            alpha(t.primary, 0.6)
        } else if !supported {
            alpha(t.destructive, 0.4)
        } else {
            t.border
        };
        let (status, tone, dot_color) = if supported {
            ("Ready", Tone::Success, t.success)
        } else if locked {
            ("Locked", Tone::Default, t.primary)
        } else {
            (
                "Unsupported",
                Tone::Secondary,
                alpha(t.muted_foreground, 0.4),
            )
        };
        let id = d.id.clone();
        let id2 = d.id.clone();

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
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(k),
                )
                .child(v)
        };
        let mut meta = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(alpha(t.border, 0.7))
            .bg(t.border);
        let mut has_meta = false;
        if let Some(fp) = d.fingerprint() {
            has_meta = true;
            meta = meta.child(meta_row(
                "Fingerprint",
                div()
                    .font_family(FONT_MONO)
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.primary)
                    .child(fp.to_string())
                    .into_any_element(),
            ));
        }
        if let DeviceState::Supported {
            version,
            registered,
            ..
        } = &d.state
        {
            if let Some(v) = version {
                has_meta = true;
                meta = meta.child(meta_row(
                    "Firmware",
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(11.))
                        .child(v.clone())
                        .into_any_element(),
                ));
            }
            if let Some(r) = registered {
                has_meta = true;
                meta = meta.child(meta_row(
                    "Policy",
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .child(dot(if *r { t.success } else { t.warning }))
                        .child(if *r { "Registered" } else { "Not registered" })
                        .into_any_element(),
                ));
            }
        }

        let pairing = match &d.state {
            DeviceState::Locked {
                pairing_code: Some(code),
            } if unlocking => Some(code.clone()),
            _ => None,
        };
        let unsupported = match &d.state {
            DeviceState::Unsupported { reason, .. } => Some(reason.text()),
            _ => None,
        };

        div()
            .id(("device", ix))
            .relative()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .map(|c| {
                if selected {
                    c.border_color(alpha(t.primary, 0.6))
                        .bg(alpha(t.primary, 0.06))
                } else if locked {
                    c.border_color(alpha(t.primary, 0.4))
                        .bg(alpha(t.primary, 0.03))
                } else if !supported {
                    c.border_color(t.border)
                        .bg(alpha(t.muted, 0.3))
                        .opacity(0.8)
                } else {
                    c.border_color(t.border).bg(t.card)
                }
            })
            .when(supported, |c| {
                c.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(id.clone());
                        cx.notify();
                    }))
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(3.))
                    .bg(rail),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .py_5()
                    .pl_6()
                    .pr_5()
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
                                            .child(icon("hard-drive")),
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
                                                            .text_size(px(15.))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child(d.model.clone()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .mt(px(2.))
                                                    .text_size(px(12.))
                                                    .text_color(t.muted_foreground)
                                                    .child(d.device_type.clone()),
                                            ),
                                    ),
                            )
                            .child(badge(status, tone)),
                    )
                    .when(has_meta, |c| c.child(meta))
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
                                    .text_center()
                                    .text_size(px(11.))
                                    .text_color(t.muted_foreground)
                                    .child("Confirm on device display"),
                            )
                    }))
                    .children(unsupported.map(|r| alert(Tone::Destructive, r)))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .border_t_1()
                            .border_color(alpha(t.border, 0.6))
                            .pt_3()
                            .when(supported, |c| {
                                c.child(
                                    button(
                                        ("select", ix),
                                        if selected { "Selected" } else { "Select" },
                                    )
                                    .small()
                                    .map(|b| {
                                        if selected {
                                            b.secondary().icon("check")
                                        } else {
                                            b.icon_right("chevron-right")
                                        }
                                    }),
                                )
                            })
                            .when(locked, |c| {
                                c.child(
                                    button(
                                        ("unlock", ix),
                                        if unlocking { "Unlocking" } else { "Unlock" },
                                    )
                                    .small()
                                    .icon("lock")
                                    .loading(unlocking)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.unlock(id2.clone(), cx);
                                        },
                                    )),
                                )
                            })
                            .when(!supported && !locked, |c| {
                                c.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .h(px(32.))
                                        .px_3()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(t.border)
                                        .bg(alpha(t.muted, 0.4))
                                        .text_size(px(12.))
                                        .text_color(t.muted_foreground)
                                        .child("Unavailable"),
                                )
                            }),
                    ),
            )
    }
}

impl Render for DevicePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let can_extract =
            self.selected_device().is_some_and(|d| d.is_supported()) && !self.extracting;
        let cards: Vec<_> = self
            .devices
            .clone()
            .iter()
            .enumerate()
            .map(|(i, d)| self.render_card(i, d, cx).into_any_element())
            .collect();
        let has_devices = !self.devices.is_empty();

        div()
            .flex()
            .flex_col()
            .gap_6()
            .rounded_lg()
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .p_6()
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
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
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
                        button(
                            "discover",
                            if self.discovering {
                                "Scanning…"
                            } else {
                                "Discover devices"
                            },
                        )
                        .loading(self.discovering)
                        .on_click(cx.listener(|this, _, _, cx| this.discover(cx))),
                    ),
            )
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .when(has_devices, |d| {
                d.child(div().flex().flex_col().gap_3().children(cards))
                    .when(can_extract || self.extracting, |d| {
                        d.child(
                            button(
                                "extract",
                                if self.extracting {
                                    "Extracting device info…"
                                } else {
                                    "Continue with selected device"
                                },
                            )
                            .large()
                            .full_width()
                            .loading(self.extracting)
                            .on_click(cx.listener(|this, _, _, cx| this.extract(cx))),
                        )
                    })
            })
    }
}
