//! `setup/MnemonicInput.tsx` — BIP39 phrase entry: per-word autocomplete
//! against the English wordlist, a live checksum badge, and
//! paste-the-whole-phrase support (pasting into any cell spreads the
//! words across the grid).

use bip39::Language;
use gpui::{
    anchored, deferred, div, prelude::*, px, Context, Corner, Entity, EventEmitter, FontWeight,
    SharedString, Subscription, Window,
};

use super::ui::{seg, segmented};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::input::{Down, Enter, FocusNext, Up};
use crate::ui::{InputEvent, TextInput};

pub const PHRASE_LENGTHS: [usize; 5] = [12, 15, 18, 21, 24];
const MAX_WORDS: usize = 24;

fn wordlist() -> &'static [&'static str; 2048] {
    Language::English.word_list()
}

fn is_word(w: &str) -> bool {
    Language::English.find_word(w).is_some()
}

/// Suggestions are bounded to 5 and only shown from 2 typed characters,
/// which keeps the grid calm during fast typing.
pub fn suggestions_for(value: &str) -> Vec<&'static str> {
    let prefix = value.trim().to_lowercase();
    if prefix.len() < 2 {
        return vec![];
    }
    wordlist()
        .iter()
        .filter(|w| w.starts_with(&prefix))
        .take(5)
        .copied()
        .collect()
}

pub fn validate_mnemonic(phrase: &str) -> bool {
    bip39::Mnemonic::parse_in_normalized(Language::English, phrase).is_ok()
}

#[derive(Clone, Debug)]
pub enum MnemonicEvent {
    Changed,
    /// Enter pressed in a cell with no suggestion open (form submit).
    Submit,
}

pub struct MnemonicInput {
    length: usize,
    words: Vec<Entity<TextInput>>,
    highlight: usize,
    _subs: Vec<Subscription>,
}

impl EventEmitter<MnemonicEvent> for MnemonicInput {}

impl MnemonicInput {
    pub fn new(length: usize, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut words = Vec::with_capacity(MAX_WORDS);
        let mut subs = Vec::with_capacity(MAX_WORDS);
        for i in 0..MAX_WORDS {
            let input = cx.new(|cx| TextInput::new(window, cx).mono());
            subs.push(cx.subscribe(&input, move |this, _, ev: &InputEvent, cx| {
                match ev {
                    InputEvent::Changed => this.on_word_changed(i, cx),
                    InputEvent::Submit => cx.emit(MnemonicEvent::Submit),
                }
            }));
            words.push(input);
        }
        Self {
            length,
            words,
            highlight: 0,
            _subs: subs,
        }
    }

    fn word(&self, i: usize, cx: &gpui::App) -> String {
        self.words[i].read(cx).text().to_string()
    }

    /// Normalised words in use (`mnemonicString` split).
    pub fn phrase(&self, cx: &gpui::App) -> String {
        (0..self.length)
            .map(|i| self.word(i, cx).trim().to_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn is_complete(&self, cx: &gpui::App) -> bool {
        (0..self.length).all(|i| !self.word(i, cx).trim().is_empty())
    }

    pub fn is_valid(&self, cx: &gpui::App) -> bool {
        self.is_complete(cx) && validate_mnemonic(&self.phrase(cx))
    }

    fn on_word_changed(&mut self, i: usize, cx: &mut Context<Self>) {
        let text = self.word(i, cx);
        if !self.paste_at(i, &text, cx) {
            let norm = text.to_lowercase().trim().to_string();
            if norm != text {
                self.words[i].update(cx, |w, cx| w.set_text(norm, cx));
            }
        }
        self.highlight = 0;
        cx.emit(MnemonicEvent::Changed);
        cx.notify();
    }

    /// A multi-word paste spreads across the grid. A whole phrase pasted
    /// into the first cell also picks the length.
    fn paste_at(&mut self, i: usize, text: &str, cx: &mut Context<Self>) -> bool {
        let lower = text.trim().to_lowercase();
        let parts: Vec<&str> = lower.split_whitespace().collect();
        if parts.len() < 2 {
            return false;
        }
        if i == 0 && PHRASE_LENGTHS.contains(&parts.len()) {
            self.length = parts.len();
        }
        for (j, p) in parts.iter().take(MAX_WORDS - i).enumerate() {
            let p = p.to_string();
            self.words[i + j].update(cx, |w, cx| w.set_text(p, cx));
        }
        true
    }

    fn set_length(&mut self, length: usize, cx: &mut Context<Self>) {
        self.length = length;
        cx.emit(MnemonicEvent::Changed);
        cx.notify();
    }

    fn showing_suggestions(&self, i: usize, window: &Window, cx: &gpui::App) -> Vec<&'static str> {
        if !self.words[i].read(cx).is_focused(window) {
            return vec![];
        }
        let value = self.word(i, cx);
        let s = suggestions_for(&value);
        if s.contains(&value.as_str()) {
            return vec![];
        }
        s
    }

    fn accept(&mut self, i: usize, word: &str, cx: &mut Context<Self>) {
        let w = word.to_string();
        self.words[i].update(cx, |input, cx| input.set_text(w, cx));
        self.highlight = 0;
        cx.emit(MnemonicEvent::Changed);
        cx.notify();
    }

    fn render_cell(&self, i: usize, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let input = self.words[i].clone();
        let focused = input.read(cx).is_focused(window);
        let value = self.word(i, cx);
        let unknown = !value.is_empty() && !focused && !is_word(value.trim());
        let suggestions = self.showing_suggestions(i, window, cx);
        let highlight = self.highlight.min(suggestions.len().saturating_sub(1));

        div()
            .id(("word", i))
            .relative()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .capture_action(cx.listener(move |this, _: &Down, window, cx| {
                let s = this.showing_suggestions(i, window, cx);
                if !s.is_empty() {
                    cx.stop_propagation();
                    this.highlight = (this.highlight + 1).min(s.len() - 1);
                    cx.notify();
                }
            }))
            .capture_action(cx.listener(move |this, _: &Up, window, cx| {
                let s = this.showing_suggestions(i, window, cx);
                if !s.is_empty() {
                    cx.stop_propagation();
                    this.highlight = this.highlight.saturating_sub(1);
                    cx.notify();
                }
            }))
            .capture_action(cx.listener(move |this, _: &Enter, window, cx| {
                let s = this.showing_suggestions(i, window, cx);
                if !s.is_empty() {
                    cx.stop_propagation();
                    let w = s.get(this.highlight).unwrap_or(&s[0]);
                    this.accept(i, w, cx);
                }
            }))
            .capture_action(cx.listener(move |this, _: &FocusNext, window, cx| {
                let s = this.showing_suggestions(i, window, cx);
                if !s.is_empty() {
                    cx.stop_propagation();
                    let w = s.get(this.highlight).unwrap_or(&s[0]);
                    this.accept(i, w, cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .font_family(FONT_MONO)
                            .text_size(px(10.))
                            .text_color(alpha(t.muted_foreground, 0.7))
                            .child(format!("{:02}", i + 1)),
                    )
                    .child(div().flex_1().min_w_0().child(input)),
            )
            .when(unknown, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(24.))
                        .right_0()
                        .h(px(1.))
                        .bg(alpha(t.destructive, 0.6)),
                )
            })
            .when(!suggestions.is_empty(), |d| {
                d.child(
                    deferred(
                        anchored().anchor(Corner::TopLeft).snap_to_window().child(
                            div()
                                .id("suggestions")
                                .occlude()
                                .mt_1()
                                .ml(px(24.))
                                .w(px(150.))
                                .rounded_md()
                                .border_1()
                                .border_color(t.border)
                                .bg(t.card)
                                .shadow_lg()
                                .overflow_hidden()
                                .children(suggestions.iter().enumerate().map(|(j, s)| {
                                    let word = *s;
                                    div()
                                        .id(("sugg", j))
                                        .flex()
                                        .w_full()
                                        .px(px(10.))
                                        .py(px(4.))
                                        .font_family(FONT_MONO)
                                        .text_size(px(11.))
                                        .cursor_pointer()
                                        .map(|d| {
                                            if j == highlight {
                                                d.bg(alpha(t.primary, 0.10)).text_color(t.foreground)
                                            } else {
                                                d.text_color(t.muted_foreground).hover(move |s| {
                                                    s.bg(alpha(t.muted, 0.4)).text_color(t.foreground)
                                                })
                                            }
                                        })
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.accept(i, word, cx);
                                            }),
                                        )
                                        .child(word)
                                })),
                        ),
                    )
                    .with_priority(6),
                )
            })
    }
}

fn checksum_badge(complete: bool, valid: bool) -> impl IntoElement {
    let t = theme();
    let (border, bg, fg, text) = if !complete {
        (t.border, t.card, t.muted_foreground, "Incomplete")
    } else if valid {
        (alpha(t.success, 0.4), alpha(t.success, 0.08), t.success, "Valid checksum")
    } else {
        (alpha(t.destructive, 0.4), alpha(t.destructive, 0.06), t.destructive, "Bad checksum")
    };
    div()
        .rounded(px(3.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .px(px(6.))
        .py(px(2.))
        .text_size(px(10.))
        .text_color(fg)
        .child(text)
}

impl Render for MnemonicInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let complete = self.is_complete(cx);
        let valid = self.is_valid(cx);
        let length = self.length;
        let entity = cx.entity().downgrade();

        let rows: Vec<_> = (0..length)
            .collect::<Vec<_>>()
            .chunks(3)
            .map(|chunk| {
                let mut row = div().flex().flex_row().gap_2();
                for &i in chunk {
                    row = row.child(self.render_cell(i, window, cx));
                }
                for _ in chunk.len()..3 {
                    row = row.child(div().flex_1().flex_basis(px(0.)));
                }
                row
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.muted_foreground)
                                    .child("Recovery phrase"),
                            )
                            .child(checksum_badge(complete, valid)),
                    )
                    .child(segmented(
                        "phrase-length",
                        &length.to_string(),
                        PHRASE_LENGTHS.iter().map(|n| seg(n.to_string(), n.to_string())).collect(),
                        true,
                        move |v: SharedString, _, cx| {
                            if let Ok(n) = v.parse::<usize>() {
                                entity.update(cx, |this, cx| this.set_length(n, cx)).ok();
                            }
                        },
                    )),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.))
                    .line_height(px(17.))
                    .text_color(t.muted_foreground)
                    .child("Type each word, or paste the whole phrase into the first box. Tab accepts a suggestion."),
            )
            .child(div().mt_3().flex().flex_col().gap_2().children(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_from_two_characters() {
        assert!(suggestions_for("a").is_empty());
        let s = suggestions_for("ab");
        assert_eq!(s.len(), 5);
        assert!(s.iter().all(|w| w.starts_with("ab")));
        assert_eq!(suggestions_for("zoo"), vec!["zoo"]);
    }

    #[test]
    fn validates_checksum() {
        assert!(validate_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        ));
        assert!(!validate_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon"
        ));
    }
}
