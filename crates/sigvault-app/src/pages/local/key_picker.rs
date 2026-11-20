//! Key picker: choose a key-library key for one wallet slot.
//!
//! Lists `keychain.keys_for_network(network)`, greys out keys already used
//! in another slot (the core rejects a wallet that names the same key
//! twice), filters by name / fingerprint / source, and can add a new key
//! in place through [`AddKeyFlow`]. Renders only its body; the host wraps
//! it in `crate::ui::modal` (see [`KeyPicker::modal_width`]).

use std::collections::HashMap;

use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FontWeight, SharedString,
    Subscription, Task, Window,
};

use super::keys::add_flow::{AddKeyEvent, AddKeyFlow};
use super::setup::api::HotSecret;
use super::setup::finish::network_label;
use super::setup::slot_board::{custody_badge, meta_line, source_icon};
use crate::keychain::{keychain, source_label, KeyRecord, KeychainEvent};
use crate::theme::{alpha, theme};
use crate::ui::{button, card, divider, icon, InputEvent, TextInput};

#[derive(Debug, Clone)]
pub enum KeyPickerEvent {
    Picked(String),
    Cancelled,
}

/// Identity of a key as the core sees it.
pub fn key_identity(fingerprint: &str, xpub: &str) -> (String, String) {
    (fingerprint.to_lowercase(), xpub.to_string())
}

/// Decrypt the private keys of library keys, keyed by the caller's slot id.
/// The keychain must be unlocked (check `keychain(cx).read(cx).is_unlocked()`
/// first and show `KeychainUnlock` when it isn't).
pub fn fetch_secrets(
    cx: &mut App,
    wanted: Vec<(String, String)>,
) -> Task<Result<HashMap<String, HotSecret>, String>> {
    let kc = keychain(cx);
    let tasks: Vec<_> = wanted
        .into_iter()
        .map(|(slot, key)| (slot, kc.update(cx, |k, cx| k.secret(&key, cx))))
        .collect();
    cx.background_spawn(async move {
        let mut out = HashMap::new();
        for (slot, task) in tasks {
            let secret = task.await?;
            out.insert(slot, HotSecret::from(&secret));
        }
        Ok(out)
    })
}

pub struct KeyPicker {
    network: String,
    slot_label: String,
    /// Keys used by *other* slots → that slot's label.
    used: HashMap<(String, String), String>,
    current: Option<String>,
    search: Entity<TextInput>,
    adding: Option<Entity<AddKeyFlow>>,
    _subs: Vec<Subscription>,
    _add_sub: Option<Subscription>,
}

impl EventEmitter<KeyPickerEvent> for KeyPicker {}

impl KeyPicker {
    pub fn new(
        network: String,
        slot_label: String,
        used: HashMap<(String, String), String>,
        current: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            TextInput::new(window, cx).placeholder("Search by name, fingerprint or type")
        });
        let subs = vec![
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()),
        ];
        search.read(cx).focus(window);
        let mut this = Self {
            network,
            slot_label,
            used,
            current,
            search,
            adding: None,
            _subs: subs,
            _add_sub: None,
        };
        // Nothing to pick from yet: go straight to adding one.
        if keychain(cx)
            .read(cx)
            .keys_for_network(&this.network)
            .next()
            .is_none()
        {
            this.start_adding(window, cx);
        }
        this
    }

    /// Width the host modal should use.
    pub fn modal_width(&self) -> f32 {
        if self.adding.is_some() {
            640.
        } else {
            560.
        }
    }

    fn start_adding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let network = self.network.clone();
        let flow = cx.new(|cx| AddKeyFlow::new(Some(network), window, cx));
        self._add_sub = Some(cx.subscribe_in(
            &flow,
            window,
            |this, _, ev: &AddKeyEvent, window, cx| match ev {
                AddKeyEvent::Added(id) => cx.emit(KeyPickerEvent::Picked(id.clone())),
                AddKeyEvent::Cancelled => {
                    this.adding = None;
                    this._add_sub = None;
                    if keychain(cx)
                        .read(cx)
                        .keys_for_network(&this.network)
                        .next()
                        .is_none()
                    {
                        cx.emit(KeyPickerEvent::Cancelled);
                    } else {
                        this.search.read(cx).focus(window);
                    }
                    cx.notify();
                }
            },
        ));
        self.adding = Some(flow);
        cx.notify();
    }

    fn matches(k: &KeyRecord, q: &str) -> bool {
        if q.is_empty() {
            return true;
        }
        let q = q.to_lowercase();
        k.label.to_lowercase().contains(&q)
            || k.fingerprint.contains(&q)
            || source_label(&k.source).to_lowercase().contains(&q)
            || k.device
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(&q))
    }

    fn key_row(&self, k: &KeyRecord, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let used_by = self
            .used
            .get(&key_identity(&k.fingerprint, &k.xpub))
            .cloned();
        let selected = self.current.as_deref() == Some(k.id.as_str());
        let disabled = used_by.is_some();
        let id = k.id.clone();
        div()
            .id(SharedString::from(format!("pick-{}", k.id)))
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .min_h(px(56.))
            .px_4()
            .py_2()
            .when(selected, |d| d.bg(alpha(t.primary, 0.08)))
            .when(disabled, |d| d.opacity(0.5))
            .when(!disabled, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(alpha(t.muted, 0.5)))
                    .on_click(
                        cx.listener(move |_, _, _, cx| cx.emit(KeyPickerEvent::Picked(id.clone()))),
                    )
            })
            .child(
                div()
                    .size(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(t.muted)
                    .child(
                        icon(source_icon(&k.source))
                            .size(px(15.))
                            .text_color(t.foreground),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child(k.label.clone()),
                            )
                            .child(custody_badge(k.is_hot())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(match &k.device {
                                Some(d) => format!("{} ({d})", source_label(&k.source)),
                                None => source_label(&k.source).to_string(),
                            })
                            .child("·")
                            .child(meta_line(&k.fingerprint, &k.derivation_path)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .map(|d| match (&used_by, selected) {
                        (Some(slot), _) => d.child(format!("Used for {slot}")),
                        (None, true) => {
                            d.child(icon("check").size(px(16.)).text_color(t.brand_ink))
                        }
                        (None, false) => d.child(icon("chevron-right").size(px(14.))),
                    }),
            )
    }
}

impl Render for KeyPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(flow) = &self.adding {
            return div()
                .flex()
                .flex_col()
                .gap_4()
                .child(flow.clone())
                .into_any_element();
        }
        let t = theme();
        let q = self.search.read(cx).text().trim().to_string();
        let kc = keychain(cx).read(cx);
        let all: Vec<KeyRecord> = kc.keys_for_network(&self.network).cloned().collect();
        let mut shown: Vec<KeyRecord> = all
            .iter()
            .filter(|k| Self::matches(k, &q))
            .cloned()
            .collect();
        // Available keys first, then ones used elsewhere; newest first.
        shown.sort_by_key(|k| {
            (
                self.used
                    .contains_key(&key_identity(&k.fingerprint, &k.xpub)),
                -k.created_at,
            )
        });
        let net = network_label(&self.network);

        let list = if shown.is_empty() {
            div()
                .py_8()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t.foreground)
                        .child(format!("No keys match “{q}”")),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Try another name or fingerprint."),
                )
                .into_any_element()
        } else {
            let n = shown.len();
            let mut rows = Vec::new();
            for (i, k) in shown.iter().enumerate() {
                rows.push(self.key_row(k, cx).into_any_element());
                if i + 1 < n {
                    rows.push(divider().into_any_element());
                }
            }
            card()
                .id("key-picker-list")
                .max_h(px(340.))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.foreground)
                            .child(format!("Choose a key for {}", self.slot_label)),
                    )
                    .child(div().text_size(px(13.)).text_color(t.muted_foreground).child(format!(
                        "{} {net} key{} in your library. Keys already used in this wallet can't be picked twice.",
                        all.len(),
                        if all.len() == 1 { "" } else { "s" }
                    ))),
            )
            .when(!all.is_empty(), |d| d.child(self.search.clone()))
            .child(list)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        button("picker-add", "Add a new key…")
                            .outline()
                            .icon("plus")
                            .on_click(cx.listener(|this, _, w, cx| this.start_adding(w, cx))),
                    )
                    .child(
                        button("picker-cancel", "Cancel")
                            .ghost()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(KeyPickerEvent::Cancelled))),
                    ),
            )
            .into_any_element()
    }
}
