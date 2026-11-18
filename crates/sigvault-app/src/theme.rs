//! "Vault Instrument" palette (dark), converted from the oklch tokens in the
//! Tauri build's `globals.css`.

use gpui::{rgb, Hsla, Rgba};

#[derive(Clone, Copy)]
pub struct Theme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub card: Hsla,
    pub popover: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub destructive: Hsla,
    pub destructive_foreground: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    pub sand: Hsla,
    pub bitcoin: Hsla,
}

fn c(hex: u32) -> Hsla {
    let rgba: Rgba = rgb(hex);
    rgba.into()
}

pub fn theme() -> Theme {
    Theme {
        background: c(0x040a19),
        foreground: c(0xf2ebd9),
        card: c(0x091224),
        popover: c(0x0b1629),
        primary: c(0x2ccceb),
        primary_foreground: c(0x020819),
        secondary: c(0x121e33),
        muted: c(0x0f192c),
        muted_foreground: c(0x779fb0),
        accent: c(0x0087bc),
        accent_foreground: c(0xfdf8eb),
        destructive: c(0xee3533),
        destructive_foreground: c(0xfcf8f0),
        success: c(0x00c0a0),
        warning: c(0xebbc63),
        border: c(0x103243),
        input: c(0x0a2c3e),
        ring: c(0x2ccceb),
        sand: c(0xe2cc95),
        bitcoin: c(0xf7941d),
    }
}

/// Tailwind-style `color/NN` alpha helper.
pub fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla { a, ..color }
}

pub const FONT_MONO: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "DejaVu Sans Mono"
};
