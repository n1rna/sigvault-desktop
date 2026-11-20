//! `setup/keys.ts` — client-side helpers for descriptor key expressions.
//! The backend does the authoritative parsing; these only give instant
//! feedback in the UI.

use super::api::SlotKey;

const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn base58_decode(s: &str) -> Option<Vec<u8>> {
    // Big-endian base-256 accumulator.
    let mut bytes: Vec<u8> = Vec::new();
    for c in s.bytes() {
        let mut carry = B58.iter().position(|&b| b == c)? as u32;
        for b in bytes.iter_mut().rev() {
            carry += (*b as u32) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    // Strip leading zero bytes produced by the accumulator, then add one
    // per leading '1'.
    let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    let mut out: Vec<u8> = s.bytes().take_while(|&c| c == b'1').map(|_| 0).collect();
    out.extend_from_slice(&bytes[first..]);
    Some(out)
}

/// Extended-key bytes without the 4-byte version prefix or checksum, so
/// the same key compares equal whether it's printed as xpub, tpub, Zpub…
fn key_body(extended: &str) -> Option<Vec<u8>> {
    let raw = base58_decode(extended.trim())?;
    if raw.len() != 82 {
        return None;
    }
    Some(raw[4..78].to_vec())
}

/// BIP32 depth byte of an extended key (0 = master key), or `None` when
/// the string isn't a well-formed extended key.
pub fn extended_key_depth(extended: &str) -> Option<u8> {
    let raw = base58_decode(extended.trim())?;
    (raw.len() == 82).then(|| raw[4])
}

/// True for extended keys encoded for a test network (`tpub`, `tprv`,
/// `upub`, `vpub`, …). Mainnet encodings start with x/y/z/Y/Z.
pub fn is_test_network_key(extended: &str) -> bool {
    extended.trim().starts_with(['t', 'u', 'v', 'U', 'V'])
}

#[cfg(test)]
fn same_extended_key(a: &str, b: &str) -> bool {
    match (key_body(a), key_body(b)) {
        (Some(ka), Some(kb)) => ka == kb,
        _ => false,
    }
}

pub fn normalize_path(path: &str) -> String {
    let p = path.trim().replace(['h', 'H'], "'");
    let p = if p == "m" {
        ""
    } else if let Some(rest) = p.strip_prefix("m/") {
        rest
    } else {
        p.as_str()
    };
    let p = p.trim_start_matches('/');
    if p.is_empty() {
        "m".into()
    } else {
        format!("m/{p}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PastedKey {
    Public(SlotKey),
    Private(String),
    Invalid(String),
}

/// Parse `(/[0-9]+['hH]?)*` — returns false if `s` doesn't match.
fn is_origin_path(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let Some(rest) = s.strip_prefix('/') else {
        return false;
    };
    rest.split('/').all(|c| {
        let digits = c.trim_end_matches(['\'', 'h', 'H']);
        let suffix_len = c.len() - digits.len();
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) && suffix_len <= 1
    })
}

/// Classify whatever the user pasted into a slot's "paste key" box.
pub fn classify_pasted_key(raw: &str) -> PastedKey {
    let text = raw.trim();
    if text.is_empty() {
        return PastedKey::Invalid(String::new());
    }
    let generic = || {
        PastedKey::Invalid("Expected a key like [fingerprint/48'/1'/0'/2']tpub… or an xprv.".into())
    };

    // Optional `[fingerprint/path]` origin.
    let (fp, path, rest) = if let Some(after) = text.strip_prefix('[') {
        let Some(close) = after.find(']') else {
            return generic();
        };
        let origin = &after[..close];
        if origin.len() < 8 || !origin.is_char_boundary(8) {
            return generic();
        }
        let (fp, path) = origin.split_at(8);
        if !fp.bytes().all(|b| b.is_ascii_hexdigit()) || !is_origin_path(path) {
            return generic();
        }
        (Some(fp), path, &after[close + 1..])
    } else {
        (None, "", text)
    };

    let key_len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    let (key, suffix) = rest.split_at(key_len);
    if key.is_empty() || !matches!(suffix, "" | "/<0;1>/*" | "/0/*" | "/1/*") {
        return generic();
    }

    let prefix = key.get(1..4).unwrap_or("");
    if prefix == "prv" {
        return PastedKey::Private(text.to_string());
    }
    if prefix != "pub" {
        return PastedKey::Invalid("That doesn't look like an extended key.".into());
    }
    let Some(fp) = fp else {
        return PastedKey::Invalid("Include the key origin, e.g. [fingerprint/48'/1'/0'/2']tpub…. Most wallets export keys this way; without it, signers can't recognise their key.".into());
    };
    if key_body(key).is_none() {
        return PastedKey::Invalid("That extended key is malformed.".into());
    }
    PastedKey::Public(SlotKey {
        fingerprint: fp.to_lowercase(),
        derivation_path: normalize_path(if path.is_empty() { "m" } else { path }),
        xpub: key.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // BIP32 test vector 1 master key, printed for mainnet and testnet.
    const XPUB: &str = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
    const TPUB: &str = "tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp";

    #[test]
    fn compares_extended_keys_across_network_encodings() {
        assert!(same_extended_key(XPUB, TPUB));
        // Swapping a character mid-key changes the key bytes themselves.
        let c = TPUB.as_bytes()[40];
        let other = format!(
            "{}{}{}",
            &TPUB[..40],
            if c == b'a' { 'b' } else { 'a' },
            &TPUB[41..]
        );
        assert!(!same_extended_key(XPUB, &other));
    }

    #[test]
    fn reads_depth_and_network() {
        assert_eq!(extended_key_depth(TPUB), Some(0));
        assert_eq!(extended_key_depth("tpubnope"), None);
        assert!(is_test_network_key(TPUB));
        assert!(!is_test_network_key(XPUB));
    }

    #[test]
    fn classifies_pasted_keys() {
        assert_eq!(
            classify_pasted_key(&format!("[D34DB33F/48h/1h/0h/2h]{TPUB}/<0;1>/*")),
            PastedKey::Public(SlotKey {
                fingerprint: "d34db33f".into(),
                derivation_path: "m/48'/1'/0'/2'".into(),
                xpub: TPUB.into(),
            })
        );
        assert!(matches!(classify_pasted_key(TPUB), PastedKey::Invalid(_)));
        assert!(matches!(
            classify_pasted_key("tprv8ZgxMBicQKsPd"),
            PastedKey::Private(_)
        ));
        assert_eq!(classify_pasted_key("  "), PastedKey::Invalid(String::new()));
        assert!(matches!(
            classify_pasted_key("[zzzz/1]tpub"),
            PastedKey::Invalid(_)
        ));
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize_path("m/48h/1H/0'/2'"), "m/48'/1'/0'/2'");
        assert_eq!(normalize_path("/84'/1'"), "m/84'/1'");
        assert_eq!(normalize_path("m"), "m");
        assert_eq!(normalize_path(""), "m");
    }
}
