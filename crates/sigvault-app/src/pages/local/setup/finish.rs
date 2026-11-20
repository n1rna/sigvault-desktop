//! The last two steps both wizards share: review (with the wallet
//! passphrase when the wallet stores private keys), then the done screen
//! that pushes a wallet backup file. For any wallet with more than one key
//! the descriptor is what makes the keys usable, so the backup gets top
//! billing.
//!
//! Both render only their body; the host page supplies the frame and the
//! footer buttons (Back / Create, Open wallet).

use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, EventEmitter, FontWeight, SharedString,
    Subscription, Window,
};

use super::api;
use super::files::{file_safe, save_text_file};
use super::slot_board::{custody_badge, meta_line, source_icon, BoardKey};
use crate::keychain::source_label;
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{
    alert, button, card, checkbox, divider, field, icon, section, InputEvent, TextInput, Tone,
    FORM_WIDTH,
};

/// One slot on the review screen.
#[derive(Clone, Debug)]
pub struct ReviewRow {
    pub slot_label: String,
    pub path_title: String,
    /// `None`: restore slot left watch-only (descriptor key only).
    pub key: Option<BoardKey>,
    /// Shown for watch-only rows.
    pub fingerprint: String,
    pub derivation_path: String,
}

#[derive(Clone, Debug, Default)]
pub struct ReviewData {
    pub name: String,
    pub network: String,
    pub summary: String,
    pub rows: Vec<ReviewRow>,
    /// Slots whose private key gets stored in the wallet.
    pub hot: usize,
    /// Paths the hot keys could spend on their own.
    pub exposed: Vec<String>,
    /// Restore only: the wallet will be created watch-only.
    pub watch_only: bool,
}

pub enum ReviewEvent {
    /// Enter pressed in a passphrase field while ready.
    Submit,
}

pub struct ReviewStep {
    data: ReviewData,
    submitting: bool,
    passphrase: Entity<TextInput>,
    confirm: Entity<TextInput>,
    accept_exposure: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ReviewEvent> for ReviewStep {}

impl ReviewStep {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let passphrase =
            cx.new(|cx| TextInput::new(window, cx).masked().placeholder("At least 8 characters"));
        let confirm =
            cx.new(|cx| TextInput::new(window, cx).masked().placeholder("Repeat passphrase"));
        let on_input = |this: &mut Self, _: Entity<TextInput>, ev: &InputEvent, cx: &mut Context<Self>| {
            match ev {
                InputEvent::Changed => cx.notify(),
                InputEvent::Submit => {
                    if this.ready(cx) && !this.submitting {
                        cx.emit(ReviewEvent::Submit);
                    }
                }
            }
        };
        let subs = vec![cx.subscribe(&passphrase, on_input), cx.subscribe(&confirm, on_input)];
        Self {
            data: ReviewData::default(),
            submitting: false,
            passphrase,
            confirm,
            accept_exposure: false,
            _subs: subs,
        }
    }

    /// Refresh the review data before showing the step.
    pub fn configure(&mut self, data: ReviewData, cx: &mut Context<Self>) {
        if data.exposed != self.data.exposed {
            self.accept_exposure = false;
        }
        self.data = data;
        cx.notify();
    }

    pub fn set_submitting(&mut self, submitting: bool, cx: &mut Context<Self>) {
        self.submitting = submitting;
        cx.notify();
    }

    pub fn passphrase(&self, cx: &gpui::App) -> String {
        self.passphrase.read(cx).text().to_string()
    }

    pub fn ready(&self, cx: &gpui::App) -> bool {
        let pass = self.passphrase(cx);
        let confirm = self.confirm.read(cx).text();
        let pass_ok = self.data.hot == 0 || (pass.chars().count() >= 8 && pass == confirm);
        let exposure_ok = self.data.exposed.is_empty() || self.accept_exposure;
        pass_ok && exposure_ok
    }

    fn summary_card(&self) -> impl IntoElement {
        let t = theme();
        let row = |k: &'static str, v: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .h(px(44.))
                .px_4()
                .child(div().w(px(150.)).flex_none().text_size(px(13.)).text_color(t.muted_foreground).child(k))
                .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(t.foreground).child(v))
        };
        card()
            .child(row("Name", self.data.name.clone().into_any_element()))
            .child(divider())
            .child(row("Network", network_label(&self.data.network).into_any_element()))
            .child(divider())
            .child(row("Policy", self.data.summary.clone().into_any_element()))
    }

    fn keys_card(&self) -> impl IntoElement {
        let t = theme();
        let n = self.data.rows.len();
        card().overflow_hidden().children(self.data.rows.iter().enumerate().flat_map(|(i, r)| {
            let body = match &r.key {
                Some(k) => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_w_0()
                    .child(icon(source_icon(&k.source)).size(px(15.)).text_color(t.foreground))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.foreground)
                            .child(k.label.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(t.muted_foreground)
                            .child(source_label(&k.source)),
                    )
                    .child(custody_badge(k.hot)),
                None => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(icon("eye").size(px(15.)))
                    .child(div().text_size(px(13.)).text_color(t.muted_foreground).child("Watch-only")),
            };
            let (fp, path) = match &r.key {
                Some(k) => (k.fingerprint.clone(), k.derivation_path.clone()),
                None => (r.fingerprint.clone(), r.derivation_path.clone()),
            };
            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .min_h(px(52.))
                .px_4()
                .py_2()
                .child(
                    div()
                        .w(px(150.))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.foreground).child(r.slot_label.clone()))
                        .child(div().text_size(px(12.)).text_color(t.muted_foreground).truncate().child(r.path_title.clone())),
                )
                .child(div().flex_1().min_w_0().child(body))
                .child(div().flex_none().child(meta_line(&fp, &path)));
            let mut out = vec![row.into_any_element()];
            if i + 1 < n {
                out.push(divider().into_any_element());
            }
            out
        }))
    }
}

pub fn network_label(n: &str) -> String {
    super::ui::SUPPORTED_NETWORKS
        .iter()
        .find(|(id, _, _)| *id == n)
        .map(|(_, l, _)| l.to_string())
        .unwrap_or_else(|| n.to_string())
}

impl Render for ReviewStep {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let hot = self.data.hot;
        let pass = self.passphrase(cx);
        let confirm = self.confirm.read(cx).text().to_string();
        let this = cx.entity().downgrade();

        let custody: AnyElement = if hot > 0 {
            section("Wallet passphrase")
                .description(format!(
                    "The wallet stores its own encrypted copy of {}, so it keeps working even if you later remove keys from the library. You'll type this passphrase to unlock the wallet and to sign.",
                    if hot == 1 { "the private key".to_string() } else { format!("the {hot} private keys") }
                ))
                .child(
                    div()
                        .max_w(FORM_WIDTH)
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(field("Passphrase", self.passphrase.clone(), None))
                        .child(field(
                            "Confirm passphrase",
                            self.confirm.clone(),
                            (!confirm.is_empty() && pass != confirm).then(|| SharedString::from("Passphrases don't match")),
                        )),
                )
                .into_any_element()
        } else if self.data.watch_only {
            alert(
                Tone::Secondary,
                "None of this wallet's keys are in your library, so it will be watch-only: you can see balances and addresses and build transactions for other key holders to sign.",
            )
            .into_any_element()
        } else {
            alert(
                Tone::Secondary,
                "No private keys are stored with this wallet, so it needs no passphrase. Signing happens on hardware wallets or by exporting PSBTs to the other key holders.",
            )
            .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(section("Summary").child(self.summary_card()))
            .child(
                section("Keys")
                    .description("Each slot and the library key that fills it.")
                    .child(self.keys_card()),
            )
            .child(custody)
            .when(!self.data.exposed.is_empty(), |d| {
                let titles = self.data.exposed.iter().map(|p| p.to_lowercase()).collect::<Vec<_>>().join(", ");
                d.child(
                    div()
                        .rounded_lg()
                        .border_1()
                        .border_color(alpha(t.warning, 0.4))
                        .bg(alpha(t.warning, 0.06))
                        .px_4()
                        .py_3()
                        .child(checkbox(
                            "accept-exposure",
                            self.accept_exposure,
                            format!("I understand that keys on this computer can spend without any other signer ({titles})."),
                            move |v, _, cx| {
                                this.update(cx, |this, cx| {
                                    this.accept_exposure = v;
                                    cx.notify();
                                })
                                .ok();
                            },
                        )),
                )
            })
    }
}

pub struct DoneStep {
    title: &'static str,
    wallet_id: String,
    wallet_name: String,
    multi_key: bool,
    saved: Option<String>,
    error: Option<SharedString>,
}

impl DoneStep {
    pub fn new(title: &'static str, wallet_id: String, wallet_name: String, multi_key: bool) -> Self {
        Self {
            title,
            wallet_id,
            wallet_name,
            multi_key,
            saved: None,
            error: None,
        }
    }

    fn save_backup(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        let export = api::export_wallet_backup(cx, self.wallet_id.clone());
        let name = format!("{}.wallet-backup.json", file_safe(&self.wallet_name));
        cx.spawn(async move |this, cx| {
            let res = match export.await {
                Ok(contents) => save_text_file(cx, contents, name).await,
                Err(e) => Err(e),
            };
            this.update(cx, |this, cx| {
                match res {
                    Ok(Some(p)) => this.saved = Some(p),
                    Ok(None) => {}
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for DoneStep {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let multi = self.multi_key;
        div()
            .max_w(FORM_WIDTH)
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .size(px(40.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(alpha(t.success, 0.12))
                            .child(icon("check").size(px(20.)).text_color(t.success)),
                    )
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
                                    .child(self.title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(19.))
                                    .text_color(t.muted_foreground)
                                    .child(format!(
                                        "“{}” is unlocked and syncing. Balance and history fill in as the sync completes.",
                                        self.wallet_name
                                    )),
                            ),
                    ),
            )
            .child(
                section(if multi { "Save the wallet backup now" } else { "Wallet backup" })
                    .description(if multi {
                        "Seed phrases alone can't restore a multi-key wallet. You also need the descriptor, which lists every key and the spending rules. This file holds it (no private keys), so give a copy to every key holder."
                    } else {
                        "The descriptor and key details, without private keys. Restoring from the seed works too; this file makes it faster and records the exact setup."
                    })
                    .child(
                        card()
                            .px_4()
                            .py_3()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .when(multi && self.saved.is_none(), |d| {
                                d.border_color(alpha(t.warning, 0.5)).bg(alpha(t.warning, 0.05))
                            })
                            .child(icon("file").size(px(16.)))
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
                                            .text_color(t.foreground)
                                            .child(format!("{}.wallet-backup.json", file_safe(&self.wallet_name))),
                                    )
                                    .child(match &self.saved {
                                        Some(p) => div()
                                            .truncate()
                                            .font_family(FONT_MONO)
                                            .text_size(px(12.))
                                            .text_color(t.success)
                                            .child(format!("Saved to {p}")),
                                        None => div()
                                            .text_size(px(12.))
                                            .text_color(t.muted_foreground)
                                            .child("Not saved yet"),
                                    }),
                            )
                            .child(
                                button(
                                    "save-wallet-backup",
                                    if self.saved.is_some() { "Save another copy" } else { "Save backup…" },
                                )
                                .outline()
                                .small()
                                .icon("download")
                                .on_click(cx.listener(|this, _, _, cx| this.save_backup(cx))),
                            ),
                    )
                    .children(self.error.clone().map(|e| alert(Tone::Destructive, e))),
            )
    }
}
