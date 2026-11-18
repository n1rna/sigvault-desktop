//! The Send wizard's two QR dialogs.
//!
//! - Show QR — port of `components/AnimatedQrModal.tsx`: cycles BBQr / UR
//!   frames at 5 fps with a GPUI timer.
//! - Scan QR — replacement for `components/QrScanModal.tsx`. GPUI has no
//!   camera API, so instead of a webcam feed the user picks screenshots /
//!   photos of the signer's QR display (several at once for an animated
//!   sequence), or pastes the base64 PSBT / UR / BBQr parts as text. Frames
//!   go through the same `PsbtQrAssembler` as before.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, ClickEvent, Context, Entity, FontWeight, PathPromptOptions, SharedString,
    Task, Window,
};

use super::SendPage;
use crate::psbt::{
    file::decode_psbt_text,
    image::decode_qr_image,
    qr::{psbt_to_frames, PsbtQrAssembler, QrFormat},
};
use crate::theme::{alpha, theme};
use crate::ui::{alert, button, icon_button, modal, spinner, InputEvent, QrCode, TextInput, Tone};

const FRAMES_PER_SECOND: u64 = 5;

// ───────────────────────── Show QR ─────────────────────────

pub struct AnimatedQr {
    format: QrFormat,
    frames: Vec<String>,
    total_parts: usize,
    frame_idx: usize,
    error: Option<SharedString>,
    _ticker: Option<Task<()>>,
}

impl SendPage {
    pub(super) fn open_show_qr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.psbt_base64.is_none() {
            return;
        }
        self.show_qr = Some(AnimatedQr {
            format: QrFormat::Bbqr,
            frames: Vec::new(),
            total_parts: 0,
            frame_idx: 0,
            error: None,
            _ticker: None,
        });
        self.set_qr_format(QrFormat::Bbqr, cx);
        window.focus(&self.focus);
    }

    fn set_qr_format(&mut self, format: QrFormat, cx: &mut Context<Self>) {
        let Some(psbt) = self.psbt_base64.clone() else { return };
        let Some(q) = self.show_qr.as_mut() else { return };
        q.format = format;
        q.error = None;
        q.frame_idx = 0;
        match psbt_to_frames(&psbt, format) {
            Ok(out) => {
                q.frames = out.frames;
                q.total_parts = out.total_parts;
            }
            Err(e) => {
                q.error = Some(e.into());
                q.frames.clear();
            }
        }
        q._ticker = (q.frames.len() > 1).then(|| {
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(1000 / FRAMES_PER_SECOND))
                    .await;
                let alive = this
                    .update(cx, |this, cx| match this.show_qr.as_mut() {
                        Some(q) if !q.frames.is_empty() => {
                            q.frame_idx = (q.frame_idx + 1) % q.frames.len();
                            cx.notify();
                            true
                        }
                        _ => false,
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            })
        });
        cx.notify();
    }

    pub(super) fn close_show_qr(&mut self, cx: &mut Context<Self>) {
        self.show_qr = None;
        cx.notify();
    }

    pub(super) fn render_show_qr(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let q = self.show_qr.as_ref()?;
        let t = theme();
        let format = q.format;
        let current = q.frames.get(q.frame_idx).cloned().unwrap_or_default();
        // UR is case-insensitive; uppercase lets the QR use alphanumeric
        // mode (denser, easier to scan), as Sparrow/Passport emit it.
        let current = if format == QrFormat::Ur { current.to_uppercase() } else { current };
        let display_idx = if q.total_parts > 0 { q.frame_idx % q.total_parts + 1 } else { 1 };

        Some(
            modal("show-qr-modal", 480.)
                .on_dismiss(cx.listener(|this, _, _, cx| this.close_show_qr(cx)))
                .child(dialog_header(
                    "Sign on another machine",
                    cx.listener(|this, _, _, cx| this.close_show_qr(cx)),
                ))
                .child(segmented(
                    "qr-format",
                    [("BBQr", format == QrFormat::Bbqr), ("UR", format == QrFormat::Ur)],
                    cx.listener(|this, _, _, cx| this.set_qr_format(QrFormat::Bbqr, cx)),
                    cx.listener(|this, _, _, cx| this.set_qr_format(QrFormat::Ur, cx)),
                ))
                .child(info_box(if format == QrFormat::Bbqr {
                    rich(
                        "BBQr",
                        " works with Coldcard, SeedSigner, Krux. Hold this QR up to the device's camera; it will read the frames as they cycle.",
                    )
                } else {
                    rich(
                        "UR",
                        " works with Sparrow, Foundation Passport, Specter, Keystone. Fountain-coded — scan order doesn't matter, the device can pick frames out of sequence.",
                    )
                }))
                .map(|m| match q.error.clone() {
                    Some(e) => m.child(alert(Tone::Destructive, e)),
                    None => m
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .border_1()
                                .border_color(t.border)
                                .bg(gpui::white())
                                .p_4()
                                .when(!current.is_empty(), |d| d.child(QrCode::new(current, px(320.)))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .text_size(px(12.))
                                .text_color(t.muted_foreground)
                                .child(format!("Frame {} / {}", q.frame_idx + 1, q.frames.len()))
                                .child(format!("Part {} of {}", display_idx, q.total_parts)),
                        ),
                }),
        )
    }
}

// ───────────────────────── Scan QR ─────────────────────────

pub struct QrImport {
    assembler: PsbtQrAssembler,
    seen: HashSet<String>,
    progress: (usize, usize),
    frames_read: usize,
    decoding: bool,
    error: Option<SharedString>,
    paste: Entity<TextInput>,
}

impl SendPage {
    pub(super) fn open_scan_qr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paste = cx.new(|cx| {
            TextInput::new(window, cx)
                .mono()
                .multiline(4)
                .placeholder("Base64 PSBT, or UR / BBQr parts (one per line)")
        });
        cx.subscribe(&paste, |_, _, _: &InputEvent, cx| cx.notify()).detach();
        self.scan_qr = Some(QrImport {
            assembler: PsbtQrAssembler::new(),
            seen: HashSet::new(),
            progress: (0, 0),
            frames_read: 0,
            decoding: false,
            error: None,
            paste,
        });
        window.focus(&self.focus);
        cx.notify();
    }

    pub(super) fn close_scan_qr(&mut self, cx: &mut Context<Self>) {
        self.scan_qr = None;
        cx.notify();
    }

    fn reset_scan(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.scan_qr.as_mut() {
            s.assembler = PsbtQrAssembler::new();
            s.seen.clear();
            s.progress = (0, 0);
            s.frames_read = 0;
            s.error = None;
            cx.notify();
        }
    }

    /// Feed decoded frame strings into the assembler. On completion the
    /// dialog closes and the signed PSBT goes to broadcast.
    fn push_frames(&mut self, frames: Vec<String>, cx: &mut Context<Self>) {
        let Some(s) = self.scan_qr.as_mut() else { return };
        let mut done = None;
        for f in frames {
            let f = f.trim().to_string();
            if f.is_empty() || !s.seen.insert(f.clone()) {
                continue;
            }
            s.frames_read += 1;
            match s.assembler.push(&f) {
                Ok(Some(psbt)) => {
                    done = Some(psbt);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    s.error = Some(e.into());
                    break;
                }
            }
        }
        s.progress = s.assembler.progress();
        cx.notify();
        if let Some(psbt) = done {
            self.on_scanned_signed_psbt(psbt, cx);
        }
    }

    fn choose_qr_images(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Choose QR images".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            if paths.is_empty() {
                return;
            }
            this.update(cx, |this, cx| {
                if let Some(s) = this.scan_qr.as_mut() {
                    s.decoding = true;
                    s.error = None;
                    cx.notify();
                }
            })
            .ok();
            let results = cx
                .background_executor()
                .spawn(async move { decode_images(paths) })
                .await;
            this.update(cx, |this, cx| {
                let Some(s) = this.scan_qr.as_mut() else { return };
                s.decoding = false;
                let mut frames = Vec::new();
                let mut errors = Vec::new();
                for r in results {
                    match r {
                        Ok(f) => frames.extend(f),
                        Err(e) => errors.push(e),
                    }
                }
                if !errors.is_empty() {
                    s.error = Some(errors.join("\n").into());
                }
                this.push_frames(frames, cx);
            })
            .ok();
        })
        .detach();
    }

    fn decode_pasted(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.scan_qr.as_mut() else { return };
        let text = s.paste.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        s.error = None;
        let first = text.split_whitespace().next().unwrap_or_default();
        let is_frames = first.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("ur:")) || first.starts_with("B$");
        if is_frames {
            let frames: Vec<String> = text.split_whitespace().map(str::to_string).collect();
            s.paste.update(cx, |p, cx| p.clear(cx));
            self.push_frames(frames, cx);
        } else if let Some(raw) = decode_psbt_text(&text) {
            self.on_scanned_signed_psbt(crate::psbt::bytes_to_base64(&raw), cx);
        } else {
            s.error = Some("Couldn't find a PSBT in the pasted text. Paste a base64 PSBT or the UR / BBQr parts.".into());
            cx.notify();
        }
    }

    pub(super) fn render_scan_qr(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let s = self.scan_qr.as_ref()?;
        let t = theme();
        let (received, expected) = s.progress;
        let has_paste = !s.paste.read(cx).text().trim().is_empty();
        let started = s.frames_read > 0;

        Some(
            modal("scan-qr-modal", 520.)
                .on_dismiss(cx.listener(|this, _, _, cx| this.close_scan_qr(cx)))
                .child(dialog_header(
                    "Scan signed PSBT",
                    cx.listener(|this, _, _, cx| this.close_scan_qr(cx)),
                ))
                .child(info_box(
                    div().child(
                        "Take a screenshot or photo of each QR your signing device displays, then choose the images below — select every frame of an animated QR at once. Both BBQr (Coldcard, SeedSigner, Krux) and UR (Sparrow, Passport, Specter, Keystone) frames are accepted — the format is detected automatically from the first frame.",
                    ),
                ))
                .children(s.error.clone().map(|e| alert(Tone::Destructive, e)))
                .child(
                    button("choose-qr-images", if s.decoding { "Reading images…" } else { "Choose QR images…" })
                        .outline()
                        .large()
                        .full_width()
                        .icon("scan")
                        .loading(s.decoding)
                        .on_click(cx.listener(|this, _, _, cx| this.choose_qr_images(cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .text_size(px(12.))
                        .text_color(t.muted_foreground)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .when(s.decoding, |d| d.child(spinner(px(12.))))
                                .child(if s.decoding {
                                    "Scanning…".to_string()
                                } else if started {
                                    format!("{} frame{} read", s.frames_read, if s.frames_read == 1 { "" } else { "s" })
                                } else {
                                    "Waiting for frames…".to_string()
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .when(expected > 0, |d| d.child(format!("{received} / {expected} parts")))
                                .when(started, |d| {
                                    d.child(
                                        button("reset-scan", "Reset")
                                            .ghost()
                                            .small()
                                            .on_click(cx.listener(|this, _, _, cx| this.reset_scan(cx))),
                                    )
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1().h(px(1.)).bg(alpha(t.border, 0.6)))
                        .child(div().text_size(px(11.)).text_color(t.muted_foreground).child("or paste"))
                        .child(div().flex_1().h(px(1.)).bg(alpha(t.border, 0.6))),
                )
                .child(s.paste.clone())
                .child(
                    button("decode-pasted", "Decode pasted text")
                        .disabled(!has_paste || s.decoding)
                        .full_width()
                        .on_click(cx.listener(|this, _, _, cx| this.decode_pasted(cx))),
                ),
        )
    }
}

fn decode_images(paths: Vec<PathBuf>) -> Vec<Result<Vec<String>, String>> {
    paths.iter().map(|p| decode_qr_image(p)).collect()
}

// ───────────────────────── shared bits ─────────────────────────

fn dialog_header(
    title: &'static str,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme().foreground)
                .child(title),
        )
        .child(icon_button("dialog-close", "x").on_click(on_close))
}

fn info_box(content: impl IntoElement) -> impl IntoElement {
    let t = theme();
    div()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(alpha(t.card, 0.4))
        .px_4()
        .py_3()
        .text_size(px(12.))
        .line_height(px(19.))
        .text_color(t.muted_foreground)
        .child(content)
}

/// "**Bold** rest of sentence" — the TSX `<span font-medium>` lead-in.
fn rich(lead: &'static str, rest: &'static str) -> gpui::Div {
    let t = theme();
    div().child(
        gpui::StyledText::new(format!("{lead}{rest}")).with_highlights([(
            0..lead.len(),
            gpui::HighlightStyle {
                color: Some(t.foreground),
                font_weight: Some(FontWeight::MEDIUM),
                ..Default::default()
            },
        )]),
    )
}

/// Two-option segmented toggle (`grid grid-cols-2` buttons in the TSX).
pub(super) fn segmented(
    id: &'static str,
    items: [(&'static str, bool); 2],
    on_first: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    on_second: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let t = theme();
    let seg = |ix: usize, (label, active): (&'static str, bool)| {
        div()
            .id((id, ix))
            .flex_1()
            .h(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .map(|d| {
                if active {
                    d.bg(t.primary).text_color(t.primary_foreground)
                } else {
                    d.text_color(t.muted_foreground).hover(move |s| s.text_color(t.foreground))
                }
            })
            .child(label)
    };
    div()
        .flex()
        .flex_row()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(alpha(t.card, 0.4))
        .p_1()
        .child(seg(0, items[0]).on_click(on_first))
        .child(seg(1, items[1]).on_click(on_second))
}
