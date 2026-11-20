//! `setup/KeyBackup.tsx` — backup flow for a key generated on this
//! computer:
//!   1. show the words once,
//!   2. quiz three random positions so we know they were written down,
//!   3. offer an encrypted key file and (behind a warning) a plaintext copy.
//! The slot only counts as filled once step 2 passes.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use gpui::{div, prelude::*, px, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Window};

use super::api::{self, DerivedKey, HotSecret};
use super::files::{file_safe, save_text_file};
use super::ui::{
    alert_text, error_line, field_label, primary_button, secondary_button, setup_alert, AlertTone,
};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::{checkbox, InputEvent, TextInput};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Words,
    Quiz,
    Files,
}

pub enum KeyBackupEvent {
    /// Backup confirmed (the TSX passes the saved file list, which no
    /// caller reads).
    Done,
    Discard,
}

fn random_u64(i: usize) -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_usize(i);
    h.finish()
}

fn pick_positions(count: usize, total: usize) -> Vec<usize> {
    let mut all: Vec<usize> = (0..total).collect();
    let mut out = Vec::with_capacity(count);
    for i in 0..count.min(total) {
        let j = (random_u64(i) % all.len() as u64) as usize;
        out.push(all.remove(j));
    }
    out.sort_unstable();
    out
}

pub struct KeyBackup {
    words: Vec<String>,
    derived: DerivedKey,
    slot_label: String,
    base_name: String,
    stage: Stage,
    saved_files: Vec<String>,
    // words
    revealed: bool,
    // quiz
    positions: Vec<usize>,
    answers: Vec<Entity<TextInput>>,
    checked: bool,
    // files
    password: Entity<TextInput>,
    confirm: Entity<TextInput>,
    busy: bool,
    error: Option<SharedString>,
    plain_ack: bool,
    /// Label of the final "done" button.
    done_label: SharedString,
    _subs: Vec<Subscription>,
}

impl EventEmitter<KeyBackupEvent> for KeyBackup {}

impl KeyBackup {
    pub fn new(
        words: Vec<String>,
        derived: DerivedKey,
        slot_label: String,
        wallet_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let answers: Vec<_> = (0..3)
            .map(|_| cx.new(|cx| TextInput::new(window, cx).mono()))
            .collect();
        let password = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("File password (8+ characters)")
        });
        let confirm =
            cx.new(|cx| TextInput::new(window, cx).masked().placeholder("Confirm password"));
        let mut subs = Vec::new();
        for a in &answers {
            subs.push(cx.subscribe(a, |this, _, ev: &InputEvent, cx| match ev {
                InputEvent::Changed => {
                    this.checked = false;
                    cx.notify();
                }
                InputEvent::Submit => this.submit_quiz(cx),
            }));
        }
        for i in [&password, &confirm] {
            subs.push(cx.subscribe(i, |_, _, _: &InputEvent, cx| cx.notify()));
        }
        let base_name = format!(
            "{}-{}-{}",
            file_safe(&wallet_name),
            file_safe(&slot_label),
            derived.fingerprint
        );
        Self {
            positions: pick_positions(3, words.len()),
            words,
            derived,
            slot_label,
            base_name,
            stage: Stage::Words,
            saved_files: vec![],
            revealed: false,
            answers,
            checked: false,
            password,
            confirm,
            busy: false,
            error: None,
            plain_ack: false,
            done_label: "Use this key".into(),
            _subs: subs,
        }
    }

    /// Backup flow for a key generated into the key library, before any
    /// wallet exists: files are named after the fingerprint and the last
    /// button reads "Continue".
    pub fn for_library(
        words: Vec<String>,
        derived: DerivedKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let label = format!("key {}", derived.fingerprint);
        let mut this = Self::new(words, derived, label, String::new(), window, cx);
        this.base_name = format!("sigvault-key-{}", this.derived.fingerprint);
        this.done_label = "Continue".into();
        this
    }

    fn secret(&self) -> HotSecret {
        HotSecret::Mnemonic {
            mnemonic: self.words.join(" "),
            bip39_passphrase: String::new(),
        }
    }

    fn go_quiz(&mut self, cx: &mut Context<Self>) {
        // A fresh quiz each time, like the remounted TSX stage.
        self.positions = pick_positions(3, self.words.len());
        for a in &self.answers {
            a.update(cx, |a, cx| a.clear(cx));
        }
        self.checked = false;
        self.stage = Stage::Quiz;
        cx.notify();
    }

    fn correct(&self, cx: &gpui::App) -> Vec<bool> {
        self.positions
            .iter()
            .enumerate()
            .map(|(i, p)| self.answers[i].read(cx).text().trim().to_lowercase() == self.words[*p])
            .collect()
    }

    fn submit_quiz(&mut self, cx: &mut Context<Self>) {
        if self.answers.iter().any(|a| a.read(cx).text().trim().is_empty()) {
            return;
        }
        self.checked = true;
        if self.correct(cx).iter().all(|c| *c) {
            self.stage = Stage::Files;
        }
        cx.notify();
    }

    fn save_encrypted(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        cx.notify();
        let password = self.password.read(cx).text().to_string();
        let task = api::encrypt_key_backup(cx, self.secret(), password, self.derived.slot_key());
        let name = format!("{}.sigvault-key", self.base_name);
        cx.spawn(async move |this, cx| {
            let res = match task.await {
                Ok(contents) => save_text_file(cx, contents, name).await,
                Err(e) => Err(e),
            };
            this.update(cx, |this, cx| {
                match res {
                    Ok(Some(path)) => {
                        this.saved_files.push(path);
                        this.password.update(cx, |i, cx| i.clear(cx));
                        this.confirm.update(cx, |i, cx| i.clear(cx));
                    }
                    Ok(None) => {}
                    Err(e) => this.error = Some(e.into()),
                }
                this.busy = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_plain(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        let mut lines = vec![
            format!("SigVault key backup — {}", self.slot_label),
            String::new(),
            "KEEP THIS SECRET. Anyone with these words can spend with this key.".into(),
            String::new(),
        ];
        lines.extend(self.words.iter().enumerate().map(|(i, w)| format!("{:>2}. {w}", i + 1)));
        lines.push(String::new());
        lines.push(format!("Master fingerprint: {}", self.derived.fingerprint));
        lines.push(format!("Derivation path:    {}", self.derived.derivation_path));
        lines.push(format!("Public key:         {}", self.derived.key_expr));
        lines.push(String::new());
        let text = lines.join("\n");
        let name = format!("{}.txt", self.base_name);
        cx.spawn(async move |this, cx| {
            let res = save_text_file(cx, text, name).await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(Some(path)) => this.saved_files.push(path),
                    Ok(None) => {}
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_words(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let revealed = self.revealed;
        let rows: Vec<_> = self
            .words
            .chunks(3)
            .enumerate()
            .map(|(r, chunk)| {
                div().flex().flex_row().gap_2().children(chunk.iter().enumerate().map(|(c, w)| {
                    let i = r * 3 + c;
                    div()
                        .flex_1()
                        .flex_basis(px(0.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .border_1()
                        .border_color(t.border)
                        .bg(t.card)
                        .px(px(10.))
                        .py_2()
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(10.))
                                .text_color(alpha(t.muted_foreground, 0.7))
                                .child(format!("{:02}", i + 1)),
                        )
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(12.))
                                .text_color(if revealed { t.foreground } else { alpha(t.muted_foreground, 0.35) })
                                .child(if revealed { w.clone() } else { "••••••".to_string() }),
                        )
                }))
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(setup_alert(
                AlertTone::Warning,
                Some("Write these words down on paper, in order.".into()),
                Some(
                    "They are the only way to restore this key. Anyone who sees them can spend with it. Don't screenshot them or paste them into other apps."
                        .into_any_element(),
                ),
            ))
            .child(
                div()
                    .relative()
                    .child(div().flex().flex_col().gap_2().children(rows))
                    .when(!revealed, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .id("reveal")
                                        .rounded_md()
                                        .border_1()
                                        .border_color(t.border)
                                        .bg(t.background)
                                        .px_4()
                                        .py_2()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .shadow_sm()
                                        .cursor_pointer()
                                        .hover(move |s| s.border_color(t.primary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.revealed = true;
                                            cx.notify();
                                        }))
                                        .child("Make sure no one is watching, then reveal"),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        secondary_button("discard-key", "Discard key")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(KeyBackupEvent::Discard))),
                    )
                    .child(
                        div().flex_1().child(
                            primary_button("written", "I've written them down")
                                .full_width()
                                .icon_right("arrow-right")
                                .disabled(!revealed)
                                .on_click(cx.listener(|this, _, _, cx| this.go_quiz(cx))),
                        ),
                    ),
            )
    }

    fn render_quiz(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let correct = self.correct(cx);
        let all_correct = correct.iter().all(|c| *c);
        let any_blank = self.answers.iter().any(|a| a.read(cx).text().trim().is_empty());
        let checked = self.checked;

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(field_label(
                "Confirm your backup",
                Some("Enter the requested words from your written copy, not from memory of the screen.".into()),
            ))
            .child(div().flex().flex_row().gap_3().children(self.positions.iter().enumerate().map(
                |(i, p)| {
                    let wrong = checked && !correct[i];
                    div()
                        .flex_1()
                        .flex_basis(px(0.))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.muted_foreground)
                                .child(format!("Word #{}", p + 1)),
                        )
                        .child(
                            div()
                                .rounded_md()
                                .when(wrong, |d| d.border_1().border_color(alpha(t.destructive, 0.6)))
                                .child(self.answers[i].clone()),
                        )
                },
            )))
            .when(checked && !all_correct, |d| {
                d.child(alert_text(
                    AlertTone::Error,
                    "That doesn't match. Check your written copy, or go back and look at the words again.",
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(secondary_button("show-words", "← Show words").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.stage = Stage::Words;
                            cx.notify();
                        },
                    )))
                    .child(
                        div().flex_1().child(
                            primary_button("confirm-quiz", "Confirm")
                                .full_width()
                                .disabled(any_blank)
                                .on_click(cx.listener(|this, _, _, cx| this.submit_quiz(cx))),
                        ),
                    ),
            )
    }

    fn render_files(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let password = self.password.read(cx).text().to_string();
        let confirm = self.confirm.read(cx).text().to_string();
        let password_ok = password.chars().count() >= 8 && password == confirm;
        let box_ = || {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .rounded_md()
                .border_1()
                .border_color(t.border)
                .bg(alpha(t.card, 0.4))
                .p_4()
        };

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(setup_alert(
                AlertTone::Success,
                Some("Backup confirmed.".into()),
                Some(
                    "Optionally keep an extra copy below. The written words remain your primary backup."
                        .into_any_element(),
                ),
            ))
            .child(
                box_()
                    .child(field_label(
                        "Encrypted key file",
                        Some("Encrypted with a password of your choice (Argon2id + ChaCha20-Poly1305). Import it back later with the same password.".into()),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(div().flex_1().flex_basis(px(0.)).child(self.password.clone()))
                            .child(div().flex_1().flex_basis(px(0.)).child(self.confirm.clone())),
                    )
                    .when(!confirm.is_empty() && password != confirm, |d| {
                        d.child(error_line("Passwords do not match"))
                    })
                    .child(
                        secondary_button(
                            "save-enc",
                            if self.busy { "Encrypting…" } else { "Save .sigvault-key file" },
                        )
                        .full_width()
                        .disabled(!password_ok || self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.save_encrypted(cx))),
                    ),
            )
            .child(
                box_()
                    .child(field_label(
                        "Plaintext copy",
                        Some("An unencrypted text file with the words. Anyone who finds the file can spend with this key. Only use it if you're moving it straight to offline storage.".into()),
                    ))
                    .child(checkbox(
                        "plain-ack",
                        self.plain_ack,
                        "I understand this file is not protected.",
                        {
                            let this = cx.entity().downgrade();
                            move |v, _, cx| {
                                this.update(cx, |this, cx| {
                                    this.plain_ack = v;
                                    cx.notify();
                                })
                                .ok();
                            }
                        },
                    ))
                    .child(
                        secondary_button("save-plain", "Save .txt file")
                            .full_width()
                            .disabled(!self.plain_ack)
                            .on_click(cx.listener(|this, _, _, cx| this.save_plain(cx))),
                    ),
            )
            .when(!self.saved_files.is_empty(), |d| {
                d.child(div().flex().flex_col().gap_1().children(self.saved_files.iter().map(|p| {
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(11.))
                        .text_color(t.muted_foreground)
                        .truncate()
                        .child(format!("✓ {p}"))
                })))
            })
            .children(self.error.clone().map(|e| alert_text(AlertTone::Error, e)))
            .child(
                primary_button("use-key", self.done_label.clone())
                    .full_width()
                    .icon_right("arrow-right")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(KeyBackupEvent::Done))),
            )
    }
}

impl Render for KeyBackup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let stage = self.stage;
        let crumbs = [(Stage::Words, "Write down"), (Stage::Quiz, "Confirm"), (Stage::Files, "Extra copies")];
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.muted_foreground)
                    .children(crumbs.iter().enumerate().map(|(i, (s, label))| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .when(i > 0, |d| d.child(div().text_color(alpha(t.muted_foreground, 0.5)).child("›")))
                            .child(
                                div()
                                    .when(*s == stage, |d| d.text_color(t.foreground))
                                    .child(*label),
                            )
                    })),
            )
            .map(|d| match stage {
                Stage::Words => d.child(self.render_words(cx)),
                Stage::Quiz => d.child(self.render_quiz(cx)),
                Stage::Files => d.child(self.render_files(cx)),
            })
    }
}
