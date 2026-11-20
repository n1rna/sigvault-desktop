//! Where a library key comes from, plus the small helpers the add flow,
//! the list and the detail page share (icons, labels, derivation defaults,
//! secret conversion).

use gpui::{div, prelude::*, px, ClipboardItem, ElementId, Hsla, SharedString};

use crate::e2e::{E2eClick, Probe};
use crate::keychain::KeyRecord;
use crate::pages::local::setup::api::{self, SlotKey};
use crate::pages::local::setup::keys::{extended_key_depth, normalize_path};
use crate::theme::{alpha, theme, FONT_MONO};
use crate::ui::icon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    Generate,
    Seed,
    Xprv,
    KeyFile,
    Hardware,
    Xpub,
}

impl KeySource {
    /// Sources that create a brand-new key.
    pub const CREATE: [KeySource; 1] = [KeySource::Generate];
    /// Imports whose private key ends up (encrypted) on this computer.
    pub const IMPORT_PRIVATE: [KeySource; 3] =
        [KeySource::Seed, KeySource::Xprv, KeySource::KeyFile];
    /// Imports of a public key only: signing happens elsewhere.
    pub const IMPORT_PUBLIC: [KeySource; 2] = [KeySource::Hardware, KeySource::Xpub];

    /// The keychain's `source` string (see `keychain::SOURCES`).
    pub fn id(self) -> &'static str {
        match self {
            KeySource::Generate => "generated",
            KeySource::Seed => "seed",
            KeySource::Xprv => "xprv",
            KeySource::KeyFile => "key_file",
            KeySource::Hardware => "hardware",
            KeySource::Xpub => "xpub",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            KeySource::Generate => "Generate a new key",
            KeySource::Seed => "Recovery phrase",
            KeySource::Xprv => "Extended private key",
            KeySource::KeyFile => "SigVault key file",
            KeySource::Hardware => "Hardware wallet",
            KeySource::Xpub => "Extended public key",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            KeySource::Generate => {
                "A fresh 24-word recovery phrase made on this computer. You'll write it down and confirm it before it's saved."
            }
            KeySource::Seed => "Type or paste an existing 12–24 word BIP39 phrase.",
            KeySource::Xprv => "Paste an xprv or tprv. Account-level keys need their fingerprint and path origin.",
            KeySource::KeyFile => "Open an encrypted .sigvault-key backup exported from SigVault.",
            KeySource::Hardware => "Ledger, Trezor, BitBox, Jade or Coldcard. We read its public key; the private key stays on the device.",
            KeySource::Xpub => "Paste a co-signer's [fingerprint/path]xpub. Signing happens wherever their private key lives.",
        }
    }

    pub fn icon(self) -> &'static str {
        source_icon(self.id())
    }

    /// Only the public half is stored: the key is watch-only here.
    pub fn is_public(self) -> bool {
        matches!(self, KeySource::Hardware | KeySource::Xpub)
    }

    pub fn from_id(id: &str) -> Option<KeySource> {
        [
            KeySource::Generate,
            KeySource::Seed,
            KeySource::Xprv,
            KeySource::KeyFile,
            KeySource::Hardware,
            KeySource::Xpub,
        ]
        .into_iter()
        .find(|s| s.id() == id)
    }
}

/// Icon for a keychain `source` string.
pub fn source_icon(source: &str) -> &'static str {
    match source {
        "generated" => "sparkles",
        "seed" => "scroll-text",
        "xprv" => "key-round",
        "key_file" => "file-key",
        "hardware" => "hw-device",
        "xpub" => "eye",
        _ => "key",
    }
}

/// Square icon tile used in source cards and key rows.
pub fn icon_tile(name: &'static str, size: f32, color: Option<Hsla>) -> gpui::Div {
    let t = theme();
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(alpha(t.muted, 0.8))
        .border_1()
        .border_color(alpha(t.border, 0.7))
        .child(
            icon(name)
                .size(px(size * 0.5))
                .text_color(color.unwrap_or(t.foreground)),
        )
}

/// Supported local networks: (id, label).
pub const NETWORKS: [(&str, &str); 3] = [
    ("regtest", "Regtest"),
    ("signet", "Signet"),
    ("testnet4", "Testnet 4"),
];

pub fn network_label(id: &str) -> String {
    NETWORKS
        .iter()
        .find(|(n, _)| *n == id)
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| id.to_string())
}

/// Network dot colour, matching the sidebar.
pub fn network_color(network: &str) -> Hsla {
    let t = theme();
    match network {
        "bitcoin" | "mainnet" => t.bitcoin,
        "testnet" | "testnet4" => t.success,
        "signet" => t.warning,
        _ => t.accent,
    }
}

/// Small outlined network pill with a coloured dot.
pub fn network_badge(network: &str) -> gpui::Div {
    let t = theme();
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .border_1()
        .border_color(t.border)
        .px(px(8.))
        .py(px(2.))
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(t.muted_foreground)
        .child(div().size(px(6.)).rounded_full().bg(network_color(network)))
        .child(network_label(network))
}

/// Derivation-path presets for keys made from a seed / master xprv.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathPreset {
    /// BIP48 multisig (P2WSH) — what multisig and timelocked wallets use.
    Multisig,
    /// BIP84 single-sig.
    Singlesig,
    Custom,
}

/// BIP44 coin index: mainnet 0', every test network 1'.
fn coin(network: &str) -> &'static str {
    if network == "bitcoin" {
        "0'"
    } else {
        "1'"
    }
}

impl PathPreset {
    pub fn path(self, network: &str) -> Option<String> {
        match self {
            PathPreset::Multisig => Some(format!("m/48'/{}/0'/2'", coin(network))),
            PathPreset::Singlesig => Some(format!("m/84'/{}/0'", coin(network))),
            PathPreset::Custom => None,
        }
    }

    pub fn detect(path: &str, network: &str) -> PathPreset {
        let p = normalize_path(path);
        [PathPreset::Multisig, PathPreset::Singlesig]
            .into_iter()
            .find(|preset| preset.path(network).as_deref() == Some(p.as_str()))
            .unwrap_or(PathPreset::Custom)
    }
}

/// Default library path: BIP48 multisig, since library keys are mostly
/// combined into multisig / timelocked wallets.
pub fn default_path(network: &str) -> String {
    PathPreset::Multisig.path(network).unwrap_or_default()
}

/// The setup module mirrors the core `HotSecret` with an identical wire
/// shape; convert through serde.
pub fn to_core_secret(
    s: &api::HotSecret,
) -> Result<sigvault_core::local_wallet::storage::HotSecret, String> {
    serde_json::to_value(s)
        .and_then(serde_json::from_value)
        .map_err(|e| e.to_string())
}

pub fn from_core_secret(
    s: &sigvault_core::local_wallet::storage::HotSecret,
) -> Result<api::HotSecret, String> {
    serde_json::to_value(s)
        .and_then(serde_json::from_value)
        .map_err(|e| e.to_string())
}

/// An account-level xprv only makes sense with its origin. Store it as
/// `[fp/path]xprv` so the library record is self-contained.
pub fn with_origin(secret: api::HotSecret, key: &SlotKey) -> api::HotSecret {
    match secret {
        api::HotSecret::Xprv { xprv } if !xprv.trim_start().starts_with('[') => {
            let bare = xprv.trim();
            if extended_key_depth(bare).is_some_and(|d| d > 0) {
                let path = normalize_path(&key.derivation_path);
                let path = path.trim_start_matches('m');
                api::HotSecret::Xprv {
                    xprv: format!("[{}{}]{}", key.fingerprint.to_lowercase(), path, bare),
                }
            } else {
                api::HotSecret::Xprv {
                    xprv: bare.to_string(),
                }
            }
        }
        other => other,
    }
}

/// `[fp/path]xpub` for a key that isn't in the library yet.
pub fn key_expr(key: &SlotKey) -> String {
    let path = normalize_path(&key.derivation_path);
    let path = path.trim_start_matches('m');
    format!("[{}{}]{}", key.fingerprint.to_lowercase(), path, key.xpub)
}

/// One-line description of where a stored key lives.
pub fn custody_text(k: &KeyRecord) -> &'static str {
    if k.is_hot() {
        "Private key encrypted under your keychain password"
    } else if k.source == "hardware" {
        "Watch-only here: the private key stays on the hardware wallet"
    } else {
        "Watch-only here: signing happens wherever the private key lives"
    }
}

/// Read-only mono value with a copy button. Unlike `ui::copy_field`, long
/// unbroken strings (xpubs) wrap inside the box.
pub fn copy_box(id: impl Into<ElementId>, value: impl Into<SharedString>) -> gpui::Div {
    let t = theme();
    let value: SharedString = value.into();
    let v2 = value.clone();
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(t.border)
        .bg(alpha(t.background, 0.6))
        .pl_3()
        .pr_1()
        .py_1()
        .child(
            div()
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .py(px(5.))
                .flex()
                .flex_col()
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(t.foreground)
                .children(display_lines(&value).into_iter().map(|groups| {
                    // Wrap only between 4-character groups, never mid-group.
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_x(px(5.))
                        .children(groups.into_iter().map(|g| div().child(g)))
                })),
        )
        .child(
            div()
                .id(id)
                .flex_none()
                .p(px(6.))
                .rounded(px(4.))
                .cursor_pointer()
                .hover(move |s| s.bg(t.muted))
                .clickable(Probe::new("button", "Copy"), move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(v2.to_string()));
                    crate::shell::toast(cx, "Copied to clipboard");
                })
                .child(icon("copy").size(px(14.))),
        )
}

/// How [`copy_box`] lays a key out: a `[fingerprint/path]` origin on its
/// own line, then the key in 4-character groups (the copied text is
/// unchanged).
fn display_lines(value: &str) -> Vec<Vec<String>> {
    let (origin, rest) = match value.find(']') {
        Some(i) if value.starts_with('[') => (Some(&value[..=i]), &value[i + 1..]),
        _ => (None, value),
    };
    let chars: Vec<char> = rest.chars().collect();
    let groups: Vec<String> = chars.chunks(4).map(|c| c.iter().collect()).collect();
    origin
        .map(|o| vec![o.to_string()])
        .into_iter()
        .chain(std::iter::once(groups))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip() {
        assert_eq!(default_path("signet"), "m/48'/1'/0'/2'");
        assert_eq!(
            PathPreset::detect("m/84h/1h/0h", "regtest"),
            PathPreset::Singlesig
        );
        assert_eq!(
            PathPreset::detect("m/48'/1'/3'/2'", "regtest"),
            PathPreset::Custom
        );
        assert_eq!(KeySource::from_id("key_file"), Some(KeySource::KeyFile));
    }

    #[test]
    fn key_expr_strips_m() {
        let k = SlotKey {
            fingerprint: "AABBCCDD".into(),
            derivation_path: "m/84'/1'/0'".into(),
            xpub: "tpubX".into(),
        };
        assert_eq!(key_expr(&k), "[aabbccdd/84'/1'/0']tpubX");
    }
}
