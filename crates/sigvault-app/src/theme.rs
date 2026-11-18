//! SigVault "Instrument" palette (dark), ported 1:1 from the web dashboard
//! (`sigvault/apps/dashboard/app/globals.css`, `.dark` block).
//!
//! Structure follows the web app's Linear-style ladders:
//!   surfaces  page < canvas < surface < surface-2 < surface-3, inset/hover wells
//!   ink       ink > ink-muted > ink-2 > ink-3 > ink-4
//!   lines     line < line-strong < line-tertiary
//! Hierarchy comes from the surface ladder plus hairlines, not drop shadows.
//!
//! The shadcn-style names (`background`, `card`, `muted_foreground`, …) are kept
//! and mapped onto the ladder exactly as the web app's `--background`, `--card`,
//! … are, so existing pages pick up the new look without changes.

use std::borrow::Cow;

use gpui::{px, rgb, App, Hsla, Pixels, Rgba};

#[derive(Clone, Copy)]
pub struct Theme {
    // ── ladder ──
    pub page: Hsla,
    pub canvas: Hsla,
    pub surface: Hsla,
    pub surface_2: Hsla,
    pub surface_3: Hsla,
    pub inset: Hsla,
    pub hover: Hsla,
    pub hover_strong: Hsla,
    pub ink: Hsla,
    pub ink_muted: Hsla,
    pub ink_2: Hsla,
    pub ink_3: Hsla,
    pub ink_4: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub line_tertiary: Hsla,
    pub brand: Hsla,
    pub brand_contrast: Hsla,
    pub brand_ink: Hsla,
    pub chart_2: Hsla,

    // ── shadcn-style aliases (same mapping as the web app) ──
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
    let page = c(0x02080e);
    let canvas = c(0x040c12);
    let surface = c(0x060f16);
    let surface_2 = c(0x09131a);
    let surface_3 = c(0x101a21);
    let inset = c(0x172128);
    let hover_strong = c(0x202a32);
    let ink = c(0xe9f0f5);
    let ink_2 = c(0x85919a);
    let line = c(0x202a32);
    let brand = c(0x2ccceb);
    let brand_contrast = c(0x000408);
    let brand_ink = c(0x4bcdee);
    Theme {
        page,
        canvas,
        surface,
        surface_2,
        surface_3,
        inset,
        hover: inset,
        hover_strong,
        ink,
        ink_muted: c(0xc0c9cf),
        ink_2,
        ink_3: c(0x5d6870),
        ink_4: c(0x475159),
        line,
        line_strong: c(0x2e3942),
        line_tertiary: c(0x3c4851),
        brand,
        brand_contrast,
        brand_ink,
        chart_2: c(0x2fbc5b),

        // Pages sit on the content panel (`surface`), as in the web shell.
        background: surface,
        foreground: ink,
        card: surface_2,
        popover: surface_3,
        primary: brand,
        primary_foreground: brand_contrast,
        secondary: inset,
        muted: inset,
        muted_foreground: ink_2,
        accent: brand_ink,
        accent_foreground: brand_contrast,
        destructive: c(0xf22a36),
        destructive_foreground: c(0xf3faff),
        success: c(0x53be70),
        warning: c(0xebbc63),
        border: line,
        input: line,
        ring: brand,
        sand: c(0xe2cc95),
        bitcoin: c(0xf7941d),
    }
}

/// Tailwind-style `color/NN` alpha helper.
pub fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla { a, ..color }
}

/// Body / UI font (web: Manrope).
pub const FONT_SANS: &str = "Manrope";
/// Numbers, addresses, eyebrows (web: Space Mono).
pub const FONT_MONO: &str = "Space Mono";

// Radius scale (web: --radius-chip / control / card / panel).
pub const RADIUS_CHIP: Pixels = px(8.);
pub const RADIUS_CONTROL: Pixels = px(10.);
pub const RADIUS_CARD: Pixels = px(12.);
pub const RADIUS_PANEL: Pixels = px(16.);

/// Register the bundled fonts (OFL, see assets/fonts/OFL-*.txt).
pub fn load_fonts(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Manrope-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Manrope-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Manrope-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Manrope-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SpaceMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SpaceMono-Bold.ttf")),
    ];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        log::error!("failed to load bundled fonts: {e}");
    }
}
