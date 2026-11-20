//! Embeddable "add a key to the library" flow.
//!
//! INTERFACE (stable — the wallet wizard embeds this in a modal):
//! - `AddKeyFlow::new(network: Option<String>, window, cx)` — `Some(net)`
//!   preselects and locks the network (wizard knows the wallet's network);
//!   `None` lets the user choose.
//! - emits [`AddKeyEvent::Added(key_id)`] once the key is saved in the
//!   keychain, or [`AddKeyEvent::Cancelled`].
//! - renders only its body (no page frame); hosts supply the frame/modal.
//!
//! Optional host hooks (used by the full-page host, `add.rs`):
//! - [`AddKeyFlow::step`] for a stepper,
//! - [`AddKeyFlow::set_hosted`] + [`AddKeyFlow::footer`] /
//!   [`AddKeyFlow::overlay`] to render the Back/Continue bar in the page
//!   footer and the keychain-unlock dialog as a page-level modal. Without
//!   them the flow renders its own action row and an inline unlock card.
//!
//! Steps: 1. choose a source → 2. produce the key (per-source panel, incl.
//! the write-down + quiz backup for generated seeds) → 3. details (label,
//! network, derivation path, fingerprint/xpub preview) → save.

use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, EventEmitter, FontWeight, SharedString,
    Subscription, Window,
};
use sigvault_core::local_wallet::commands::cmd_local_get_settings;
use sigvault_core::State;

use super::sources::{
    default_path, icon_tile, key_expr, network_badge, to_core_secret, with_origin, KeySource,
    PathPreset, NETWORKS,
};
use super::unlock::{KeychainUnlock, UnlockEvent};
use crate::e2e::{E2eClick, Probe};
use crate::keychain::{keychain, NewKey};
use crate::pages::local::setup::api::{self, DeriveInput, DerivedKey, HotSecret, SlotKey};
use crate::pages::local::setup::device_picker::{DevicePicker, DevicePickerEvent};
use crate::pages::local::setup::files::{open_text_file, OpenedFile};
use crate::pages::local::setup::key_backup::{KeyBackup, KeyBackupEvent};
use crate::pages::local::setup::keys::{
    classify_pasted_key, extended_key_depth, is_test_network_key, normalize_path, PastedKey,
};
use crate::pages::local::setup::mnemonic::{MnemonicEvent, MnemonicInput};
use crate::pages::local::setup::ui::{seg, segmented};
use crate::runtime::backend;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, badge, button, card, choice_card, divider, field, footer_bar, icon, modal, spinner,
    InputEvent, TextInput, Tone, FORM_WIDTH,
};

#[derive(Debug, Clone)]
pub enum AddKeyEvent {
    Added(String),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddKeyStep {
    Source,
    Key,
    Details,
}

impl AddKeyStep {
    pub const LABELS: [&'static str; 3] = ["Source", "Key", "Details"];

    pub fn index(self) -> usize {
        match self {
            AddKeyStep::Source => 0,
            AddKeyStep::Key => 1,
            AddKeyStep::Details => 2,
        }
    }
}

/// What step 2 produced.
#[derive(Clone)]
enum Material {
    /// Seed or master xprv: derivable at any path (editable in Details).
    Derivable(HotSecret),
    /// Account-level private key with a fixed origin.
    Account { secret: HotSecret, key: SlotKey },
    /// Public key only (hardware wallet, pasted xpub).
    Public {
        key: SlotKey,
        device: Option<String>,
    },
}

impl Material {
    fn secret(&self) -> Option<&HotSecret> {
        match self {
            Material::Derivable(s) | Material::Account { secret: s, .. } => Some(s),
            Material::Public { .. } => None,
        }
    }
}

pub struct AddKeyFlow {
    network: String,
    network_locked: bool,
    network_touched: bool,
    step: AddKeyStep,
    source: Option<KeySource>,
    hosted: bool,
    error: Option<SharedString>,
    busy: bool,
    // shared
    path: Entity<TextInput>,
    show_advanced: bool,
    label: Entity<TextInput>,
    focus_label: bool,
    // generate
    backup: Option<Entity<KeyBackup>>,
    // seed
    mnemonic: Entity<MnemonicInput>,
    show_passphrase: bool,
    bip39_passphrase: Entity<TextInput>,
    // xprv
    xprv: Entity<TextInput>,
    // key file
    key_file: Option<OpenedFile>,
    key_file_password: Entity<TextInput>,
    // hardware
    devices: Entity<DevicePicker>,
    // xpub
    xpub: Entity<TextInput>,
    // details
    material: Option<Material>,
    preview: Option<SlotKey>,
    preview_error: Option<SharedString>,
    deriving: bool,
    derive_epoch: u64,
    saving: bool,
    unlock: Option<Entity<KeychainUnlock>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<AddKeyEvent> for AddKeyFlow {}

impl AddKeyFlow {
    pub fn new(network: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let network_locked = network.is_some();
        let network = network.unwrap_or_else(|| "regtest".into());
        let path = cx.new(|cx| {
            TextInput::new(window, cx)
                .mono()
                .label("Derivation path")
                .with_text(default_path(&network))
        });
        let label = cx.new(|cx| {
            TextInput::new(window, cx)
                .max_len(60)
                .placeholder("e.g. Laptop key, Trezor (Alice), Backup seed")
        });
        let mnemonic = cx.new(|cx| MnemonicInput::new(24, window, cx));
        let bip39_passphrase = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .mono()
                .label("BIP39 passphrase")
        });
        let xprv = cx.new(|cx| {
            TextInput::new(window, cx)
                .mono()
                .multiline(3)
                .placeholder("tprv…   or   [fingerprint/48'/1'/0'/2']tprv…")
        });
        let key_file_password = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Password used when the file was saved")
        });
        let devices = cx.new(|_| DevicePicker::new(default_path(&network)));
        let xpub = cx.new(|cx| {
            TextInput::new(window, cx)
                .mono()
                .multiline(3)
                .placeholder("[fingerprint/48'/1'/0'/2']tpub…")
        });

        let subs = vec![
            cx.subscribe(&path, |this, input, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Changed) {
                    let p = input.read(cx).text().to_string();
                    this.devices.update(cx, |d, cx| d.set_path(p, cx));
                    if this.step == AddKeyStep::Details {
                        this.rederive(cx);
                    }
                    cx.notify();
                }
            }),
            cx.subscribe(&label, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Changed => {
                    this.error = None;
                    cx.notify();
                }
                InputEvent::Submit => this.save(None, cx),
            }),
            cx.subscribe(&mnemonic, |this, _, ev: &MnemonicEvent, cx| match ev {
                MnemonicEvent::Changed => {
                    this.error = None;
                    cx.notify();
                }
                MnemonicEvent::Submit => this.submit_key(cx),
            }),
            cx.subscribe(&bip39_passphrase, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Submit) {
                    this.submit_key(cx);
                }
            }),
            cx.subscribe(&xprv, |this, _, _: &InputEvent, cx| {
                this.error = None;
                cx.notify();
            }),
            cx.subscribe(
                &key_file_password,
                |this, _, ev: &InputEvent, cx| match ev {
                    InputEvent::Submit => this.submit_key(cx),
                    InputEvent::Changed => {
                        this.error = None;
                        cx.notify();
                    }
                },
            ),
            cx.subscribe(&devices, |this, _, ev: &DevicePickerEvent, cx| match ev {
                DevicePickerEvent::Picked(info) => {
                    let key = SlotKey {
                        fingerprint: info.fingerprint.to_lowercase(),
                        derivation_path: normalize_path(&info.derivation_path),
                        xpub: info.xpub.clone(),
                    };
                    let device = info.device_type.clone();
                    if this.label.read(cx).text().trim().is_empty() {
                        let l = device.clone();
                        this.label.update(cx, |i, cx| i.set_text(l, cx));
                    }
                    this.go_details(
                        Material::Public {
                            key,
                            device: Some(device),
                        },
                        cx,
                    );
                }
            }),
            cx.subscribe(&xpub, |this, _, _: &InputEvent, cx| {
                this.error = None;
                cx.notify();
            }),
        ];

        let mut this = Self {
            network,
            network_locked,
            network_touched: false,
            step: AddKeyStep::Source,
            source: None,
            hosted: false,
            error: None,
            busy: false,
            path,
            show_advanced: false,
            label,
            focus_label: false,
            backup: None,
            mnemonic,
            show_passphrase: false,
            bip39_passphrase,
            xprv,
            key_file: None,
            key_file_password,
            devices,
            xpub,
            material: None,
            preview: None,
            preview_error: None,
            deriving: false,
            derive_epoch: 0,
            saving: false,
            unlock: None,
            _subs: subs,
        };
        if !network_locked {
            this.load_default_network(cx);
        }
        this
    }

    // ── host hooks ──

    /// Debug builds: prefill the paste box of the current source and
    /// submit (screenshots of the Details step).
    pub fn dev_submit_text(&mut self, text: String, cx: &mut Context<Self>) {
        if !cfg!(debug_assertions) {
            return;
        }
        match self.source {
            Some(KeySource::Xpub) => self.xpub.update(cx, |i, cx| i.set_text(text, cx)),
            Some(KeySource::Xprv) => self.xprv.update(cx, |i, cx| i.set_text(text, cx)),
            _ => return,
        }
        self.submit_key(cx);
    }

    pub fn step(&self) -> AddKeyStep {
        self.step
    }

    /// The host renders [`Self::footer`] and [`Self::overlay`] itself.
    pub fn set_hosted(&mut self, hosted: bool, cx: &mut Context<Self>) {
        self.hosted = hosted;
        cx.notify();
    }

    /// Jump straight to a source's panel (e.g. from an empty-state CTA).
    pub fn choose(&mut self, source: KeySource, window: &mut Window, cx: &mut Context<Self>) {
        self.source = Some(source);
        self.step = AddKeyStep::Key;
        self.error = None;
        self.busy = false;
        match source {
            KeySource::Xprv => self.xprv.read(cx).focus(window),
            KeySource::Xpub => self.xpub.read(cx).focus(window),
            _ => {}
        }
        cx.notify();
    }

    fn load_default_network(&mut self, cx: &mut Context<Self>) {
        let task = backend(cx)
            .call(|app, st| async move { cmd_local_get_settings(app, State::new(&st)).await });
        cx.spawn(async move |this, cx| {
            let Ok(settings) = task.await else { return };
            this.update(cx, |this, cx| {
                let net = settings.default_network;
                if !this.network_touched && NETWORKS.iter().any(|(n, _)| *n == net) {
                    this.set_network(net, false, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn set_network(&mut self, network: String, by_user: bool, cx: &mut Context<Self>) {
        if self.network_locked || self.network == network {
            return;
        }
        // Keep the path's preset when switching networks.
        let preset = PathPreset::detect(self.path.read(cx).text(), &self.network);
        self.network = network;
        self.network_touched |= by_user;
        if let Some(p) = preset.path(&self.network) {
            self.path.update(cx, |i, cx| i.set_text(p, cx));
        }
        if self.step == AddKeyStep::Details {
            self.rederive(cx);
        }
        cx.notify();
    }

    fn current_path(&self, cx: &App) -> String {
        normalize_path(self.path.read(cx).text())
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.unlock = None;
        match self.step {
            AddKeyStep::Source => cx.emit(AddKeyEvent::Cancelled),
            AddKeyStep::Key => {
                self.step = AddKeyStep::Source;
                self.source = None;
                self.backup = None;
                self.busy = false;
            }
            AddKeyStep::Details => {
                self.step = AddKeyStep::Key;
                self.material = None;
                self.preview = None;
                self.preview_error = None;
            }
        }
        cx.notify();
    }

    fn go_details(&mut self, material: Material, cx: &mut Context<Self>) {
        self.material = Some(material);
        self.preview = None;
        self.preview_error = None;
        self.error = None;
        self.busy = false;
        self.step = AddKeyStep::Details;
        self.focus_label = true;
        self.rederive(cx);
        cx.notify();
    }

    // ── step 2: produce key material ──

    /// Footer "Continue" for the current source.
    fn submit_key(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.step != AddKeyStep::Key {
            return;
        }
        self.error = None;
        match self.source {
            Some(KeySource::Seed) => {
                if !self.mnemonic.read(cx).is_valid(cx) {
                    return;
                }
                let secret = HotSecret::Mnemonic {
                    mnemonic: self.mnemonic.read(cx).phrase(cx),
                    bip39_passphrase: self.bip39_passphrase.read(cx).text().to_string(),
                };
                self.go_details(Material::Derivable(secret), cx);
            }
            Some(KeySource::Xprv) => self.submit_xprv(cx),
            Some(KeySource::KeyFile) => self.submit_key_file(cx),
            Some(KeySource::Xpub) => match classify_pasted_key(self.xpub.read(cx).text()) {
                PastedKey::Public(key) => {
                    if !is_test_network_key(&key.xpub) {
                        self.error = Some(
                            "This is a mainnet key. Local wallets only run on test networks (regtest, signet, testnet4), whose keys start with tpub.".into(),
                        );
                    } else {
                        self.go_details(Material::Public { key, device: None }, cx);
                    }
                }
                PastedKey::Invalid(reason) if !reason.is_empty() => {
                    self.error = Some(reason.into())
                }
                _ => {}
            },
            _ => {}
        }
        cx.notify();
    }

    fn submit_xprv(&mut self, cx: &mut Context<Self>) {
        let PastedKey::Private(text) = classify_pasted_key(self.xprv.read(cx).text()) else {
            return;
        };
        let bare: String = text
            .rsplit(']')
            .next()
            .unwrap_or(&text)
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !is_test_network_key(&bare) {
            self.error = Some("This is a mainnet key. Local wallets only run on test networks, whose keys start with tprv.".into());
            return;
        }
        match extended_key_depth(&bare) {
            None => self.error = Some("That extended key is malformed.".into()),
            Some(0) => {
                self.go_details(Material::Derivable(HotSecret::Xprv { xprv: bare }), cx);
            }
            Some(_) => {
                // Account key: the origin fixes the path; derive once to
                // validate it and learn the xpub. Drop any `/<0;1>/*` tail.
                let origin = text
                    .find(']')
                    .map(|i| text[..=i].to_string())
                    .unwrap_or_default();
                let secret = HotSecret::Xprv {
                    xprv: format!("{origin}{bare}"),
                };
                self.busy = true;
                let task = api::derive_key(
                    cx,
                    DeriveInput {
                        network: self.network.clone(),
                        secret: secret.clone(),
                        derivation_path: self.current_path(cx),
                        origin_fingerprint: None,
                        origin_path: None,
                    },
                );
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |this, cx| {
                        this.busy = false;
                        match res {
                            Ok(d) => {
                                let key = d.slot_key();
                                let secret = with_origin(secret, &key);
                                this.go_details(Material::Account { secret, key }, cx);
                            }
                            Err(e) => this.error = Some(friendly(&e).into()),
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
        }
    }

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
        let Some(file) = &self.key_file else { return };
        let password = self.key_file_password.read(cx).text().to_string();
        if password.is_empty() {
            return;
        }
        self.busy = true;
        let decrypt = api::decrypt_key_backup(cx, file.contents.clone(), password);
        let network = self.network.clone();
        cx.spawn(async move |this, cx| {
            let res: Result<(HotSecret, DerivedKey), String> = async {
                let opened = decrypt.await?;
                let derive = cx
                    .update(|cx| {
                        api::derive_key(
                            cx,
                            DeriveInput {
                                network,
                                secret: opened.secret.clone(),
                                derivation_path: opened.derivation_path.clone(),
                                origin_fingerprint: Some(opened.fingerprint.clone()),
                                origin_path: Some(opened.derivation_path.clone()),
                            },
                        )
                    })
                    .map_err(|e| e.to_string())?;
                Ok((opened.secret, derive.await?))
            }
            .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match res {
                    Ok((secret, d)) => {
                        let key = d.slot_key();
                        let material = match secret {
                            HotSecret::Mnemonic { .. } => {
                                // Keep the path the file was saved with.
                                let p = key.derivation_path.clone();
                                this.path.update(cx, |i, cx| i.set_text(p, cx));
                                Material::Derivable(secret)
                            }
                            HotSecret::Xprv { .. }
                                if extended_key_depth(&strip_origin(&secret)) == Some(0) =>
                            {
                                let p = key.derivation_path.clone();
                                this.path.update(cx, |i, cx| i.set_text(p, cx));
                                Material::Derivable(secret)
                            }
                            HotSecret::Xprv { .. } => Material::Account {
                                secret: with_origin(secret, &key),
                                key,
                            },
                        };
                        this.go_details(material, cx);
                    }
                    Err(e) => this.error = Some(friendly(&e).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let network = self.network.clone();
        let path = self.current_path(cx);
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
                        this.busy = false;
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

    fn show_backup(
        &mut self,
        words: Vec<String>,
        derived: DerivedKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let secret = HotSecret::Mnemonic {
            mnemonic: words.join(" "),
            bip39_passphrase: String::new(),
        };
        let backup = cx.new(|cx| KeyBackup::for_library(words, derived, window, cx));
        self._subs.push(
            cx.subscribe(&backup, move |this, _, ev: &KeyBackupEvent, cx| match ev {
                KeyBackupEvent::Discard => {
                    this.backup = None;
                    cx.notify();
                }
                KeyBackupEvent::Done => this.go_details(Material::Derivable(secret.clone()), cx),
            }),
        );
        self.backup = Some(backup);
    }

    // ── step 3: details ──

    fn rederive(&mut self, cx: &mut Context<Self>) {
        let Some(material) = self.material.clone() else {
            return;
        };
        self.derive_epoch += 1;
        let epoch = self.derive_epoch;
        self.preview_error = None;
        let network = self.network.clone();
        let input = match material {
            Material::Public { key, .. } => {
                self.deriving = false;
                if !is_test_network_key(&key.xpub) {
                    self.preview_error =
                        Some("This is a mainnet key; it can't be used on a test network.".into());
                }
                self.preview = Some(key);
                cx.notify();
                return;
            }
            Material::Derivable(secret) => DeriveInput {
                network,
                secret,
                derivation_path: self.current_path(cx),
                origin_fingerprint: None,
                origin_path: None,
            },
            Material::Account { secret, key } => DeriveInput {
                network,
                secret,
                derivation_path: key.derivation_path.clone(),
                origin_fingerprint: Some(key.fingerprint.clone()),
                origin_path: Some(key.derivation_path.clone()),
            },
        };
        self.deriving = true;
        let task = api::derive_key(cx, input);
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if this.derive_epoch != epoch {
                    return;
                }
                this.deriving = false;
                match res {
                    Ok(d) => {
                        let mut k = d.slot_key();
                        k.fingerprint = k.fingerprint.to_lowercase();
                        k.derivation_path = normalize_path(&k.derivation_path);
                        this.preview = Some(k);
                    }
                    Err(e) => {
                        this.preview = None;
                        this.preview_error = Some(friendly(&e).into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn can_save(&self, cx: &App) -> bool {
        !self.saving
            && !self.deriving
            && self.preview.is_some()
            && self.preview_error.is_none()
            && !self.label.read(cx).text().trim().is_empty()
    }

    /// Save to the library. Hot keys need the keychain unlocked; when it
    /// isn't, ask for the password first (needs `window` to build the
    /// dialog) and continue from [`UnlockEvent::Unlocked`].
    fn save(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        if self.step != AddKeyStep::Details || self.saving {
            return;
        }
        if self.label.read(cx).text().trim().is_empty() {
            self.error = Some("Give this key a name so you can recognise it later.".into());
            cx.notify();
            return;
        }
        if !self.can_save(cx) {
            return;
        }
        let Some(key) = self.preview.clone() else {
            return;
        };
        if let Some(existing) = keychain(cx)
            .read(cx)
            .find_match(Some(&key.fingerprint), &key.xpub)
        {
            self.error = Some(
                format!(
                    "This key is already in your library as “{}”.",
                    existing.label
                )
                .into(),
            );
            cx.notify();
            return;
        }
        let is_secret = self.material.as_ref().is_some_and(|m| m.secret().is_some());
        if is_secret && !keychain(cx).read(cx).is_unlocked() {
            let Some(window) = window else { return };
            let unlock = cx.new(|cx| KeychainUnlock::new("to store this key", window, cx));
            self._subs
                .push(cx.subscribe(&unlock, |this, _, ev: &UnlockEvent, cx| {
                    this.unlock = None;
                    if matches!(ev, UnlockEvent::Unlocked) {
                        this.commit(cx);
                    }
                    cx.notify();
                }));
            self.unlock = Some(unlock);
            cx.notify();
            return;
        }
        self.commit(cx);
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        let (Some(key), Some(material), Some(source)) =
            (self.preview.clone(), self.material.clone(), self.source)
        else {
            return;
        };
        let device = match &material {
            Material::Public { device, .. } => device.clone(),
            _ => None,
        };
        let new = NewKey {
            label: self.label.read(cx).text().trim().to_string(),
            network: self.network.clone(),
            source: source.id().to_string(),
            fingerprint: key.fingerprint.clone(),
            derivation_path: normalize_path(&key.derivation_path),
            xpub: key.xpub.clone(),
            device,
        };
        self.error = None;
        match material.secret() {
            None => match keychain(cx).update(cx, |k, cx| k.add_public(new, cx)) {
                Ok(id) => cx.emit(AddKeyEvent::Added(id)),
                Err(e) => self.error = Some(e.into()),
            },
            Some(secret) => {
                let core = match to_core_secret(secret) {
                    Ok(s) => s,
                    Err(e) => {
                        self.error = Some(e.into());
                        cx.notify();
                        return;
                    }
                };
                self.saving = true;
                let task = keychain(cx).update(cx, |k, cx| k.add_hot(new, core, cx));
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |this, cx| {
                        this.saving = false;
                        match res {
                            Ok(id) => cx.emit(AddKeyEvent::Added(id)),
                            Err(e) => this.error = Some(e.into()),
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
        }
        cx.notify();
    }

    // ── host-rendered pieces ──

    /// Back / Cancel / Continue bar for the current step.
    pub fn footer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut left: Vec<AnyElement> = Vec::new();
        let mut right: Vec<AnyElement> = Vec::new();
        if self.step != AddKeyStep::Source {
            left.push(
                button("akf-back", "Back")
                    .outline()
                    .icon("arrow-left")
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx)))
                    .into_any_element(),
            );
        }
        right.push(
            button("akf-cancel", "Cancel")
                .ghost()
                .on_click(cx.listener(|_, _, _, cx| cx.emit(AddKeyEvent::Cancelled)))
                .into_any_element(),
        );
        match self.step {
            AddKeyStep::Source => {}
            AddKeyStep::Key => {
                let continue_btn = |label: &'static str, enabled: bool| {
                    button("akf-continue", label)
                        .icon_right("arrow-right")
                        .loading(self.busy)
                        .disabled(!enabled)
                        .on_click(cx.listener(|this, _, _, cx| this.submit_key(cx)))
                        .into_any_element()
                };
                match self.source {
                    Some(KeySource::Generate) if self.backup.is_none() => right.push(
                        button("akf-generate", "Generate key")
                            .icon("sparkles")
                            .loading(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| this.generate(window, cx)))
                            .into_any_element(),
                    ),
                    Some(KeySource::Seed) => right.push(continue_btn(
                        "Continue",
                        self.mnemonic.read(cx).is_valid(cx),
                    )),
                    Some(KeySource::Xprv) => right.push(continue_btn(
                        "Continue",
                        matches!(
                            classify_pasted_key(self.xprv.read(cx).text()),
                            PastedKey::Private(_)
                        ),
                    )),
                    Some(KeySource::KeyFile) => right.push(continue_btn(
                        "Decrypt and continue",
                        self.key_file.is_some()
                            && !self.key_file_password.read(cx).text().is_empty(),
                    )),
                    Some(KeySource::Xpub) => right.push(continue_btn(
                        "Continue",
                        matches!(
                            classify_pasted_key(self.xpub.read(cx).text()),
                            PastedKey::Public(_)
                        ),
                    )),
                    _ => {}
                }
            }
            AddKeyStep::Details => {
                let enabled = self.can_save(cx);
                right.push(
                    button("akf-save", "Add to library")
                        .icon("check")
                        .loading(self.saving)
                        .disabled(!enabled)
                        .on_click(cx.listener(|this, _, window, cx| this.save(Some(window), cx)))
                        .into_any_element(),
                );
            }
        }
        footer_bar(left, right).into_any_element()
    }

    /// Keychain password dialog, as a page-level modal (hosted mode).
    pub fn overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let unlock = self.unlock.clone()?;
        Some(
            modal("akf-unlock", 420.)
                .on_dismiss(cx.listener(|this, _, _, cx| {
                    this.unlock = None;
                    cx.notify();
                }))
                .child(unlock)
                .into_any_element(),
        )
    }

    // ── render ──

    fn render_source_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let source_card = |s: KeySource, cx: &mut Context<Self>| {
            div()
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .flex()
                .child(
                    choice_card(SharedString::from(format!("src-{}", s.id())), false)
                        .w_full()
                        .clickable(
                            Probe::new("choice", s.title()),
                            cx.listener(move |this, _, window, cx| this.choose(s, window, cx)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_start()
                                .gap_3()
                                .child(icon_tile(
                                    s.icon(),
                                    36.,
                                    Some(if s.is_public() {
                                        t.muted_foreground
                                    } else {
                                        t.primary
                                    }),
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_size(px(13.))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(t.foreground)
                                                        .child(s.title()),
                                                )
                                                .when(s.is_public(), |d| {
                                                    d.child(badge("Public key", Tone::Secondary))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .line_height(px(17.))
                                                .text_color(t.muted_foreground)
                                                .child(s.description()),
                                        ),
                                ),
                        ),
                )
                .into_any_element()
        };
        let group =
            |title: &'static str, desc: &'static str, cards: Vec<AnyElement>, cols: usize| {
                let filler = cols.saturating_sub(cards.len());
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child(desc),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .children(cards)
                            .children((0..filler).map(|_| div().flex_1().flex_basis(px(0.)))),
                    )
            };
        let create: Vec<_> = KeySource::CREATE
            .iter()
            .map(|s| source_card(*s, cx))
            .collect();
        let private: Vec<_> = KeySource::IMPORT_PRIVATE
            .iter()
            .map(|s| source_card(*s, cx))
            .collect();
        let public: Vec<_> = KeySource::IMPORT_PUBLIC
            .iter()
            .map(|s| source_card(*s, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(group(
                "Create new",
                "Start a key from scratch on this computer.",
                create,
                3,
            ))
            .child(group(
                "Import a private key",
                "Bring in a key you already have. It's stored encrypted under your keychain password.",
                private,
                3,
            ))
            .child(group(
                "Add a public key (watch-only)",
                "Only the public key is stored here. The private key never leaves the device or its owner, and signing happens there.",
                public,
                3,
            ))
            .into_any_element()
    }

    fn source_heading(&self, s: KeySource) -> impl IntoElement {
        let t = theme();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(icon_tile(
                s.icon(),
                36.,
                Some(if s.is_public() {
                    t.muted_foreground
                } else {
                    t.primary
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
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground)
                                    .child(s.title()),
                            )
                            .when(s.is_public(), |d| {
                                d.child(badge("Watch-only", Tone::Secondary))
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .text_color(t.muted_foreground)
                            .child(s.description()),
                    ),
            )
    }

    fn render_generate(&self) -> AnyElement {
        let t = theme();
        if let Some(b) = &self.backup {
            return b.clone().into_any_element();
        }
        let step = |n: &'static str, title: &'static str, body: &'static str| {
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_3()
                .child(
                    div()
                        .size(px(22.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(alpha(t.primary, 0.5))
                        .text_size(px(12.))
                        .text_color(t.brand_ink)
                        .child(n),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .text_color(t.muted_foreground)
                                .child(body),
                        ),
                )
        };
        card()
            .p_5()
            .gap_4()
            .child(step(
                "1",
                "Generate",
                "24 random words are created on this computer. Nothing is sent anywhere.",
            ))
            .child(step(
                "2",
                "Write the words down",
                "On paper, in order. They're the only way to restore this key.",
            ))
            .child(step(
                "3",
                "Confirm three words",
                "A short quiz makes sure your copy is right before the key is saved.",
            ))
            .child(step(
                "4",
                "Name and save",
                "The key is stored encrypted in your library, ready to use in any wallet.",
            ))
            .into_any_element()
    }

    fn render_seed(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.mnemonic.clone())
            .child(if self.show_passphrase {
                field(
                    "BIP39 passphrase",
                    self.bip39_passphrase.clone(),
                    Some("Only if this phrase was set up with one. A different passphrase gives a completely different key.".into()),
                )
                .into_any_element()
            } else {
                div()
                    .id("show-bip39")
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(t.foreground))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_passphrase = true;
                        this.bip39_passphrase.read(cx).focus(window);
                        cx.notify();
                    }))
                    .child("+ This phrase uses a BIP39 passphrase")
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_xprv(&self, cx: &mut Context<Self>) -> AnyElement {
        let parsed = classify_pasted_key(self.xprv.read(cx).text());
        let hint: SharedString = match &parsed {
            PastedKey::Public(_) => "That's a public key. To add it watch-only, go back and choose “Extended public key”.".into(),
            PastedKey::Invalid(r) if !r.is_empty() => r.clone().into(),
            _ => "A master key (depth 0) is derived at the path you pick in the next step. An account-level key needs its [fingerprint/path] origin.".into(),
        };
        field("Extended private key", self.xprv.clone(), Some(hint)).into_any_element()
    }

    fn render_key_file(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(field(
                "Key file",
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        button(
                            "choose-key-file",
                            if self.key_file.is_some() {
                                "Choose another file"
                            } else {
                                "Choose .sigvault-key file"
                            },
                        )
                        .outline()
                        .small()
                        .icon("file-key")
                        .on_click(cx.listener(|this, _, _, cx| this.pick_key_file(cx))),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(if self.key_file.is_some() {
                                t.foreground
                            } else {
                                t.muted_foreground
                            })
                            .child(
                                self.key_file
                                    .as_ref()
                                    .map(|f| f.name.clone())
                                    .unwrap_or_else(|| "No file chosen".into()),
                            ),
                    ),
                None,
            ))
            .when(self.key_file.is_some(), |d| {
                d.child(field("File password", self.key_file_password.clone(), None))
            })
            .into_any_element()
    }

    fn render_hardware(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(t.muted_foreground)
                    .child("Reads the account key at")
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_color(t.foreground)
                            .child(self.current_path(cx)),
                    )
                    .child(
                        div()
                            .id("hw-advanced")
                            .text_color(t.brand_ink)
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_advanced = !this.show_advanced;
                                cx.notify();
                            }))
                            .child(if self.show_advanced { "Hide" } else { "Change" }),
                    ),
            )
            .when(self.show_advanced, |d| d.child(self.render_path_editor(cx)))
            .child(self.devices.clone())
            .into_any_element()
    }

    fn render_xpub(&self, cx: &mut Context<Self>) -> AnyElement {
        let parsed = classify_pasted_key(self.xpub.read(cx).text());
        let hint: SharedString = match &parsed {
            PastedKey::Public(k) => format!(
                "Public key · fingerprint {} · {}",
                k.fingerprint,
                k.derivation_path
            )
            .into(),
            PastedKey::Private(_) => "That's a private key. Go back and choose “Extended private key” to import it.".into(),
            PastedKey::Invalid(r) if !r.is_empty() => r.clone().into(),
            _ => "Ask the key's owner to export it from their wallet (Sparrow, Liana, a hardware wallet app…) with its origin.".into(),
        };
        field("Extended public key", self.xpub.clone(), Some(hint)).into_any_element()
    }

    fn render_key_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(source) = self.source else {
            return div().into_any_element();
        };
        let body = match source {
            KeySource::Generate => self.render_generate(),
            KeySource::Seed => self.render_seed(cx),
            KeySource::Xprv => self.render_xprv(cx),
            KeySource::KeyFile => self.render_key_file(cx),
            KeySource::Hardware => self.render_hardware(cx),
            KeySource::Xpub => self.render_xpub(cx),
        };
        div()
            .max_w(FORM_WIDTH)
            .flex()
            .flex_col()
            .gap_5()
            .child(self.source_heading(source))
            .child(body)
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .into_any_element()
    }

    fn render_path_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = self.path.read(cx).text().to_string();
        let preset = PathPreset::detect(&path, &self.network);
        let entity = cx.entity().downgrade();
        let network = self.network.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(segmented(
                "path-preset",
                match preset {
                    PathPreset::Multisig => "multisig",
                    PathPreset::Singlesig => "singlesig",
                    PathPreset::Custom => "custom",
                },
                vec![
                    seg("multisig", "Multisig · BIP48"),
                    seg("singlesig", "Single-sig · BIP84"),
                    seg("custom", "Custom"),
                ],
                false,
                move |v: SharedString, window, cx| {
                    let p = match v.as_ref() {
                        "multisig" => PathPreset::Multisig.path(&network),
                        "singlesig" => PathPreset::Singlesig.path(&network),
                        _ => None,
                    };
                    entity
                        .update(cx, |this, cx| {
                            match p {
                                Some(p) => this.path.update(cx, |i, cx| i.set_text(p, cx)),
                                None => this.path.read(cx).focus(window),
                            }
                            cx.notify();
                        })
                        .ok();
                },
            ))
            .child(self.path.clone())
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(theme().ink_3)
                    .child("Multisig and timelocked wallets use BIP48 (m/48'/1'/0'/2'). Change the account number to get another independent key from the same seed."),
            )
            .into_any_element()
    }

    fn render_details_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme();
        let Some(source) = self.source else {
            return div().into_any_element();
        };
        let material = self.material.clone();
        let derivable = matches!(material, Some(Material::Derivable(_)));
        let is_secret = material.as_ref().is_some_and(|m| m.secret().is_some());

        let custody = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(icon_tile(
                source.icon(),
                36.,
                Some(if is_secret {
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
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(source.title()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(if is_secret {
                                "The private key will be stored encrypted on this computer."
                            } else if source == KeySource::Hardware {
                                "Watch-only: the private key stays on the hardware wallet."
                            } else {
                                "Watch-only: signing happens wherever the private key lives."
                            }),
                    ),
            )
            .child(if is_secret {
                badge("Private key", Tone::Success)
            } else {
                badge("Watch-only", Tone::Secondary)
            });

        let network_control = if self.network_locked {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(network_badge(&self.network))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Matches the wallet you're creating."),
                )
                .into_any_element()
        } else {
            let entity = cx.entity().downgrade();
            segmented(
                "akf-network",
                &self.network,
                NETWORKS
                    .iter()
                    .map(|(id, label)| seg(*id, *label))
                    .collect(),
                false,
                move |v: SharedString, _, cx| {
                    entity
                        .update(cx, |this, cx| this.set_network(v.to_string(), true, cx))
                        .ok();
                },
            )
            .into_any_element()
        };

        let preview_body = if let Some(err) = &self.preview_error {
            alert(Tone::Destructive, err.clone()).into_any_element()
        } else if let Some(k) = self.preview.clone().filter(|_| !self.deriving) {
            let row = |k_: &'static str, v: AnyElement| {
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_4()
                    .px_4()
                    .py(px(10.))
                    .child(
                        div()
                            .w(px(140.))
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(k_),
                    )
                    .child(div().flex_1().min_w_0().child(v))
            };
            let mono_ = |s: String| {
                div()
                    .font_family(FONT_MONO)
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(t.foreground)
                    .child(s)
                    .into_any_element()
            };
            card()
                .child(row("Fingerprint", mono_(k.fingerprint.clone())))
                .child(divider())
                .child(row("Derivation path", mono_(k.derivation_path.clone())))
                .child(divider())
                .child(row("Extended public key", mono_(k.xpub.clone())))
                .child(divider())
                .child(row("Key expression", mono_(key_expr(&k))))
                .into_any_element()
        } else {
            card()
                .p_4()
                .flex_row()
                .items_center()
                .gap_2()
                .child(spinner(px(14.)))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Deriving public key…"),
                )
                .into_any_element()
        };

        let advanced = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("akf-advanced")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(t.foreground))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_advanced = !this.show_advanced;
                        cx.notify();
                    }))
                    .child(
                        icon(if self.show_advanced {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(px(14.)),
                    )
                    .child("Advanced: derivation path"),
            )
            .when(self.show_advanced, |d| d.child(self.render_path_editor(cx)));

        div()
            .max_w(FORM_WIDTH)
            .flex()
            .flex_col()
            .gap_5()
            .child(custody)
            .child(field(
                "Name",
                self.label.clone(),
                Some("Shown when you pick keys for a wallet. Only stored on this computer.".into()),
            ))
            .child(field("Network", network_control, None))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Public key"),
                    )
                    .child(preview_body)
                    .when(!derivable, |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(t.ink_3)
                                .child(match source {
                                    KeySource::Hardware => "The derivation path was chosen before reading the device. Go back to read a different account.",
                                    _ => "The derivation path comes from the key's origin and can't be changed.",
                                }),
                        )
                    }),
            )
            .when(derivable, |d| d.child(advanced))
            .children(self.error.clone().map(|e| alert(Tone::Destructive, e)))
            .when(!self.hosted, |d| {
                d.children(self.unlock.clone().map(|u| {
                    card().p_5().border_color(alpha(t.primary, 0.5)).child(u)
                }))
            })
            .into_any_element()
    }
}

/// Strip an optional `[fp/path]` origin from an xprv secret.
fn strip_origin(secret: &HotSecret) -> String {
    match secret {
        HotSecret::Xprv { xprv } => xprv
            .rsplit(']')
            .next()
            .unwrap_or(xprv)
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect(),
        HotSecret::Mnemonic { .. } => String::new(),
    }
}

/// Make the core's error strings read like UI copy.
fn friendly(e: &str) -> String {
    let e = e.trim();
    if (e.contains("Network") && e.contains("xprv"))
        || (e.contains("network") && e.contains("does not match"))
    {
        return "This key belongs to a different network.".into();
    }
    if e.to_lowercase().contains("decrypt") || e.to_lowercase().contains("password") {
        return "Couldn't open the file. Check the password.".into();
    }
    let mut chars = e.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Something went wrong".into(),
    }
}

impl Render for AddKeyFlow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_label) {
            self.label.read(cx).focus(window);
        }
        let t = theme();
        let body = match self.step {
            AddKeyStep::Source => self.render_source_step(cx),
            AddKeyStep::Key => self.render_key_step(cx),
            AddKeyStep::Details => self.render_details_step(cx),
        };
        let hosted = self.hosted;
        let footer = (!hosted).then(|| self.footer(cx));
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(body)
            .children(footer.map(|f| {
                div()
                    .pt_4()
                    .border_t_1()
                    .border_color(alpha(t.border, 0.7))
                    .child(f)
            }))
    }
}
