//! `/local/keys/<id>` — one library key: public key (fingerprint, path,
//! xpub, key expression, QR), the wallets using it, and rename / export /
//! delete.

use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, SharedString, Subscription,
    Window,
};

use super::list::{load_usage, wallets_using, UsageMap};
use super::sources::{copy_box, custody_text, from_core_secret, network_badge};
use super::unlock::{KeychainUnlock, UnlockEvent};
use crate::keychain::{keychain, source_label, KeyRecord, KeychainEvent};
use crate::pages::local::setup::api::{self, SlotKey};
use crate::pages::local::setup::files::{file_safe, save_text_file};
use crate::router::Page;
use crate::shell::{navigate, toast};
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, badge, button, card, divider, field, format_timestamp, icon, modal, page_frame, section,
    InputEvent, QrCode, TextInput, Tone,
};

enum Dialog {
    Rename,
    Unlock(Entity<KeychainUnlock>),
    Export,
    Delete,
}

pub struct KeyDetailPage {
    key_id: String,
    usage: Option<UsageMap>,
    dialog: Option<Dialog>,
    busy: bool,
    error: Option<SharedString>,
    rename_input: Entity<TextInput>,
    export_pw: Entity<TextInput>,
    export_confirm: Entity<TextInput>,
    delete_input: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl KeyDetailPage {
    pub fn new(key_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rename_input = cx.new(|cx| TextInput::new(window, cx).max_len(60));
        let export_pw = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("At least 8 characters")
        });
        let export_confirm = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Repeat password")
        });
        let delete_input = cx.new(|cx| TextInput::new(window, cx));
        let mut subs = vec![
            cx.subscribe(&keychain(cx), |_, _, _: &KeychainEvent, cx| cx.notify()),
            cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| {
                if matches!(ev, StoreEvent::LocalDataChanged) {
                    this.refresh_usage(cx);
                }
            }),
            cx.subscribe(&rename_input, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Submit => this.submit_rename(cx),
                InputEvent::Changed => cx.notify(),
            }),
            cx.subscribe(&delete_input, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Submit => this.submit_delete(cx),
                InputEvent::Changed => cx.notify(),
            }),
        ];
        for i in [&export_pw, &export_confirm] {
            subs.push(cx.subscribe(i, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Submit => this.submit_export(cx),
                InputEvent::Changed => {
                    this.error = None;
                    cx.notify();
                }
            }));
        }
        let mut this = Self {
            key_id,
            usage: None,
            dialog: None,
            busy: false,
            error: None,
            rename_input,
            export_pw,
            export_confirm,
            delete_input,
            _subs: subs,
        };
        this.refresh_usage(cx);
        this
    }

    fn key(&self, cx: &gpui::App) -> Option<KeyRecord> {
        keychain(cx).read(cx).get(&self.key_id).cloned()
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

    fn close(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        self.busy = false;
        self.error = None;
        cx.notify();
    }

    // ── rename ──

    fn open_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(k) = self.key(cx) else { return };
        self.rename_input
            .update(cx, |i, cx| i.set_text(k.label, cx));
        self.rename_input.read(cx).focus(window);
        self.error = None;
        self.dialog = Some(Dialog::Rename);
        cx.notify();
    }

    fn submit_rename(&mut self, cx: &mut Context<Self>) {
        let label = self.rename_input.read(cx).text().trim().to_string();
        if label.is_empty() {
            return;
        }
        let id = self.key_id.clone();
        keychain(cx).update(cx, |k, cx| k.rename(&id, label, cx));
        self.close(cx);
    }

    // ── export ──

    fn start_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        if keychain(cx).read(cx).is_unlocked() {
            self.open_export(window, cx);
            return;
        }
        let unlock = cx.new(|cx| KeychainUnlock::new("to export this key", window, cx));
        self._subs.push(cx.subscribe_in(
            &unlock,
            window,
            |this, _, ev: &UnlockEvent, window, cx| match ev {
                UnlockEvent::Unlocked => this.open_export(window, cx),
                UnlockEvent::Cancelled => this.close(cx),
            },
        ));
        self.dialog = Some(Dialog::Unlock(unlock));
        cx.notify();
    }

    fn open_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.export_pw.update(cx, |i, cx| i.clear(cx));
        self.export_confirm.update(cx, |i, cx| i.clear(cx));
        self.export_pw.read(cx).focus(window);
        self.dialog = Some(Dialog::Export);
        cx.notify();
    }

    fn export_ready(&self, cx: &gpui::App) -> bool {
        let pw = self.export_pw.read(cx).text();
        pw.chars().count() >= 8 && pw == self.export_confirm.read(cx).text()
    }

    fn submit_export(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.export_ready(cx) {
            return;
        }
        let Some(k) = self.key(cx) else { return };
        self.busy = true;
        self.error = None;
        cx.notify();
        let password = self.export_pw.read(cx).text().to_string();
        let secret = keychain(cx).update(cx, |kc, cx| kc.secret(&k.id, cx));
        let slot_key = SlotKey {
            fingerprint: k.fingerprint.clone(),
            derivation_path: k.derivation_path.clone(),
            xpub: k.xpub.clone(),
        };
        let name = format!("{}-{}.sigvault-key", file_safe(&k.label), k.fingerprint);
        cx.spawn(async move |this, cx| {
            let res: Result<Option<String>, String> = async {
                let secret = from_core_secret(&secret.await?)?;
                let encrypt = cx
                    .update(|cx| api::encrypt_key_backup(cx, secret, password, slot_key))
                    .map_err(|e| e.to_string())?;
                let contents = encrypt.await?;
                save_text_file(cx, contents, name).await
            }
            .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match res {
                    Ok(Some(path)) => {
                        this.close(cx);
                        toast(
                            cx,
                            format!("Saved {}", path.rsplit('/').next().unwrap_or(&path)),
                        );
                    }
                    Ok(None) => {}
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── delete ──

    fn open_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_input.update(cx, |i, cx| i.clear(cx));
        self.delete_input.read(cx).focus(window);
        self.dialog = Some(Dialog::Delete);
        cx.notify();
    }

    fn delete_ready(&self, cx: &gpui::App) -> bool {
        self.key(cx)
            .is_some_and(|k| self.delete_input.read(cx).text().trim() == k.label.trim())
    }

    fn submit_delete(&mut self, cx: &mut Context<Self>) {
        if !self.delete_ready(cx) {
            return;
        }
        let id = self.key_id.clone();
        keychain(cx).update(cx, |k, cx| k.delete(&id, cx));
        self.dialog = None;
        toast(cx, "Key removed from library");
        navigate(Page::LocalKeys, cx);
    }

    // ── render ──

    fn render_public(&self, k: &KeyRecord) -> impl IntoElement {
        let t = theme();
        let row = |label: &'static str, value: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .py(px(9.))
                .child(
                    div()
                        .w(px(120.))
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(label),
                )
                .child(div().flex_1().min_w_0().flex().flex_row().child(value))
        };
        let mono = |s: String| {
            div()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .text_color(t.foreground)
                .child(s)
                .into_any_element()
        };
        let text = |s: String| div().text_size(px(13.)).child(s).into_any_element();
        let source = match &k.device {
            Some(d) if k.source == "hardware" => format!("{} · {d}", source_label(&k.source)),
            _ => source_label(&k.source).to_string(),
        };
        card()
            .flex_row()
            .items_start()
            .gap_6()
            .p_5()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(row("Fingerprint", mono(k.fingerprint.clone())))
                    .child(divider())
                    .child(row("Derivation path", mono(k.derivation_path.clone())))
                    .child(divider())
                    .child(row("Network", network_badge(&k.network).into_any_element()))
                    .child(divider())
                    .child(row("Source", text(source)))
                    .child(divider())
                    .child(row("Added", text(format_timestamp(k.created_at))))
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child("Extended public key"),
                            )
                            .child(copy_box("copy-xpub", k.xpub.clone())),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child("Key expression (for descriptors and co-signers)"),
                            )
                            .child(copy_box("copy-expr", k.key_expr())),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(QrCode::new(k.key_expr(), px(184.)))
                    .child(
                        div()
                            .w(px(184.))
                            .text_center()
                            .text_size(px(11.))
                            .line_height(px(15.))
                            .text_color(t.muted_foreground)
                            .child("Scan to share this public key with a co-signer"),
                    ),
            )
    }

    fn render_used_by(&self, k: &KeyRecord) -> AnyElement {
        let t = theme();
        let Some(usage) = &self.usage else {
            return card()
                .p_4()
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child("Loading wallets…"),
                )
                .into_any_element();
        };
        let wallets = wallets_using(usage, k);
        if wallets.is_empty() {
            return card()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .px_4()
                .py_3()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t.muted_foreground)
                        .child("No wallet uses this key yet. Pick it when you create a wallet."),
                )
                .child(
                    button("new-wallet", "Create wallet")
                        .outline()
                        .small()
                        .icon("plus")
                        .on_click(|_, _, cx| navigate(Page::LocalWalletNew, cx)),
                )
                .into_any_element();
        }
        let mut list = card().overflow_hidden();
        for (i, w) in wallets.iter().enumerate() {
            if i > 0 {
                list = list.child(divider());
            }
            let page = Page::LocalWallet(w.id.as_str().to_string());
            let kind = match w.policy_type.as_str() {
                "singlesig" | "singlesig_hot" | "singlesig_hardware" => "Single key",
                "multisig" => "Multisig",
                "timelocked" => "Timelocked policy",
                "descriptor" => "Descriptor",
                "watch_only" => "Watch-only",
                other => other,
            }
            .to_string();
            list = list.child(
                div()
                    .id(("used-by", i))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(px(52.))
                    .px_4()
                    .cursor_pointer()
                    .hover(move |s| s.bg(alpha(t.muted, 0.5)))
                    .on_click(move |_, _, cx| navigate(page.clone(), cx))
                    .child(icon("wallet").size(px(15.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(w.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted_foreground)
                                    .child({
                                        let n = w.fingerprints.len().max(1);
                                        format!(
                                            "{kind} · {n} {}",
                                            if n == 1 { "key" } else { "keys" }
                                        )
                                    }),
                            ),
                    )
                    .child(network_badge(&w.network))
                    .child(icon("chevron-right").size(px(14.))),
            );
        }
        list.into_any_element()
    }

    fn render_manage(&self, k: &KeyRecord, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let row = |title: &'static str, body: String, action: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .px_4()
                .py_3()
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
                .child(div().flex_none().child(action))
        };
        let mut c = card().child(row(
            "Rename",
            "Change the name shown in the library and the wallet wizard.".into(),
            button("rename", "Rename")
                .outline()
                .small()
                .icon("pen")
                .on_click(cx.listener(|this, _, window, cx| this.open_rename(window, cx)))
                .into_any_element(),
        ));
        if k.is_hot() {
            c = c.child(divider()).child(row(
                "Export encrypted backup",
                "Save the private key as a password-protected .sigvault-key file you can import on another computer.".into(),
                button("export", "Export")
                    .outline()
                    .small()
                    .icon("download")
                    .on_click(cx.listener(|this, _, window, cx| this.start_export(window, cx)))
                    .into_any_element(),
            ));
        }
        c.child(divider()).child(row(
            "Remove from library",
            if k.is_hot() {
                "Deletes the stored private key from this computer. Wallets already created with it keep working."
            } else {
                "Forgets this public key. Wallets already created with it keep working."
            }
            .into(),
            button("delete", "Remove")
                .outline()
                .small()
                .icon("trash")
                .on_click(cx.listener(|this, _, window, cx| this.open_delete(window, cx)))
                .into_any_element(),
        ))
    }

    fn render_dialog(&self, k: &KeyRecord, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = theme();
        let dialog = self.dialog.as_ref()?;
        let title = |s: &'static str| {
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(s)
        };
        let body = |s: String| {
            div()
                .text_size(px(13.))
                .line_height(px(19.))
                .text_color(t.muted_foreground)
                .child(s)
        };
        let actions = |cancel: AnyElement, primary: AnyElement| {
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap_2()
                .child(cancel)
                .child(primary)
        };
        let cancel = || {
            button("dlg-cancel", "Cancel")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
                .into_any_element()
        };
        let content: Vec<AnyElement> = match dialog {
            Dialog::Unlock(u) => vec![u.clone().into_any_element()],
            Dialog::Rename => vec![
                title("Rename key").into_any_element(),
                field("Name", self.rename_input.clone(), None).into_any_element(),
                actions(
                    cancel(),
                    button("rename-save", "Save")
                        .disabled(self.rename_input.read(cx).text().trim().is_empty())
                        .on_click(cx.listener(|this, _, _, cx| this.submit_rename(cx)))
                        .into_any_element(),
                )
                .into_any_element(),
            ],
            Dialog::Export => {
                let pw = self.export_pw.read(cx).text().to_string();
                let confirm = self.export_confirm.read(cx).text().to_string();
                let mismatch = !confirm.is_empty() && pw != confirm;
                let mut v = vec![
                    title("Export encrypted key backup").into_any_element(),
                    body(format!(
                        "Choose a password for the backup of “{}”. You'll need it to import the file again. It's separate from your keychain password.",
                        k.label
                    ))
                    .into_any_element(),
                    field("Backup password", self.export_pw.clone(), None).into_any_element(),
                    field(
                        "Confirm password",
                        self.export_confirm.clone(),
                        mismatch.then(|| SharedString::from("Passwords don't match")),
                    )
                    .into_any_element(),
                ];
                if let Some(e) = &self.error {
                    v.push(alert(Tone::Destructive, e.clone()).into_any_element());
                }
                v.push(
                    actions(
                        cancel(),
                        button("export-save", "Save backup file")
                            .icon("download")
                            .loading(self.busy)
                            .disabled(!self.export_ready(cx))
                            .on_click(cx.listener(|this, _, _, cx| this.submit_export(cx)))
                            .into_any_element(),
                    )
                    .into_any_element(),
                );
                v
            }
            Dialog::Delete => vec![
                title("Remove key from library?").into_any_element(),
                body(if k.is_hot() {
                    "The encrypted private key is deleted from this computer's keychain. Wallets already created with this key keep their own copy and keep working. Make sure you still have the recovery phrase or a backup file if you'll need this key again.".into()
                } else {
                    "Only the saved public key is forgotten. Wallets already created with this key keep working.".into()
                })
                .into_any_element(),
                field(
                    format!("Type “{}” to confirm", k.label),
                    self.delete_input.clone(),
                    None,
                )
                .into_any_element(),
                actions(
                    cancel(),
                    button("delete-confirm", "Remove key")
                        .destructive()
                        .icon("trash")
                        .disabled(!self.delete_ready(cx))
                        .on_click(cx.listener(|this, _, _, cx| this.submit_delete(cx)))
                        .into_any_element(),
                )
                .into_any_element(),
            ],
        };
        let width = if matches!(dialog, Dialog::Delete | Dialog::Export) {
            460.
        } else {
            420.
        };
        Some(
            modal("key-dialog", width)
                .on_dismiss(cx.listener(|this, _, _, cx| this.close(cx)))
                .children(content)
                .into_any_element(),
        )
    }
}

impl Render for KeyDetailPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let Some(k) = self.key(cx) else {
            return div().size_full().child(
                page_frame("key-detail")
                    .crumb("Key library", Some(Page::LocalKeys))
                    .crumb("Not found", None)
                    .title("Key not found")
                    .child(
                        card()
                            .items_center()
                            .gap_3()
                            .py_10()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(t.muted_foreground)
                                    .child("This key is no longer in your library."),
                            )
                            .child(
                                button("back-to-keys", "Back to key library")
                                    .outline()
                                    .small()
                                    .on_click(|_, _, cx| navigate(Page::LocalKeys, cx)),
                            ),
                    ),
            );
        };
        let adornment = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(if k.is_hot() {
                badge("Private key on this computer", Tone::Success)
            } else {
                badge("Watch-only", Tone::Secondary)
            })
            .child(network_badge(&k.network));
        let subtitle = custody_text(&k);
        let dialog = self.render_dialog(&k, cx);

        div()
            .size_full()
            .relative()
            .child(
                page_frame("key-detail")
                    .crumb("Key library", Some(Page::LocalKeys))
                    .crumb(k.label.clone(), None)
                    .title(k.label.clone())
                    .title_adornment(adornment)
                    .subtitle(subtitle)
                    .child(
                        section("Public key")
                            .description("Safe to share. Co-signers need it to build a multisig wallet with you.")
                            .child(self.render_public(&k)),
                    )
                    .child(
                        section("Used by")
                            .description("Wallets on this computer that include this key (matched by fingerprint).")
                            .child(self.render_used_by(&k)),
                    )
                    .child(section("Manage").child(self.render_manage(&k, cx))),
            )
            .children(dialog)
    }
}
