//! `setup/SlotEditor.tsx` — fill one key slot. The available sources
//! depend on the flow:
//!   create  — generate here, hardware, seed phrase, key file, paste
//!   recover — hardware, seed phrase, key file, paste (no generating:
//!             recovery restores keys that already exist)
//! When the slot came from a parsed descriptor (`slot.expected`), every
//! source must reproduce exactly that key; otherwise we'd be attaching a
//! private key the wallet can't use.

use gpui::{
    div, prelude::*, px, AsyncApp, Context, Entity, EventEmitter, FontWeight, SharedString,
    Subscription, Window,
};

use super::api::{self, AnalyzedSlot, DeriveInput, DerivedKey, DeviceInfo, HotSecret, SlotKey, SlotSource};
use super::device_picker::{DevicePicker, DevicePickerEvent};
use super::files::{open_text_file, OpenedFile};
use super::key_backup::{KeyBackup, KeyBackupEvent};
use super::keys::{classify_pasted_key, normalize_path, same_extended_key, same_path, same_slot_key, PastedKey};
use super::mnemonic::{MnemonicEvent, MnemonicInput};
use super::policy::{duplicate_of, with_account, FilledKey, Slot};
use super::ui::{
    alert_text, error_line, field_label, fingerprint, hint_text, option_card, para, primary_button,
    secondary_button, truncate_middle, AlertTone,
};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{InputEvent, TextInput};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Create,
    Recover,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Generate,
    Hardware,
    Seed,
    KeyFile,
    Paste,
}

const METHODS: [(Method, &str, &str); 5] = [
    (Method::Generate, "Generate here", "New 24-word key, backed up before use"),
    (Method::Hardware, "Hardware wallet", "Ledger, Trezor, BitBox, Jade, Coldcard"),
    (Method::Seed, "Seed phrase", "Type an existing BIP39 phrase"),
    (Method::KeyFile, "Key file", "Open a .sigvault-key backup"),
    (Method::Paste, "Paste key", "xpub with origin, or xprv"),
];

pub enum SlotEditorEvent {
    Fill(FilledKey),
    Close,
}

struct PendingKey {
    words: Vec<String>,
    derived: DerivedKey,
    backup: Entity<KeyBackup>,
}

pub struct SlotEditor {
    mode: Mode,
    slot: Slot,
    slots: Vec<Slot>,
    network: String,
    wallet_name: String,
    method: Method,
    path: Entity<TextInput>,
    error: Option<SharedString>,
    // generate
    gen_busy: bool,
    pending: Option<PendingKey>,
    // hardware
    devices: Entity<DevicePicker>,
    // seed
    mnemonic: Entity<MnemonicInput>,
    show_passphrase: bool,
    bip39_passphrase: Entity<TextInput>,
    seed_busy: bool,
    // key file
    key_file: Option<OpenedFile>,
    key_file_password: Entity<TextInput>,
    key_file_busy: bool,
    // paste
    paste: Entity<TextInput>,
    paste_busy: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<SlotEditorEvent> for SlotEditor {}

impl SlotEditor {
    pub fn new(
        mode: Mode,
        slot: Slot,
        slots: Vec<Slot>,
        network: String,
        wallet_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path_locked = slot.expected.is_some();
        let shown_path = if path_locked {
            slot.expected
                .as_ref()
                .and_then(|e| e.derivation_path.clone())
                .unwrap_or_else(|| "—".into())
        } else {
            slot.derivation_path.clone()
        };
        let path = cx.new(|cx| {
            let mut i = TextInput::new(window, cx).mono().with_text(shown_path);
            i.set_disabled(path_locked, cx);
            i
        });
        let hw_path = Self::hardware_path_for(&slot, &slot.derivation_path);
        let devices = cx.new(|_| DevicePicker::new(hw_path));
        let mnemonic = cx.new(|cx| MnemonicInput::new(24, window, cx));
        let bip39_passphrase = cx.new(|cx| TextInput::new(window, cx).masked().mono());
        let key_file_password =
            cx.new(|cx| TextInput::new(window, cx).masked().placeholder("File password"));
        let allow_public = !path_locked;
        let paste = cx.new(|cx| {
            TextInput::new(window, cx).mono().multiline(3).placeholder(if allow_public {
                "[fingerprint/48'/1'/0'/2']tpub…   or   tprv…"
            } else {
                "tprv…   or   [fingerprint/path]tprv…"
            })
        });

        let subs = vec![
            cx.subscribe(&path, |this, input, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Changed) {
                    let p = input.read(cx).text().to_string();
                    let hw = Self::hardware_path_for(&this.slot, &p);
                    this.devices.update(cx, |d, cx| d.set_path(hw, cx));
                    cx.notify();
                }
            }),
            cx.subscribe(&devices, |this, _, ev: &DevicePickerEvent, cx| match ev {
                DevicePickerEvent::Picked(info) => this.on_device(info.clone(), cx),
            }),
            cx.subscribe(&mnemonic, |this, _, ev: &MnemonicEvent, cx| match ev {
                MnemonicEvent::Changed => cx.notify(),
                MnemonicEvent::Submit => this.submit_seed(cx),
            }),
            cx.subscribe(&key_file_password, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Submit => this.submit_key_file(cx),
                InputEvent::Changed => cx.notify(),
            }),
            cx.subscribe(&bip39_passphrase, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Submit) {
                    this.submit_seed(cx);
                }
            }),
            cx.subscribe(&paste, |_, _, _: &InputEvent, cx| cx.notify()),
        ];

        Self {
            method: if mode == Mode::Create {
                Method::Generate
            } else {
                Method::Seed
            },
            mode,
            slot,
            slots,
            network,
            wallet_name,
            path,
            error: None,
            gen_busy: false,
            pending: None,
            devices,
            mnemonic,
            show_passphrase: false,
            bip39_passphrase,
            seed_busy: false,
            key_file: None,
            key_file_password,
            key_file_busy: false,
            paste,
            paste_busy: false,
            _subs: subs,
        }
    }

    pub fn slot_id(&self) -> &str {
        &self.slot.id
    }

    fn expected(&self) -> Option<&AnalyzedSlot> {
        self.slot.expected.as_ref()
    }

    /// A slot from a descriptor without origin info can't be re-derived
    /// from a seed: we'd have no idea which path produced it.
    fn no_origin(&self) -> bool {
        self.expected()
            .is_some_and(|e| e.fingerprint.is_none() || e.derivation_path.is_none())
    }

    fn hardware_path_for(slot: &Slot, path: &str) -> String {
        match &slot.expected {
            Some(e) => e.derivation_path.clone().unwrap_or_else(|| path.to_string()),
            None => path.to_string(),
        }
    }

    /// The editable derivation path (`path` state in the TSX).
    fn path(&self, cx: &gpui::App) -> String {
        if self.expected().is_some() {
            self.slot.derivation_path.clone()
        } else {
            self.path.read(cx).text().to_string()
        }
    }

    fn set_error(&mut self, e: Option<String>, cx: &mut Context<Self>) {
        self.error = e.map(Into::into);
        cx.notify();
    }

    /// Check a candidate key against what the slot requires, then fill.
    fn accept(
        &mut self,
        source: SlotSource,
        key: SlotKey,
        secret: Option<HotSecret>,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(expected) = self.expected() {
            if !same_slot_key(
                &key,
                expected.fingerprint.as_deref(),
                expected.derivation_path.as_deref(),
                &expected.xpub,
            ) {
                let fp_mismatch = expected
                    .fingerprint
                    .as_ref()
                    .is_some_and(|fp| key.fingerprint.to_lowercase() != fp.to_lowercase());
                let msg = if fp_mismatch {
                    format!(
                        "This is a different key (fingerprint {}). {} belongs to fingerprint {}.",
                        key.fingerprint,
                        self.slot.label,
                        expected.fingerprint.clone().unwrap_or_default()
                    )
                } else {
                    format!(
                        "Fingerprint matches, but the key at {} differs from the descriptor's. Check for a BIP39 passphrase or a different account.",
                        key.derivation_path
                    )
                };
                self.set_error(Some(msg), cx);
                return;
            }
        } else if let Some(dup) = duplicate_of(&self.slots, &self.slot.id, &key.xpub) {
            let msg = format!(
                "That exact key is already in {}. Each slot needs a distinct key.",
                dup.label
            );
            self.set_error(Some(msg), cx);
            return;
        }
        self.error = None;
        cx.emit(SlotEditorEvent::Fill(FilledKey {
            source,
            key: SlotKey {
                fingerprint: key.fingerprint.to_lowercase(),
                derivation_path: normalize_path(&key.derivation_path),
                xpub: key.xpub,
            },
            secret,
            detail,
        }));
        cx.notify();
    }

    // ── generate ──

    fn generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.gen_busy = true;
        self.error = None;
        cx.notify();
        let network = self.network.clone();
        let path = self.path(cx);
        let gen = api::generate_mnemonic(cx, 24);
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let res: Result<(Vec<String>, DerivedKey), String> = async {
                let words = gen.await?;
                let derive = cx
                    .update(|cx| {
                        api::derive_key(
                            cx,
                            DeriveInput {
                                network,
                                secret: HotSecret::Mnemonic {
                                    mnemonic: words.join(" "),
                                    bip39_passphrase: String::new(),
                                },
                                derivation_path: path,
                                origin_fingerprint: None,
                                origin_path: None,
                            },
                        )
                    })
                    .map_err(|e| e.to_string())?;
                Ok((words, derive.await?))
            }
            .await;
            window_handle
                .update(cx, |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.gen_busy = false;
                        match res {
                            Ok((words, derived)) => this.show_backup(words, derived, window, cx),
                            Err(e) => this.error = Some(e.into()),
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
        })
        .detach();
    }

    fn show_backup(&mut self, words: Vec<String>, derived: DerivedKey, window: &mut Window, cx: &mut Context<Self>) {
        let label = self.slot.label.clone();
        let name = self.wallet_name.clone();
        let (w, d) = (words.clone(), derived.clone());
        let backup = cx.new(|cx| KeyBackup::new(w, d, label, name, window, cx));
        self._subs.push(cx.subscribe(&backup, |this, _, ev: &KeyBackupEvent, cx| match ev {
            KeyBackupEvent::Discard => {
                this.pending = None;
                cx.notify();
            }
            KeyBackupEvent::Done => {
                if let Some(p) = &this.pending {
                    let secret = HotSecret::Mnemonic {
                        mnemonic: p.words.join(" "),
                        bip39_passphrase: String::new(),
                    };
                    let key = p.derived.slot_key();
                    this.accept(
                        SlotSource::Generated,
                        key,
                        Some(secret),
                        Some("Generated on this computer".into()),
                        cx,
                    );
                }
            }
        }));
        self.pending = Some(PendingKey { words, derived, backup });
    }

    // ── hardware ──

    fn on_device(&mut self, info: DeviceInfo, cx: &mut Context<Self>) {
        let key = SlotKey {
            fingerprint: info.fingerprint.clone(),
            derivation_path: info.derivation_path.clone(),
            xpub: info.xpub.clone(),
        };
        let mut chosen = key.clone();
        if let Some(expected) = self.expected() {
            if expected
                .fingerprint
                .as_ref()
                .is_some_and(|fp| fp.to_lowercase() == info.fingerprint.to_lowercase())
                && same_path(Some(&info.derivation_path), expected.derivation_path.as_deref())
                && !same_extended_key(&info.xpub, &expected.xpub)
            {
                self.set_error(
                    Some("The device returned a different key at this path. Is a passphrase wallet active on the device?".into()),
                    cx,
                );
                return;
            }
            // Keep the descriptor's encoding of the key when it matched.
            if same_slot_key(
                &key,
                expected.fingerprint.as_deref(),
                expected.derivation_path.as_deref(),
                &expected.xpub,
            ) {
                chosen.xpub = expected.xpub.clone();
            }
        }
        self.accept(SlotSource::Hardware, chosen, None, Some(info.device_type), cx);
    }

    // ── seed ──

    fn submit_seed(&mut self, cx: &mut Context<Self>) {
        if self.seed_busy || !self.mnemonic.read(cx).is_valid(cx) {
            return;
        }
        let secret = HotSecret::Mnemonic {
            mnemonic: self.mnemonic.read(cx).phrase(cx),
            bip39_passphrase: self.bip39_passphrase.read(cx).text().to_string(),
        };
        self.seed_busy = true;
        self.error = None;
        cx.notify();
        let network = self.network.clone();
        let path = self.path(cx);
        let expected_path = self.expected().map(|e| e.derivation_path.clone().unwrap_or_else(|| path.clone()));
        let slots = self.slots.clone();
        let slot_id = self.slot.id.clone();
        cx.spawn(async move |this, cx| {
            let res = match expected_path {
                Some(p) => derive(cx, &network, secret.clone(), p, None, None).await,
                None => derive_unique(cx, &network, secret.clone(), path, &slots, &slot_id).await,
            };
            this.update(cx, |this, cx| {
                this.seed_busy = false;
                match res {
                    Ok(d) => this.accept(SlotSource::Seed, d.slot_key(), Some(secret), Some("Seed phrase".into()), cx),
                    Err(e) => this.set_error(Some(e), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── key file ──

    fn pick_key_file(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = open_text_file(cx, "Open key backup").await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(Some(f)) => this.key_file = Some(f),
                    Ok(None) => {}
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn submit_key_file(&mut self, cx: &mut Context<Self>) {
        let Some(file) = &self.key_file else {
            return;
        };
        let password = self.key_file_password.read(cx).text().to_string();
        if password.is_empty() || self.key_file_busy {
            return;
        }
        self.key_file_busy = true;
        self.error = None;
        cx.notify();
        let name = file.name.clone();
        let decrypt = api::decrypt_key_backup(cx, file.contents.clone(), password);
        let network = self.network.clone();
        cx.spawn(async move |this, cx| {
            let res: Result<(DerivedKey, HotSecret), String> = async {
                let opened = decrypt.await?;
                let d = derive(
                    cx,
                    &network,
                    opened.secret.clone(),
                    opened.derivation_path.clone(),
                    Some(opened.fingerprint.clone()),
                    Some(opened.derivation_path.clone()),
                )
                .await?;
                Ok((d, opened.secret))
            }
            .await;
            this.update(cx, |this, cx| {
                this.key_file_busy = false;
                match res {
                    Ok((d, secret)) => this.accept(SlotSource::KeyFile, d.slot_key(), Some(secret), Some(name), cx),
                    Err(e) => this.set_error(Some(e), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── paste ──

    fn submit_paste(&mut self, cx: &mut Context<Self>) {
        let allow_public = self.expected().is_none();
        match classify_pasted_key(self.paste.read(cx).text()) {
            PastedKey::Public(key) if allow_public => {
                self.accept(SlotSource::Xpub, key, None, Some("Pasted public key".into()), cx)
            }
            PastedKey::Private(text) => {
                self.paste_busy = true;
                self.error = None;
                cx.notify();
                let network = self.network.clone();
                let path = self.path(cx);
                let (derivation_path, origin_fp, origin_path) = match self.expected() {
                    Some(e) => (
                        e.derivation_path.clone().unwrap_or(path),
                        e.fingerprint.clone(),
                        e.derivation_path.clone(),
                    ),
                    None => (path, None, None),
                };
                let secret = HotSecret::Xprv { xprv: text };
                cx.spawn(async move |this, cx| {
                    let res = derive(cx, &network, secret.clone(), derivation_path, origin_fp, origin_path).await;
                    this.update(cx, |this, cx| {
                        this.paste_busy = false;
                        match res {
                            Ok(d) => this.accept(
                                SlotSource::Xprv,
                                d.slot_key(),
                                Some(secret),
                                Some("Pasted private key".into()),
                                cx,
                            ),
                            Err(e) => this.set_error(Some(e), cx),
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            _ => {}
        }
    }

    // ── render ──

    fn render_generate(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if let Some(p) = &self.pending {
            return p.backup.clone().into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(para("Creates a fresh 24-word BIP39 key on this computer. You'll write it down and confirm it before it's used. It's stored encrypted under the wallet passphrase you set at the end."))
            .child(
                primary_button("generate-key", "Generate key")
                    .full_width()
                    .loading(self.gen_busy)
                    .on_click(cx.listener(|this, _, w, cx| this.generate(w, cx))),
            )
            .into_any_element()
    }

    fn render_hardware(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme();
        let path = Self::hardware_path_for(&self.slot, &self.path(cx));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(px(19.))
                    .text_color(t.muted_foreground)
                    .child("Connect and unlock the device, discover it, then continue. We read its public key at ")
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(t.foreground)
                            .child(path),
                    )
                    .child(". The private key stays on the device and signs there."),
            )
            .child(self.devices.clone())
            .into_any_element()
    }

    fn render_seed(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme();
        let valid = self.mnemonic.read(cx).is_valid(cx);
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.mnemonic.clone())
            .child(if self.show_passphrase {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(field_label(
                        "BIP39 passphrase",
                        Some("Only if this seed was set up with one. A different passphrase gives a completely different key.".into()),
                    ))
                    .child(self.bip39_passphrase.clone())
                    .into_any_element()
            } else {
                div()
                    .id("show-bip39")
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(t.foreground))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_passphrase = true;
                        cx.notify();
                    }))
                    .child("+ This seed uses a BIP39 passphrase")
                    .into_any_element()
            })
            .child(
                primary_button("use-seed", "Use this seed")
                    .full_width()
                    .loading(self.seed_busy)
                    .disabled(!valid)
                    .on_click(cx.listener(|this, _, _, cx| this.submit_seed(cx))),
            )
            .into_any_element()
    }

    fn render_key_file(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme();
        let has_password = !self.key_file_password.read(cx).text().is_empty();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        secondary_button(
                            "choose-key-file",
                            if self.key_file.is_some() { "Choose another file" } else { "Choose .sigvault-key file" },
                        )
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.pick_key_file(cx))),
                    )
                    .children(self.key_file.as_ref().map(|f| {
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(t.muted_foreground)
                            .child(f.name.clone())
                    })),
            )
            .when(self.key_file.is_some(), |d| {
                d.child(self.key_file_password.clone()).child(
                    primary_button("decrypt-key", "Decrypt and use")
                        .full_width()
                        .loading(self.key_file_busy)
                        .disabled(!has_password)
                        .on_click(cx.listener(|this, _, _, cx| this.submit_key_file(cx))),
                )
            })
            .into_any_element()
    }

    fn render_paste(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme();
        let allow_public = self.expected().is_none();
        let parsed = classify_pasted_key(self.paste.read(cx).text());
        let public_blocked = matches!(parsed, PastedKey::Public(_)) && !allow_public;
        let invalid = matches!(parsed, PastedKey::Invalid(_));
        let info = match &parsed {
            PastedKey::Invalid(reason) if !reason.is_empty() => Some(error_line(reason.clone()).into_any_element()),
            PastedKey::Public(key) if allow_public => Some(
                div()
                    .text_size(px(11.))
                    .line_height(px(17.))
                    .text_color(t.muted_foreground)
                    .child(format!(
                        "Public key · {} · {} · {}. This slot will be watch-only here; its owner signs elsewhere.",
                        key.fingerprint,
                        key.derivation_path,
                        truncate_middle(&key.xpub, 10, 6)
                    ))
                    .into_any_element(),
            ),
            PastedKey::Public(_) => Some(
                hint_text("This slot already has its public key from the descriptor. Paste the private key to attach it.")
                    .into_any_element(),
            ),
            PastedKey::Private(_) => Some(
                hint_text("Private key. A master xprv is derived at the slot's path; an account xprv needs its [fingerprint/path] origin unless the slot already defines one.")
                    .into_any_element(),
            ),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.paste.clone())
            .children(info)
            .child(
                primary_button("use-pasted", "Use this key")
                    .full_width()
                    .loading(self.paste_busy)
                    .disabled(invalid || public_blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.submit_paste(cx))),
            )
            .into_any_element()
    }
}

impl Render for SlotEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let expected = self.slot.expected.clone();
        let no_origin = self.no_origin();
        let methods: Vec<_> = METHODS
            .iter()
            .filter(|(m, _, _)| self.mode == Mode::Create || *m != Method::Generate)
            .collect();
        let method = self.method;

        let subtitle = match &expected {
            Some(e) => {
                let mut row = div()
                    .mt_1()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(4.))
                    .text_size(px(11.))
                    .text_color(t.muted_foreground)
                    .child("Expecting");
                row = match &e.fingerprint {
                    Some(fp) => row.child(fingerprint(fp)),
                    None => row.child("a key without origin info"),
                };
                if let Some(p) = &e.derivation_path {
                    row = row.child("at").child(div().font_family(FONT_MONO).child(p.clone()));
                }
                row
            }
            None => div()
                .mt_1()
                .text_size(px(11.))
                .text_color(t.muted_foreground)
                .child("Choose where this key comes from."),
        };

        let body = match method {
            Method::Generate => Some(self.render_generate(cx)),
            Method::Hardware => Some(self.render_hardware(cx)),
            Method::Seed if !no_origin => Some(self.render_seed(cx)),
            Method::Seed => None,
            Method::KeyFile => Some(self.render_key_file(cx)),
            Method::Paste => Some(self.render_paste(cx)),
        };

        div()
            .flex()
            .flex_col()
            .gap_5()
            .rounded_lg()
            .border_1()
            .border_color(alpha(t.primary, 0.4))
            .bg(t.card)
            .p_5()
            .shadow_sm()
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
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child(self.slot.label.clone()),
                            )
                            .child(subtitle),
                    )
                    .child(
                        div()
                            .id("close-editor")
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.muted_foreground)
                            .cursor_pointer()
                            .hover(move |s| s.text_color(t.foreground))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(SlotEditorEvent::Close)))
                            .child("Close"),
                    ),
            )
            .when(no_origin, |d| {
                d.child(alert_text(
                    AlertTone::Warning,
                    "The descriptor doesn't record where this key came from, so a seed phrase can't be matched to it. Paste the matching account-level xprv instead, or leave this slot watch-only.",
                ))
            })
            .child(div().flex().flex_row().gap(px(6.)).children(methods.into_iter().map(|(m, title, hint)| {
                let m = *m;
                option_card(SharedString::from(format!("method-{title}")), method == m)
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .px(px(10.))
                    .py_2()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.method = m;
                        this.error = None;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(*title),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(10.))
                            .line_height(px(13.))
                            .text_color(t.muted_foreground)
                            .child(*hint),
                    )
            })))
            .when(method != Method::KeyFile && method != Method::Paste, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(t.muted_foreground)
                                .child("Derivation path"),
                        )
                        .child(div().flex_1().min_w_0().child(self.path.clone())),
                )
            })
            .children(self.error.clone().map(|e| alert_text(AlertTone::Error, e)))
            .children(body)
            .when(expected.is_some(), |d| {
                d.child(hint_text("No private key for this slot? Close this panel. The slot stays watch-only and can still co-sign later with a hardware wallet or an exported PSBT."))
            })
    }
}

async fn derive(
    cx: &mut AsyncApp,
    network: &str,
    secret: HotSecret,
    derivation_path: String,
    origin_fingerprint: Option<String>,
    origin_path: Option<String>,
) -> Result<DerivedKey, String> {
    let input = DeriveInput {
        network: network.to_string(),
        secret,
        derivation_path,
        origin_fingerprint,
        origin_path,
    };
    let fut = cx
        .update(|cx| api::derive_key(cx, input))
        .map_err(|e| e.to_string())?;
    fut.await
}

/// Derive from a secret, stepping to the next account if this seed
/// already fills another slot at the same path.
async fn derive_unique(
    cx: &mut AsyncApp,
    network: &str,
    secret: HotSecret,
    path: String,
    slots: &[Slot],
    slot_id: &str,
) -> Result<DerivedKey, String> {
    let mut attempt = path.clone();
    for account in 0..20u32 {
        let derived = derive(cx, network, secret.clone(), attempt.clone(), None, None).await?;
        if duplicate_of(slots, slot_id, &derived.xpub).is_none() {
            return Ok(derived);
        }
        match with_account(&path, account + 1) {
            Some(bumped) => attempt = bumped,
            // Custom path: let accept() report the duplicate.
            None => return Ok(derived),
        }
    }
    Err("Couldn't find an unused account for this seed.".into())
}
