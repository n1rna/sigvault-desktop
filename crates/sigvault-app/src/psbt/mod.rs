//! PSBT transport helpers for the air-gapped signing flow (QBL-234).
//!
//! Ports `lib/psbtQr.ts` + `lib/psbtFile.ts` from the Tauri build:
//! - [`qr`]: animated-QR codecs — BBQr (Coldcard / SeedSigner / Krux) and
//!   UR `crypto-psbt` (Sparrow / Passport / Specter / Keystone) — plus a
//!   streaming reassembler that sniffs the format per frame.
//! - [`file`]: `.psbt` file round-trip (binary BIP-174 on disk, base64 in
//!   memory) and lenient text parsing for pasted PSBTs.
//! - [`image`]: QR decoding from image files. GPUI has no camera API, so
//!   this replaces the webcam scanner (`QrScanModal` + jsQR).
//!
//! The core carries PSBTs as base64 strings; everything here converts at
//! that boundary.

pub mod file;
pub mod image;
pub mod qr;

use base64::{engine::general_purpose::STANDARD as B64, Engine};

/// BIP-174 magic bytes: `psbt` + 0xff separator.
pub const PSBT_MAGIC: &[u8] = b"psbt\xff";

pub fn base64_to_bytes(b64: &str) -> Result<Vec<u8>, String> {
    B64.decode(b64.trim())
        .map_err(|e| format!("invalid base64 PSBT: {e}"))
}

pub fn bytes_to_base64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn is_psbt(bytes: &[u8]) -> bool {
    bytes.starts_with(PSBT_MAGIC)
}
