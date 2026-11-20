//! Text input: single-line, masked (passphrase) and multi-line variants.
//!
//! Adapted from GPUI's `examples/input.rs`, extended with a placeholder,
//! masking, soft-wrapped multi-line editing, disabled state and change /
//! submit events.
//!
//! ```ignore
//! let name = cx.new(|cx| TextInput::new(window, cx).placeholder("Wallet name"));
//! cx.subscribe(&name, |this, _, ev: &InputEvent, cx| { /* ... */ }).detach();
//! // render: .child(self.name.clone())
//! // read:   self.name.read(cx).text()
//! ```

use std::ops::Range;

use gpui::{
    actions, div, fill, point, prelude::*, px, relative, size, App, Bounds, ClipboardItem, Context,
    CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::e2e::{E2e, Probe};
use crate::theme::{alpha, theme, FONT_MONO, RADIUS_CONTROL};

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Enter,
        Undo,
        Redo,
    ]
);

const CONTEXT: &str = "TextInput";
const MASK_CHAR: char = '•';

pub fn bind_keys(cx: &mut App) {
    let ctx = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("enter", Enter, ctx),
        // Tab / Shift-Tab / Esc are global (`ui::focus`).
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-a", SelectAll, ctx),
        KeyBinding::new("cmd-v", Paste, ctx),
        KeyBinding::new("cmd-c", Copy, ctx),
        KeyBinding::new("cmd-x", Cut, ctx),
        KeyBinding::new("cmd-z", Undo, ctx),
        KeyBinding::new("cmd-shift-z", Redo, ctx),
        KeyBinding::new("cmd-left", Home, ctx),
        KeyBinding::new("cmd-right", End, ctx),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        KeyBinding::new("ctrl-v", Paste, ctx),
        KeyBinding::new("ctrl-c", Copy, ctx),
        KeyBinding::new("ctrl-x", Cut, ctx),
        KeyBinding::new("ctrl-z", Undo, ctx),
        KeyBinding::new("ctrl-y", Redo, ctx),
        KeyBinding::new("ctrl-shift-z", Redo, ctx),
    ]);
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Content changed (typing, paste, `set_text` does NOT emit).
    Changed,
    /// Enter pressed in a single-line input.
    Submit,
}

// One layout lives per input and is rebuilt every frame; boxing the single
// line would just add an allocation.
#[allow(clippy::large_enum_variant)]
enum Layout {
    Single(ShapedLine),
    Multi(Vec<(usize, WrappedLine)>), // (byte offset of hard line start, layout)
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<Layout>,
    last_bounds: Option<Bounds<Pixels>>,
    last_rows: usize,
    is_selecting: bool,
    masked: bool,
    revealed: bool,
    mono: bool,
    disabled: bool,
    invalid: bool,
    /// `Some(rows)` makes this a soft-wrapped multi-line editor that grows
    /// from `rows` lines.
    multiline: Option<usize>,
    max_len: Option<usize>,
    /// Accessible name for the e2e driver (defaults to the placeholder).
    label: Option<SharedString>,
    /// Undo / redo snapshots: (content, selection). Not kept for masked
    /// inputs, so passphrases aren't copied around.
    undo: Vec<(SharedString, Range<usize>)>,
    redo: Vec<(SharedString, Range<usize>)>,
    /// Last edit typed one character: consecutive ones undo as a group.
    typing: bool,
}

const UNDO_LIMIT: usize = 200;

impl EventEmitter<InputEvent> for TextInput {}

impl TextInput {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            content: "".into(),
            placeholder: "".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_rows: 1,
            is_selecting: false,
            masked: false,
            revealed: false,
            mono: false,
            disabled: false,
            invalid: false,
            multiline: None,
            max_len: None,
            label: None,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
        }
    }

    // ── builders ──

    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Accessible name the e2e driver reports (e.g. "Word 1"), for inputs
    /// whose placeholder doesn't name them. Defaults to the placeholder.
    pub fn label(mut self, text: impl Into<SharedString>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// Render content as bullets (passphrases, PINs).
    pub fn masked(mut self) -> Self {
        self.masked = true;
        self
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn multiline(mut self, rows: usize) -> Self {
        self.multiline = Some(rows.max(1));
        self.last_rows = rows.max(1);
        self
    }

    pub fn max_len(mut self, n: usize) -> Self {
        self.max_len = Some(n);
        self
    }

    pub fn with_text(mut self, text: impl Into<SharedString>) -> Self {
        self.content = text.into();
        let len = self.content.len();
        self.selected_range = len..len;
        self
    }

    // ── accessors ──

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.undo.clear();
        self.redo.clear();
        self.typing = false;
        self.content = text.into();
        let len = self.content.len();
        self.selected_range = len..len;
        self.marked_range = None;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_text("", cx);
    }

    pub fn set_placeholder(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = text.into();
        cx.notify();
    }

    /// Change the accessible name (see [`TextInput::label`]).
    pub fn set_label(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.label = Some(text.into());
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }

    pub fn set_invalid(&mut self, invalid: bool, cx: &mut Context<Self>) {
        if self.invalid != invalid {
            self.invalid = invalid;
            cx.notify();
        }
    }

    /// For masked inputs: show the plain text instead of bullets.
    pub fn set_revealed(&mut self, revealed: bool, cx: &mut Context<Self>) {
        self.revealed = revealed;
        cx.notify();
    }

    pub fn is_revealed(&self) -> bool {
        self.revealed
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    fn is_masking(&self) -> bool {
        self.masked && !self.revealed
    }

    // ── actions ──

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn vertical(&mut self, dir: f32, window: &mut Window, cx: &mut Context<Self>) {
        let line_height = window.line_height();
        let Some(pos) = self.position_for_offset(self.cursor_offset(), line_height) else {
            return;
        };
        let target = point(pos.x, pos.y + line_height * dir + line_height / 2.);
        if let Some(ix) = self.offset_for_local_point(target, line_height) {
            self.move_to(ix, cx);
        }
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline.is_some() {
            self.vertical(-1., window, cx);
        } else {
            self.move_to(0, cx);
        }
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline.is_some() {
            self.vertical(1., window, cx);
        } else {
            self.move_to(self.content.len(), cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let start = if self.multiline.is_some() {
            self.content[..self.cursor_offset()]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0)
        } else {
            0
        };
        self.move_to(start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let end = if self.multiline.is_some() {
            let c = self.cursor_offset();
            self.content[c..]
                .find('\n')
                .map(|i| c + i)
                .unwrap_or(self.content.len())
        } else {
            self.content.len()
        };
        self.move_to(end, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline.is_some() {
            if !self.disabled {
                self.replace_text_in_range(None, "\n", window, cx);
            }
        } else {
            cx.emit(InputEvent::Submit);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some((content, sel)) = self.undo.pop() {
            self.redo
                .push((self.content.clone(), self.selected_range.clone()));
            self.restore(content, sel, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some((content, sel)) = self.redo.pop() {
            self.undo
                .push((self.content.clone(), self.selected_range.clone()));
            self.restore(content, sel, cx);
        }
    }

    fn restore(&mut self, content: SharedString, sel: Range<usize>, cx: &mut Context<Self>) {
        self.content = content;
        let len = self.content.len();
        self.selected_range = sel.start.min(len)..sel.end.min(len);
        self.marked_range = None;
        self.typing = false;
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    /// Snapshot before an edit (grouping runs of typed characters).
    fn record_undo(&mut self, range: &Range<usize>, new_text: &str) {
        if self.masked {
            return;
        }
        let typing = range.is_empty()
            && new_text.chars().count() == 1
            && !new_text.chars().all(char::is_whitespace);
        if !(typing && self.typing) {
            self.undo
                .push((self.content.clone(), self.selected_range.clone()));
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.typing = typing;
        self.redo.clear();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline.is_some() {
                text.replace("\r\n", "\n")
            } else {
                text.replace(['\n', '\r'], " ").trim().to_string()
            };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.is_masking() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.is_masking() && !self.disabled {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        let ix = self.index_for_mouse_position(event.position, window);
        if event.modifiers.shift {
            self.select_to(ix, cx);
        } else if event.click_count >= 2 {
            self.move_to(0, cx);
            self.select_to(self.content.len(), cx);
        } else {
            self.move_to(ix, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let ix = self.index_for_mouse_position(event.position, window);
            self.select_to(ix, cx);
        }
    }

    // ── helpers ──

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.typing = false;
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    /// Content byte offset → offset into the displayed (possibly masked) text.
    fn display_offset(&self, offset: usize) -> usize {
        if self.is_masking() {
            self.content[..offset.min(self.content.len())]
                .chars()
                .count()
                * MASK_CHAR.len_utf8()
        } else {
            offset
        }
    }

    /// Displayed-text offset → content byte offset.
    fn content_offset(&self, display: usize) -> usize {
        if self.is_masking() {
            let n = display / MASK_CHAR.len_utf8();
            self.content
                .char_indices()
                .nth(n)
                .map(|(i, _)| i)
                .unwrap_or(self.content.len())
        } else {
            display.min(self.content.len())
        }
    }

    fn position_for_offset(&self, offset: usize, line_height: Pixels) -> Option<Point<Pixels>> {
        match self.last_layout.as_ref()? {
            Layout::Single(line) => {
                Some(point(line.x_for_index(self.display_offset(offset)), px(0.)))
            }
            Layout::Multi(lines) => {
                let mut y = px(0.);
                for (i, (start, line)) in lines.iter().enumerate() {
                    let end = start + line.len();
                    let last = i + 1 == lines.len();
                    if offset >= *start && (offset <= end || last) {
                        let p = line.position_for_index(offset - start, line_height)?;
                        return Some(point(p.x, p.y + y));
                    }
                    y += line.size(line_height).height;
                }
                None
            }
        }
    }

    /// Point relative to the text origin → content offset.
    fn offset_for_local_point(&self, p: Point<Pixels>, line_height: Pixels) -> Option<usize> {
        match self.last_layout.as_ref()? {
            Layout::Single(line) => Some(self.content_offset(line.closest_index_for_x(p.x))),
            Layout::Multi(lines) => {
                let mut y = px(0.);
                for (i, (start, line)) in lines.iter().enumerate() {
                    let h = line.size(line_height).height;
                    if p.y < y + h || i + 1 == lines.len() {
                        let local = point(p.x.max(px(0.)), (p.y - y).max(px(0.)).min(h - px(1.)));
                        let ix = match line.closest_index_for_position(local, line_height) {
                            Ok(ix) | Err(ix) => ix,
                        };
                        return Some(start + ix.min(line.len()));
                    }
                    y += h;
                }
                Some(self.content.len())
            }
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>, window: &Window) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let Some(bounds) = self.last_bounds.as_ref() else {
            return 0;
        };
        if self.multiline.is_none() {
            if position.y < bounds.top() {
                return 0;
            }
            if position.y > bounds.bottom() {
                return self.content.len();
            }
        }
        let local = point(position.x - bounds.left(), position.y - bounds.top());
        self.offset_for_local_point(local, window.line_height())
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let mut new_text = if self.multiline.is_some() {
            new_text.to_string()
        } else {
            new_text.replace('\n', "")
        };
        if let Some(max) = self.max_len {
            let remaining = max.saturating_sub(
                self.content.chars().count() - self.content[range.clone()].chars().count(),
            );
            new_text = new_text.chars().take(remaining).collect();
        }
        self.record_undo(&range, &new_text);

        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let lh = window.line_height();
        let a = self.position_for_offset(range.start, lh)?;
        let b = self.position_for_offset(range.end, lh)?;
        Some(Bounds::from_corners(
            point(bounds.left() + a.x, bounds.top() + a.y),
            point(bounds.left() + b.x, bounds.top() + b.y + lh),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: gpui::Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let local = point(p.x - bounds.left(), p.y - bounds.top());
        let ix = self.offset_for_local_point(local, window.line_height())?;
        Some(self.offset_to_utf16(ix))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    layout: Option<Layout>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    rows: usize,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let rows = match input.multiline {
            Some(min) => input.last_rows.max(min),
            None => 1,
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let t = theme();
        let input = self.input.read(cx);
        let content = input.content.clone();
        let style = window.text_style();
        let line_height = window.line_height();
        let font_size = style.font_size.to_pixels(window.rem_size());

        let (display_text, text_color): (SharedString, _) = if content.is_empty() {
            (input.placeholder.clone(), t.ink_3)
        } else if input.is_masking() {
            (
                MASK_CHAR.to_string().repeat(content.chars().count()).into(),
                style.color,
            )
        } else {
            (content.clone(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let marked = if content.is_empty() {
            None
        } else {
            input.marked_range.clone()
        };
        let runs: Vec<TextRun> = if let Some(m) = marked.as_ref() {
            let (ms, me) = (input.display_offset(m.start), input.display_offset(m.end));
            vec![
                TextRun {
                    len: ms,
                    ..run.clone()
                },
                TextRun {
                    len: me - ms,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - me,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let selection_color = alpha(t.accent, 0.45);
        let cursor_color = t.primary;
        let sel = input.selected_range.clone();
        let cursor_ix = input.cursor_offset();

        if input.multiline.is_none() {
            let line = window
                .text_system()
                .shape_line(display_text, font_size, &runs, None);
            let (selections, cursor) = if content.is_empty() {
                (
                    vec![],
                    Some(fill(
                        Bounds::new(bounds.origin, size(px(1.5), line_height)),
                        cursor_color,
                    )),
                )
            } else if sel.is_empty() {
                let x = line.x_for_index(input.display_offset(cursor_ix));
                (
                    vec![],
                    Some(fill(
                        Bounds::new(
                            point(bounds.left() + x, bounds.top()),
                            size(px(1.5), line_height),
                        ),
                        cursor_color,
                    )),
                )
            } else {
                let a = line.x_for_index(input.display_offset(sel.start));
                let b = line.x_for_index(input.display_offset(sel.end));
                (
                    vec![fill(
                        Bounds::from_corners(
                            point(bounds.left() + a, bounds.top()),
                            point(bounds.left() + b, bounds.bottom()),
                        ),
                        selection_color,
                    )],
                    None,
                )
            };
            return PrepaintState {
                layout: Some(Layout::Single(line)),
                cursor,
                selections,
                rows: 1,
            };
        }

        // Multi-line: shape each hard line with soft wrapping at the bounds width.
        let wrapped = window
            .text_system()
            .shape_text(
                display_text.clone(),
                font_size,
                &runs,
                Some(bounds.size.width),
                None,
            )
            .unwrap_or_default();
        let mut lines = Vec::with_capacity(wrapped.len());
        let mut offset = 0usize;
        let mut rows = 0usize;
        for line in wrapped {
            let len = line.len();
            rows += line.wrap_boundaries().len() + 1;
            lines.push((offset, line));
            offset += len + 1;
        }
        let rows = rows.max(1);

        let pos = |ix: usize| -> Option<Point<Pixels>> {
            let mut y = px(0.);
            for (i, (start, line)) in lines.iter().enumerate() {
                let end = start + line.len();
                if ix >= *start && (ix <= end || i + 1 == lines.len()) {
                    let p = line.position_for_index(ix - start, line_height)?;
                    return Some(point(p.x, p.y + y));
                }
                y += line.size(line_height).height;
            }
            None
        };

        let mut selections = Vec::new();
        let mut cursor = None;
        if content.is_empty() {
            cursor = Some(fill(
                Bounds::new(bounds.origin, size(px(1.5), line_height)),
                cursor_color,
            ));
        } else if sel.is_empty() {
            if let Some(p) = pos(cursor_ix) {
                cursor = Some(fill(
                    Bounds::new(
                        point(bounds.left() + p.x, bounds.top() + p.y),
                        size(px(1.5), line_height),
                    ),
                    cursor_color,
                ));
            }
        } else if let (Some(a), Some(b)) = (pos(sel.start), pos(sel.end)) {
            if a.y == b.y {
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + a.x, bounds.top() + a.y),
                        point(bounds.left() + b.x, bounds.top() + a.y + line_height),
                    ),
                    selection_color,
                ));
            } else {
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + a.x, bounds.top() + a.y),
                        point(bounds.right(), bounds.top() + a.y + line_height),
                    ),
                    selection_color,
                ));
                if b.y > a.y + line_height {
                    selections.push(fill(
                        Bounds::from_corners(
                            point(bounds.left(), bounds.top() + a.y + line_height),
                            point(bounds.right(), bounds.top() + b.y),
                        ),
                        selection_color,
                    ));
                }
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left(), bounds.top() + b.y),
                        point(bounds.left() + b.x, bounds.top() + b.y + line_height),
                    ),
                    selection_color,
                ));
            }
        }

        PrepaintState {
            layout: Some(Layout::Multi(lines)),
            cursor,
            selections,
            rows,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for s in prepaint.selections.drain(..) {
            window.paint_quad(s);
        }
        let line_height = window.line_height();
        let layout = prepaint.layout.take();
        match layout.as_ref() {
            Some(Layout::Single(line)) => {
                line.paint(bounds.origin, line_height, window, cx).ok();
            }
            Some(Layout::Multi(lines)) => {
                let mut y = bounds.top();
                for (_, line) in lines {
                    line.paint(
                        point(bounds.left(), y),
                        line_height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
                    y += line.size(line_height).height;
                }
            }
            None => {}
        }

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        let rows = prepaint.rows;
        self.input.update(cx, |input, cx| {
            input.last_layout = layout;
            input.last_bounds = Some(bounds);
            if input.multiline.is_some() && input.last_rows != rows {
                input.last_rows = rows;
                cx.notify();
            }
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let focused = self.focus_handle.is_focused(window);
        let border = if self.invalid {
            alpha(t.destructive, 0.7)
        } else if focused {
            t.ring
        } else {
            t.line_control
        };
        let mut root = div()
            .flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .rounded(RADIUS_CONTROL)
            .border_1()
            .border_color(border)
            // Focus: a 2px ring outside the border (no layout shift).
            .when(focused && !self.invalid, |d| {
                d.shadow(vec![focus_ring(t.ring)])
            })
            .bg(t.inset)
            .px_3()
            .text_size(px(14.))
            .line_height(px(20.))
            .text_color(t.foreground);
        root = if self.multiline.is_some() {
            root.py_2()
        } else {
            root.py(px(7.))
        };
        if self.mono {
            root = root.font_family(FONT_MONO).text_size(px(12.5));
        }
        if self.disabled {
            root = root.opacity(0.55).cursor(CursorStyle::Arrow);
        }
        let shown = if self.masked {
            "•".repeat(self.content.chars().count())
        } else {
            self.content.to_string()
        };
        let focus = self.focus_handle.clone();
        root.e2e(
            Probe::new(
                "input",
                self.label
                    .clone()
                    .unwrap_or_else(|| self.placeholder.clone())
                    .to_string(),
            )
            .value(shown)
            .focused(focused)
            .disabled(self.disabled)
            .on_click(std::rc::Rc::new(move |_, window, _| window.focus(&focus))),
        )
        .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A solid ring drawn just outside an element's border.
pub fn focus_ring(color: gpui::Hsla) -> gpui::BoxShadow {
    gpui::BoxShadow {
        color: crate::theme::alpha(color, 0.45),
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
    }
}
