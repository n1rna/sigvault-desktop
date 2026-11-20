//! PSBT file round-trip (port of `lib/psbtFile.ts`).
//!
//! The standard `.psbt` on-disk format is binary (BIP-174 magic +
//! key-value records). Many tools also write base64 (or hex) text into a
//! `.psbt` / `.txt` file, so reading accepts all three.

use std::path::Path;

use super::{base64_to_bytes, bytes_to_base64, is_psbt};

/// Write a base64 PSBT to disk in binary BIP-174 form.
pub fn write_psbt_file(path: &Path, psbt_base64: &str) -> Result<(), String> {
    let bytes = base64_to_bytes(psbt_base64)?;
    std::fs::write(path, bytes).map_err(|e| format!("Failed to save PSBT: {e}"))
}

/// Read a `.psbt` file (binary, base64 or hex) and return it as base64.
pub fn read_psbt_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if is_psbt(&bytes) {
        return Ok(bytes_to_base64(&bytes));
    }
    std::str::from_utf8(&bytes)
        .ok()
        .and_then(decode_psbt_text)
        .map(|raw| bytes_to_base64(&raw))
        .ok_or_else(|| format!("{} is not a PSBT file", path.display()))
}

/// Decode a PSBT pasted/stored as base64 or hex text (whitespace ignored).
/// Returns the raw bytes only if they carry the PSBT magic.
pub fn decode_psbt_text(text: &str) -> Option<Vec<u8>> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }
    if let Ok(raw) = base64_to_bytes(&compact) {
        if is_psbt(&raw) {
            return Some(raw);
        }
    }
    if compact.len() % 2 == 0 && compact.chars().all(|c| c.is_ascii_hexdigit()) {
        let raw: Option<Vec<u8>> = (0..compact.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&compact[i..i + 2], 16).ok())
            .collect();
        if let Some(raw) = raw.filter(|r| is_psbt(r)) {
            return Some(raw);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psbt::qr::tests::fake_psbt;

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sigvault-psbt-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn binary_file_roundtrip() {
        let b64 = bytes_to_base64(&fake_psbt(500));
        let p = tmp("bin.psbt");
        write_psbt_file(&p, &b64).unwrap();
        assert!(is_psbt(&std::fs::read(&p).unwrap()));
        assert_eq!(read_psbt_file(&p).unwrap(), b64);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn base64_and_hex_text_files() {
        let raw = fake_psbt(200);
        let b64 = bytes_to_base64(&raw);
        let p = tmp("b64.psbt");
        std::fs::write(&p, format!("{}\n{}\n", &b64[..50], &b64[50..])).unwrap();
        assert_eq!(read_psbt_file(&p).unwrap(), b64);
        let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        std::fs::write(&p, hex).unwrap();
        assert_eq!(read_psbt_file(&p).unwrap(), b64);
        std::fs::write(&p, "not a psbt").unwrap();
        assert!(read_psbt_file(&p).is_err());
        std::fs::remove_file(p).ok();
    }
}
