//! `/local/keys` — the key library: every key the user has added, the
//! keychain lock state, and which wallets use each key.

use std::collections::HashMap;

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Subscription, Window};
use sigvault_core::local_wallet::commands::cmd_local_list_wallets;
use sigvault_core::local_wallet::manager::WalletSummary;
use sigvault_core::State;

use super::add::open_add_key;
use super::sources::{icon_tile, network_badge, source_icon, KeySource, NETWORKS};
use super::unlock::{KeychainUnlock, UnlockEvent};
use crate::keychain::{keychain, source_label, KeyRecord, KeychainEvent};
use crate::router::Page;
use crate::runtime::backend;
use crate::shell::navigate;
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, button, card, divider, icon, modal, page_frame, tabs, Select, SelectEvent, SelectOption,
    Tone,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Private,
    Public,
}

impl Filter {
    fn key(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Private => "private",
            Filter::Public => "public",
        }
    }
    fn matches(self, k: &KeyRecord) -> bool {
        match self {
            Filter::All => true,
            Filter::Private => k.is_hot(),
            Filter::Public => !k.is_hot(),
        }
    }
}

/// Wallets that include a key, by `fingerprint/network`.
pub type UsageMap = HashMap<String, Vec<WalletSummary>>;

fn usage_key(fingerprint: &str, network: &str) -> String {
    format!("{}/{}", fingerprint.to_lowercase(), network)
}

/// Load wallets and index them by the fingerprints they contain.
pub fn load_usage(cx: &mut gpui::App) -> gpui::Task<Result<UsageMap, String>> {
    let task = backend(cx)
        .call(|app, st| async move { cmd_local_list_wallets(app, State::new(&st)).await });
    cx.spawn(async move |_| {
        let wallets = task.await?;
        let mut map: UsageMap = HashMap::new();
        for w in wallets {
            let mut fps: Vec<String> = w.fingerprints.iter().map(|f| f.to_lowercase()).collect();
            fps.dedup();
            for fp in fps {
                map.entry(usage_key(&fp, &w.network))
                    .or_default()
                    .push(w.clone());
            }
        }
        Ok(map)
    })
}

pub fn wallets_using<'a>(usage: &'a UsageMap, k: &KeyRecord) -> &'a [WalletSummary] {
    usage
        .get(&usage_key(&k.fingerprint, &k.network))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub struct KeyListPage {
    filter: Filter,
    network: Entity<Select>,
    usage: Option<UsageMap>,
    unlock: Option<Entity<KeychainUnlock>>,
    _subs: Vec<Subscription>,
}

impl KeyListPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut options = vec![SelectOption::new("all", "All networks")];
        options.extend(
            NETWORKS
                .iter()
                .map(|(id, label)| SelectOption::new(*id, *label)),
        );
        let network = cx.new(|_| Select::new(options).with_value("all"));
        let subs = vec![
            cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()),
            cx.subscribe(&network, |_, _, _: &SelectEvent, cx| cx.notify()),
            cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| {
                if matches!(ev, StoreEvent::LocalDataChanged) {
                    this.refresh_usage(cx);
                }
            }),
        ];
        let mut this = Self {
            filter: Filter::All,
            network,
            usage: None,
            unlock: None,
            _subs: subs,
        };
        this.refresh_usage(cx);
        this
    }

    fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        let task = load_usage(cx);
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                this.usage = Some(res.unwrap_or_default());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let unlock = cx.new(|cx| KeychainUnlock::new("to use private keys", window, cx));
        self._subs
            .push(cx.subscribe(&unlock, |this, _, _: &UnlockEvent, cx| {
                this.unlock = None;
                cx.notify();
            }));
        self.unlock = Some(unlock);
        cx.notify();
    }

    fn render_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let kc = keychain(cx).read(cx);
        let hot = kc.hot_count();
        let (icon_name, color, title, body): (&str, _, SharedString, SharedString) = if !kc
            .has_password()
        {
            (
                "shield",
                t.muted_foreground,
                "Keychain not set up".into(),
                "You'll create a keychain password when you add your first private key. It encrypts every private key in this library.".into(),
            )
        } else if kc.is_unlocked() {
            (
                "unlock",
                t.success,
                "Keychain unlocked".into(),
                format!(
                    "{hot} private {} available to sign and create wallets this session.",
                    if hot == 1 { "key is" } else { "keys are" }
                )
                .into(),
            )
        } else {
            (
                "lock",
                t.muted_foreground,
                "Keychain locked".into(),
                format!(
                    "{hot} private {} encrypted. Unlock to create wallets from {} or export backups.",
                    if hot == 1 { "key is" } else { "keys are" },
                    if hot == 1 { "it" } else { "them" }
                )
                .into(),
            )
        };
        let action = if !kc.has_password() {
            None
        } else if kc.is_unlocked() {
            Some(
                button("kc-lock", "Lock")
                    .outline()
                    .small()
                    .icon("lock")
                    .on_click(|_, _, cx| keychain(cx).update(cx, |k, cx| k.lock(cx)))
                    .into_any_element(),
            )
        } else {
            Some(
                button("kc-unlock", "Unlock")
                    .small()
                    .icon("unlock")
                    .on_click(cx.listener(|this, _, window, cx| this.open_unlock(window, cx)))
                    .into_any_element(),
            )
        };
        card()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .child(icon_tile(icon_name, 32., Some(color)))
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
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(body),
                    ),
            )
            .children(action)
    }

    fn render_row(&self, ix: usize, k: &KeyRecord) -> impl IntoElement {
        let t = theme();
        let used = self.usage.as_ref().map(|u| wallets_using(u, k).len());
        let used_text: SharedString = match used {
            None => "".into(),
            Some(0) => "Not used yet".into(),
            Some(1) => "Used by 1 wallet".into(),
            Some(n) => format!("Used by {n} wallets").into(),
        };
        let page = Page::LocalKey(k.id.clone());
        let meta = match &k.device {
            Some(d) if k.source == "hardware" => format!("{d} · {}", k.derivation_path),
            _ => format!("{} · {}", source_label(&k.source), k.derivation_path),
        };
        div()
            .id(("key-row", ix))
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .h(px(60.))
            .px_4()
            .cursor_pointer()
            .hover(move |s| s.bg(alpha(t.muted, 0.5)))
            .on_click(move |_, _, cx| navigate(page.clone(), cx))
            .child(icon_tile(
                source_icon(&k.source),
                32.,
                Some(if k.is_hot() {
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
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.foreground)
                                    .child(k.label.clone()),
                            )
                            .when(!k.is_hot(), |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .text_size(px(11.))
                                        .text_color(alpha(t.muted_foreground, 0.85))
                                        .child("Watch-only"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .w(px(96.))
                    .flex_none()
                    .flex()
                    .child(network_badge(&k.network)),
            )
            .child(
                div()
                    .w(px(80.))
                    .flex_none()
                    .font_family(FONT_MONO)
                    .text_size(px(12.))
                    .text_color(t.foreground)
                    .child(k.fingerprint.clone()),
            )
            .child(
                div()
                    .w(px(120.))
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(if used.unwrap_or(0) > 0 {
                        t.foreground
                    } else {
                        t.muted_foreground
                    })
                    .child(used_text),
            )
            .child(icon("chevron-right").size(px(14.)))
    }

    fn render_empty(&self) -> impl IntoElement {
        let t = theme();
        card()
            .items_center()
            .gap_2()
            .py_12()
            .px_6()
            .child(icon_tile("key", 44., Some(t.primary)))
            .child(
                div()
                    .mt_2()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Your key library is empty"),
            )
            .child(
                div()
                    .max_w(px(460.))
                    .text_center()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(t.muted_foreground)
                    .child("Add each signing key once — a new seed, a recovery phrase you already have, or a hardware wallet — then pick from them whenever you create or restore a wallet."),
            )
            .child(
                div()
                    .mt_4()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        button("empty-generate", "Generate new key")
                            .icon("sparkles")
                            .on_click(|_, _, cx| open_add_key(Some(KeySource::Generate), cx)),
                    )
                    .child(
                        button("empty-seed", "Import recovery phrase")
                            .outline()
                            .icon("scroll-text")
                            .on_click(|_, _, cx| open_add_key(Some(KeySource::Seed), cx)),
                    )
                    .child(
                        button("empty-hw", "Connect hardware wallet")
                            .outline()
                            .icon("hw-device")
                            .on_click(|_, _, cx| open_add_key(Some(KeySource::Hardware), cx)),
                    ),
            )
    }
}

impl Render for KeyListPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let kc = keychain(cx).read(cx);
        let all: Vec<KeyRecord> = kc.keys().to_vec();
        let load_error = kc.load_error().map(str::to_string);
        let net = self
            .network
            .read(cx)
            .value()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "all".into());
        let in_net: Vec<&KeyRecord> = all
            .iter()
            .filter(|k| net == "all" || k.network == net)
            .collect();
        let count = |f: Filter| in_net.iter().filter(|k| f.matches(k)).count();
        let tab_items: [(&'static str, SharedString); 3] = [
            ("all", format!("All  {}", count(Filter::All)).into()),
            (
                "private",
                format!("On this computer  {}", count(Filter::Private)).into(),
            ),
            (
                "public",
                format!("Hardware & watch-only  {}", count(Filter::Public)).into(),
            ),
        ];
        let mut shown: Vec<&KeyRecord> = in_net
            .iter()
            .copied()
            .filter(|k| self.filter.matches(k))
            .collect();
        shown.sort_by_key(|a| a.label.to_lowercase());

        let filters = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .w(px(500.))
                    .child(tabs("key-filter", &tab_items, self.filter.key(), {
                        let this = cx.entity().downgrade();
                        move |key: &'static str, _, cx: &mut gpui::App| {
                            this.update(cx, |this, cx| {
                                this.filter = match key {
                                    "private" => Filter::Private,
                                    "public" => Filter::Public,
                                    _ => Filter::All,
                                };
                                cx.notify();
                            })
                            .ok();
                        }
                    })),
            )
            .child(div().w(px(180.)).child(self.network.clone()));

        let body = if all.is_empty() {
            self.render_empty().into_any_element()
        } else if shown.is_empty() {
            card()
                .items_center()
                .py_10()
                .gap_1()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child("No keys match these filters"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Try another network or show all keys."),
                )
                .into_any_element()
        } else {
            let mut list = card().overflow_hidden();
            for (i, k) in shown.iter().enumerate() {
                if i > 0 {
                    list = list.child(divider());
                }
                list = list.child(self.render_row(i, k));
            }
            list.into_any_element()
        };

        let frame = page_frame("keys")
            .title("Key library")
            .subtitle("Signing keys you can combine into wallets. Add a key once, then pick it when you create or restore a wallet.")
            .action(
                button("add-key", "Add key")
                    .icon("plus")
                    .on_click(|_, _, cx| open_add_key(None, cx)),
            )
            .when(!all.is_empty(), |f| f.header_extra(filters))
            .children(load_error.map(|e| alert(Tone::Destructive, e)))
            .when(!all.is_empty(), |f| f.child(self.render_status(cx)))
            .child(body);

        div()
            .size_full()
            .relative()
            .child(frame)
            .children(self.unlock.clone().map(|u| {
                modal("kc-unlock-modal", 420.)
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.unlock = None;
                        cx.notify();
                    }))
                    .child(u)
            }))
    }
}
