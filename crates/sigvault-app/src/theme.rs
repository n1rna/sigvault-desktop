//! SigVault "Instrument" palette, ported from the web dashboard
//! (`sigvault/apps/dashboard/app/globals.css`, `:root` and `.dark` blocks).
//!
//! Structure follows the web app's Linear-style ladders:
//!   surfaces  page < canvas < surface < surface-2 < surface-3, inset/hover wells
//!   ink       ink > ink-muted > ink-2 > ink-3 > ink-4
//!   lines     line < line-strong < line-tertiary
//! Hierarchy comes from the surface ladder plus hairlines, not drop shadows.
//!
//! Text roles: `ink` for primary text, `ink_muted` for secondary body,
//! `ink_2` (= `muted_foreground`) for labels and metadata, `ink_3` for the
//! quietest text that still has to be read. `ink_4` is decorative/disabled
//! only and never carries text. `brand_ink` is the brand colour for text and
//! links; `brand` is for rings, dots and strokes; `brand_fill` is the primary
//! button fill under white (`brand_fill_contrast`) text. The same split holds
//! for `destructive` (text) / `destructive_fill` and `warning` (fills,
//! borders) / `warning_ink` (text).
//!
//! The shadcn-style names (`background`, `card`, `muted_foreground`, …) are kept
//! and mapped onto the ladder exactly as the web app's `--background`, `--card`,
//! … are.
//!
//! ## Light and dark
//!
//! [`theme()`] returns the active palette. Which one is active is resolved
//! from the user's [`AppearancePref`] (System / Light / Dark, stored in
//! `<data_dir>/ui-settings.json`, overridable with `SIGVAULT_THEME`) and, for
//! System, the window's appearance. The root view calls [`observe_window`]
//! so an OS appearance change re-resolves and redraws.

use std::borrow::Cow;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use gpui::{
    px, rgb, App, Context, Global, Hsla, Pixels, Rgba, Subscription, Window, WindowAppearance,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct Theme {
    pub is_dark: bool,
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
    /// Decorative / disabled only. Never text.
    pub ink_4: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub line_tertiary: Hsla,
    /// Boundary of form controls (text inputs, selects, unselected radios):
    /// ≥3:1 against every surface they sit on (WCAG 1.4.11).
    pub line_control: Hsla,
    /// Brand for rings, dots, strokes and large marks.
    pub brand: Hsla,
    /// Dark text on a `brand` fill.
    pub brand_contrast: Hsla,
    /// Brand for text and links (AA on every surface).
    pub brand_ink: Hsla,
    /// Primary button fill (white text on it).
    pub brand_fill: Hsla,
    pub brand_fill_contrast: Hsla,
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
    /// Destructive text and icons.
    pub destructive: Hsla,
    /// Destructive button fill (white text on it).
    pub destructive_fill: Hsla,
    pub destructive_foreground: Hsla,
    pub success: Hsla,
    /// Warning fills, borders and dots.
    pub warning: Hsla,
    /// Warning text.
    pub warning_ink: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    /// Decorative only.
    pub sand: Hsla,
    /// Decorative only (the logo's centre cell, mainnet marks).
    pub bitcoin: Hsla,
    /// Modal backdrop.
    pub scrim: Hsla,
}

fn c(hex: u32) -> Hsla {
    let rgba: Rgba = rgb(hex);
    rgba.into()
}

/// Raw hex values for one palette, in the order of the token plan.
struct Palette {
    page: u32,
    canvas: u32,
    surface: u32,
    surface_2: u32,
    surface_3: u32,
    inset: u32,
    hover_strong: u32,
    ink: u32,
    ink_muted: u32,
    ink_2: u32,
    ink_3: u32,
    ink_4: u32,
    line: u32,
    line_strong: u32,
    line_tertiary: u32,
    line_control: u32,
    brand: u32,
    brand_ink: u32,
    brand_fill: u32,
    brand_fill_contrast: u32,
    brand_contrast: u32,
    destructive: u32,
    destructive_fill: u32,
    destructive_foreground: u32,
    success: u32,
    warning: u32,
    warning_ink: u32,
    chart_2: u32,
    /// Modal backdrop: colour and opacity.
    scrim: (u32, f32),
}

const LIGHT: Palette = Palette {
    page: 0xf3faff,
    canvas: 0xebf5fc,
    surface: 0xfbfeff,
    surface_2: 0xffffff,
    surface_3: 0xffffff,
    inset: 0xe0edf8,
    hover_strong: 0xd5e3ee,
    ink: 0x09131a,
    ink_muted: 0x202a32,
    ink_2: 0x4c575f,
    ink_3: 0x5e6972,
    ink_4: 0x909aa1,
    line: 0xd3e0ea,
    line_strong: 0xc1cfdb,
    line_tertiary: 0xafbfcb,
    // 3.35:1 on white, 3.30:1 on surface, 3.18:1 on page.
    line_control: 0x838e96,
    brand: 0x0092c6,
    brand_ink: 0x0073a4,
    brand_fill: 0x0074a6,
    brand_fill_contrast: 0xf3faff,
    brand_contrast: 0x09131a,
    destructive: 0xdf202e,
    destructive_fill: 0xdf202e,
    destructive_foreground: 0xf3faff,
    success: 0x0a7e3a,
    warning: 0xe4a339,
    warning_ink: 0x8a5600,
    chart_2: 0x00a33d,
    // ink at 32%.
    scrim: (0x09131a, 0.32),
};

const DARK: Palette = Palette {
    page: 0x02080e,
    canvas: 0x040c12,
    surface: 0x060f16,
    surface_2: 0x09131a,
    surface_3: 0x101a21,
    inset: 0x172128,
    hover_strong: 0x202a32,
    ink: 0xe9f0f5,
    ink_muted: 0xc0c9cf,
    ink_2: 0x919da6,
    ink_3: 0x7f8b94,
    ink_4: 0x475159,
    line: 0x202a32,
    line_strong: 0x2e3942,
    line_tertiary: 0x3c4851,
    // 3.34:1 on card, 3.44:1 on surface, 3.58:1 on page.
    line_control: 0x5e6972,
    brand: 0x2ccceb,
    brand_ink: 0x4bcdee,
    brand_fill: 0x007ba5,
    brand_fill_contrast: 0xf3faff,
    brand_contrast: 0x000408,
    destructive: 0xf22a36,
    destructive_fill: 0xcc272e,
    destructive_foreground: 0xf3faff,
    success: 0x53be70,
    warning: 0xebbc63,
    warning_ink: 0xebbc63,
    chart_2: 0x2fbc5b,
    scrim: (0x000000, 0.6),
};

impl Theme {
    pub fn dark() -> Theme {
        Self::build(&DARK, true)
    }

    pub fn light() -> Theme {
        Self::build(&LIGHT, false)
    }

    /// The text-safe variant of a fill colour: `warning` → `warning_ink`,
    /// `brand` / `brand_fill` → `brand_ink`, `destructive_fill` →
    /// `destructive`; anything else is returned unchanged. Use it where a
    /// widget takes one colour for both its dot/tint and its label.
    pub fn ink_for(&self, color: Hsla) -> Hsla {
        if color == self.warning {
            self.warning_ink
        } else if color == self.brand || color == self.brand_fill {
            self.brand_ink
        } else if color == self.destructive_fill {
            self.destructive
        } else {
            color
        }
    }

    fn build(p: &Palette, is_dark: bool) -> Theme {
        let surface = c(p.surface);
        let inset = c(p.inset);
        let ink = c(p.ink);
        let ink_2 = c(p.ink_2);
        let line = c(p.line);
        let brand = c(p.brand);
        let brand_ink = c(p.brand_ink);
        let brand_fill = c(p.brand_fill);
        let brand_fill_contrast = c(p.brand_fill_contrast);
        let brand_contrast = c(p.brand_contrast);
        Theme {
            is_dark,
            page: c(p.page),
            canvas: c(p.canvas),
            surface,
            surface_2: c(p.surface_2),
            surface_3: c(p.surface_3),
            inset,
            hover: inset,
            hover_strong: c(p.hover_strong),
            ink,
            ink_muted: c(p.ink_muted),
            ink_2,
            ink_3: c(p.ink_3),
            ink_4: c(p.ink_4),
            line,
            line_strong: c(p.line_strong),
            line_tertiary: c(p.line_tertiary),
            line_control: c(p.line_control),
            brand,
            brand_contrast,
            brand_ink,
            brand_fill,
            brand_fill_contrast,
            chart_2: c(p.chart_2),

            // Pages sit on the content panel (`surface`), as in the web shell.
            background: surface,
            foreground: ink,
            card: c(p.surface_2),
            popover: c(p.surface_3),
            primary: brand_fill,
            primary_foreground: brand_fill_contrast,
            secondary: inset,
            muted: inset,
            muted_foreground: ink_2,
            accent: brand_ink,
            accent_foreground: brand_contrast,
            destructive: c(p.destructive),
            destructive_fill: c(p.destructive_fill),
            destructive_foreground: c(p.destructive_foreground),
            success: c(p.success),
            warning: c(p.warning),
            warning_ink: c(p.warning_ink),
            border: line,
            input: line,
            ring: brand,
            sand: c(0xe2cc95),
            bitcoin: c(0xf7941d),
            scrim: alpha(c(p.scrim.0), p.scrim.1),
        }
    }
}

// ─── Active theme ───────────────────────────────────────────────────────

static DARK_ACTIVE: AtomicBool = AtomicBool::new(true);

/// The active palette (light or dark, see the module docs).
pub fn theme() -> Theme {
    static DARK_THEME: OnceLock<Theme> = OnceLock::new();
    static LIGHT_THEME: OnceLock<Theme> = OnceLock::new();
    if DARK_ACTIVE.load(Ordering::Relaxed) {
        *DARK_THEME.get_or_init(Theme::dark)
    } else {
        *LIGHT_THEME.get_or_init(Theme::light)
    }
}

/// The user's choice in Settings → Appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppearancePref {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl AppearancePref {
    pub const ALL: [AppearancePref; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" | "auto" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    fn resolve_dark(self, os: WindowAppearance) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => matches!(os, WindowAppearance::Dark | WindowAppearance::VibrantDark),
        }
    }
}

/// App-level UI preferences, plain JSON next to the other non-secret
/// settings (`<data_dir>/ui-settings.json`, sibling of
/// `local-settings.json`). Unknown fields are kept out of the way by serde
/// defaults so older/newer builds can share the file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct UiSettings {
    #[serde(default)]
    appearance: AppearancePref,
}

const UI_SETTINGS_FILE: &str = "ui-settings.json";

struct Appearance {
    pref: AppearancePref,
    /// `SIGVAULT_THEME=light|dark|system` (screenshots, tests): wins over the
    /// stored preference for this run and is never written back.
    env_override: Option<AppearancePref>,
    path: Option<PathBuf>,
    os: WindowAppearance,
}

impl Global for Appearance {}

/// Load the stored preference. Call once at startup, before the window opens.
pub fn init(cx: &mut App) {
    let path = crate::runtime::backend(cx)
        .app
        .path()
        .app_data_dir()
        .ok()
        .map(|d| d.join(UI_SETTINGS_FILE));
    let pref = path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<UiSettings>(&b).ok())
        .map(|s| s.appearance)
        .unwrap_or_default();
    let env_override = std::env::var("SIGVAULT_THEME")
        .ok()
        .and_then(|v| AppearancePref::parse(&v));
    let os = cx.window_appearance();
    cx.set_global(Appearance {
        pref,
        env_override,
        path,
        os,
    });
    apply(cx);
}

fn apply(cx: &App) {
    let a = cx.global::<Appearance>();
    let dark = a.env_override.unwrap_or(a.pref).resolve_dark(a.os);
    DARK_ACTIVE.store(dark, Ordering::Relaxed);
}

/// Follow `window`'s appearance. Called by the root view; the returned
/// subscription must be kept alive.
pub fn observe_window<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Subscription {
    let os = window.appearance();
    cx.global_mut::<Appearance>().os = os;
    apply(cx);
    cx.observe_window_appearance(window, |_, window, cx| {
        cx.global_mut::<Appearance>().os = window.appearance();
        apply(cx);
        window.refresh();
    })
}

/// The preference in effect (what Settings shows): the launch override if
/// set, else the stored choice.
pub fn preference(cx: &App) -> AppearancePref {
    cx.try_global::<Appearance>()
        .map(|a| a.env_override.unwrap_or(a.pref))
        .unwrap_or_default()
}

/// Change and persist the preference, then redraw every window.
pub fn set_preference(pref: AppearancePref, cx: &mut App) {
    let a = cx.global_mut::<Appearance>();
    if a.pref == pref && a.env_override.is_none() {
        return;
    }
    a.pref = pref;
    // An explicit choice in Settings beats the launch-time override.
    a.env_override = None;
    if let Some(path) = a.path.clone() {
        let bytes = serde_json::to_vec_pretty(&UiSettings { appearance: pref }).unwrap_or_default();
        cx.background_executor()
            .spawn(async move {
                if let Err(e) = write_atomic(&path, &bytes) {
                    log::error!("write {}: {e}", path.display());
                }
            })
            .detach();
    }
    apply(cx);
    cx.refresh_windows();
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

/// Tailwind-style `color/NN` alpha helper.
pub fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla { a, ..color }
}

/// Body / UI font (web: Manrope).
pub const FONT_SANS: &str = "Manrope";
/// Real data only: addresses, txids, amounts, fees, fingerprints, xpubs,
/// descriptors, derivation paths, block heights, seed words (web: Space Mono).
/// Never labels, captions or headings.
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
