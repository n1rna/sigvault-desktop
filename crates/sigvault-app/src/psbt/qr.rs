//! Animated QR PSBT codecs (port of `lib/psbtQr.ts`).
//!
//! Two on-the-wire formats are widely used by air-gapped Bitcoin signers:
//!
//! - **BBQr** — Coldcard / SeedSigner / Krux. Self-describing header,
//!   fixed part count. Format: `B$<encoding><type><total><idx><data>`,
//!   file type `P` for PSBTs.
//! - **UR** (BCR-2020-005) — Sparrow / Foundation Passport / Specter /
//!   Keystone. Fountain-coded, scan order doesn't matter. Format:
//!   `ur:crypto-psbt/...`, payload is the PSBT wrapped in a CBOR byte
//!   string (BCR-2020-006), exactly as `@ngraveio/bc-ur`'s
//!   `UR.fromBuffer` produced.
//!
//! On the decode side the assembler accepts ANY frame and dispatches by
//! prefix sniffing; both formats are unambiguous in their leading bytes.

use std::collections::BTreeMap;

use bbqr::{
    file_type::FileType,
    join::Joined,
    qr::Version,
    split::{Split, SplitOptions},
};

use super::{base64_to_bytes, bytes_to_base64, file::decode_psbt_text, is_psbt};

pub const UR_TYPE: &str = "crypto-psbt";
/// Same max fragment length the TS `UREncoder(wrapped, 200)` used.
const UR_MAX_FRAGMENT_LEN: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrFormat {
    Bbqr,
    Ur,
}

#[derive(Debug, Clone)]
pub struct PsbtQrFrames {
    pub frames: Vec<String>,
    pub total_parts: usize,
}

/// Encode a base64 PSBT into BBQr frames. Zlib-compressed (the bbqr
/// encoder falls back to Base32 when compression doesn't help); the same
/// split defaults as the JS `bbqr` package (`minVersion = 5`,
/// `maxVersion = 40`, 1..1295 parts).
pub fn psbt_to_bbqr_frames(psbt_base64: &str) -> Result<PsbtQrFrames, String> {
    let raw = base64_to_bytes(psbt_base64)?;
    let split = Split::try_from_data(
        &raw,
        FileType::Psbt,
        SplitOptions {
            min_version: Version::V05,
            ..Default::default()
        },
    )
    .map_err(|e| format!("BBQr encode failed: {e}"))?;
    let total_parts = split.parts.len();
    Ok(PsbtQrFrames {
        frames: split.parts,
        total_parts,
    })
}

/// Encode a base64 PSBT into UR (`crypto-psbt`) frames with a fountain
/// encoder. Emits roughly `2 * partCount` frames so a scanner that misses
/// some still has redundancy to reassemble. A PSBT that fits in one
/// fragment is emitted as a single-part UR (`ur:crypto-psbt/<body>`), the
/// same as bc-ur's `UREncoder.nextPart()`.
pub fn psbt_to_ur_frames(psbt_base64: &str) -> Result<PsbtQrFrames, String> {
    let raw = base64_to_bytes(psbt_base64)?;
    if raw.is_empty() {
        return Err("empty PSBT".into());
    }
    let cbor = cbor_wrap_bytes(&raw);
    let mut encoder = ur::Encoder::new(&cbor, UR_MAX_FRAGMENT_LEN, UR_TYPE)
        .map_err(|e| format!("UR encode failed: {e}"))?;
    let base_frames = encoder.fragment_count();
    let total_frames = (base_frames * 2).max(base_frames + 2);
    let frames = if base_frames == 1 {
        let single = ur::encode(&cbor, &ur::Type::Custom(UR_TYPE));
        vec![single; total_frames]
    } else {
        (0..total_frames)
            .map(|_| {
                encoder
                    .next_part()
                    .map_err(|e| format!("UR encode failed: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(PsbtQrFrames {
        frames,
        total_parts: base_frames,
    })
}

/// Encode as `format`.
pub fn psbt_to_frames(psbt_base64: &str, format: QrFormat) -> Result<PsbtQrFrames, String> {
    match format {
        QrFormat::Bbqr => psbt_to_bbqr_frames(psbt_base64),
        QrFormat::Ur => psbt_to_ur_frames(psbt_base64),
    }
}

// ── minimal CBOR byte-string framing (major type 2) ──

fn cbor_wrap_bytes(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut out = Vec::with_capacity(len + 9);
    if len < 24 {
        out.push(0x40 | len as u8);
    } else if len <= u8::MAX as usize {
        out.extend([0x58, len as u8]);
    } else if len <= u16::MAX as usize {
        out.push(0x59);
        out.extend((len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        out.push(0x5a);
        out.extend((len as u32).to_be_bytes());
    } else {
        out.push(0x5b);
        out.extend((len as u64).to_be_bytes());
    }
    out.extend_from_slice(data);
    out
}

/// Read a CBOR argument (`additional info` + following bytes) at `pos`.
fn cbor_arg(buf: &[u8], pos: &mut usize) -> Result<u64, String> {
    let err = || "truncated CBOR".to_string();
    let ib = *buf.get(*pos).ok_or_else(err)?;
    *pos += 1;
    let info = ib & 0x1f;
    let n = match info {
        0..=23 => return Ok(info as u64),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => return Err("unsupported CBOR length encoding".into()),
    };
    let bytes = buf.get(*pos..*pos + n).ok_or_else(err)?;
    *pos += n;
    Ok(bytes.iter().fold(0u64, |acc, b| (acc << 8) | *b as u64))
}

/// Unwrap a CBOR byte string, skipping any leading semantic tags (some
/// encoders emit `tag(310)` / `tag(40310)` around crypto-psbt).
fn cbor_unwrap_bytes(buf: &[u8]) -> Result<Vec<u8>, String> {
    let mut pos = 0;
    loop {
        let ib = *buf.get(pos).ok_or("empty CBOR")?;
        match ib >> 5 {
            6 => {
                cbor_arg(buf, &mut pos)?;
            }
            2 => {
                let len = cbor_arg(buf, &mut pos)? as usize;
                let body = buf
                    .get(pos..pos.checked_add(len).ok_or("bad CBOR length")?)
                    .ok_or("truncated CBOR byte string")?;
                return Ok(body.to_vec());
            }
            _ => return Err("UR payload is not a CBOR byte string".into()),
        }
    }
}

/// Streaming reassembler for incoming QR frames (`PsbtQrAssembler`).
/// Sniffs the format from the first frame; mixing BBQr and UR frames in
/// one session is an error. `push` returns the reassembled base64 PSBT
/// once complete.
#[derive(Default)]
pub struct PsbtQrAssembler {
    format: Option<QrFormat>,
    bbqr_parts: BTreeMap<usize, String>,
    bbqr_expected: usize,
    ur_decoder: Option<ur::Decoder>,
}

impl PsbtQrAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// (received, expected) parts so far.
    pub fn progress(&self) -> (usize, usize) {
        match self.format {
            Some(QrFormat::Bbqr) => (self.bbqr_parts.len(), self.bbqr_expected),
            Some(QrFormat::Ur) => self
                .ur_decoder
                .as_ref()
                .map(|d| (d.resolved_fragment_count().unwrap_or(0), d.fragment_count()))
                .unwrap_or((0, 0)),
            None => (0, 0),
        }
    }

    /// Push one decoded QR string. Returns `Ok(Some(base64))` when the
    /// PSBT is complete, `Ok(None)` while still collecting.
    pub fn push(&mut self, frame: &str) -> Result<Option<String>, String> {
        let frame = frame.trim();
        let detected = if frame.len() >= 3 && frame[..3].eq_ignore_ascii_case("ur:") {
            QrFormat::Ur
        } else {
            QrFormat::Bbqr
        };
        if let Some(fmt) = self.format {
            if fmt != detected {
                return Err(format!(
                    "mixed QR formats in one scan (was {}, got {})",
                    format_name(fmt),
                    format_name(detected)
                ));
            }
        }
        self.format = Some(detected);
        match detected {
            QrFormat::Ur => self.push_ur(frame),
            QrFormat::Bbqr => self.push_bbqr(frame),
        }
    }

    fn push_bbqr(&mut self, frame: &str) -> Result<Option<String>, String> {
        // Header: B$ <enc> <type> <total:2 base36> <idx:2 base36> <data>
        if !frame.starts_with("B$") || frame.len() < 8 {
            return Ok(None);
        }
        let (Ok(total), Ok(idx)) = (
            usize::from_str_radix(&frame[4..6], 36),
            usize::from_str_radix(&frame[6..8], 36),
        ) else {
            return Ok(None);
        };
        self.bbqr_expected = total;
        self.bbqr_parts.insert(idx, frame.to_string());
        if self.bbqr_parts.len() < total {
            return Ok(None);
        }
        let mut ordered = Vec::with_capacity(total);
        for i in 0..total {
            match self.bbqr_parts.get(&i) {
                Some(p) => ordered.push(p.clone()),
                None => return Ok(None),
            }
        }
        let joined =
            Joined::try_from_parts(ordered).map_err(|e| format!("BBQr join failed: {e}"))?;
        let data = if joined.file_type == FileType::UnicodeText && !is_psbt(&joined.data) {
            // Some tools send the PSBT as base64/hex text instead of 'P'.
            String::from_utf8(joined.data)
                .ok()
                .and_then(|s| decode_psbt_text(&s))
                .ok_or("BBQr payload is not a PSBT")?
        } else {
            joined.data
        };
        finish(data)
    }

    fn push_ur(&mut self, frame: &str) -> Result<Option<String>, String> {
        // Single-part UR: the whole payload in one frame.
        if frame.matches('/').count() == 1 {
            let (_, cbor) = ur::decode(frame).map_err(|e| format!("UR decode failed: {e}"))?;
            return finish(cbor_unwrap_bytes(&cbor)?);
        }
        let decoder = self.ur_decoder.get_or_insert_with(ur::Decoder::default);
        decoder
            .receive(frame)
            .map_err(|e| format!("UR decode failed: {e}"))?;
        if !decoder.complete() {
            return Ok(None);
        }
        let cbor = decoder
            .message()
            .map_err(|e| format!("UR decode failed: {e}"))?
            .ok_or("UR decode failed: incomplete message")?;
        finish(cbor_unwrap_bytes(&cbor)?)
    }
}

fn finish(data: Vec<u8>) -> Result<Option<String>, String> {
    if !is_psbt(&data) {
        return Err("Decoded QR data is not a PSBT".into());
    }
    Ok(Some(bytes_to_base64(&data)))
}

fn format_name(f: QrFormat) -> &'static str {
    match f {
        QrFormat::Bbqr => "bbqr",
        QrFormat::Ur => "ur",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A syntactically PSBT-looking blob of `n` bytes (magic + filler).
    pub(crate) fn fake_psbt(n: usize) -> Vec<u8> {
        let mut v = super::super::PSBT_MAGIC.to_vec();
        let mut x: u32 = 0x1234_5678;
        while v.len() < n {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            v.push(x as u8);
        }
        v
    }

    #[test]
    fn cbor_wrap_roundtrip_all_length_classes() {
        for n in [0usize, 5, 23, 24, 255, 256, 65535, 65536] {
            let data = vec![7u8; n];
            let w = cbor_wrap_bytes(&data);
            assert_eq!(cbor_unwrap_bytes(&w).unwrap(), data, "len {n}");
        }
        assert_eq!(cbor_wrap_bytes(&[1, 2, 3])[0], 0x43);
        assert_eq!(&cbor_wrap_bytes(&[0; 300])[..3], &[0x59, 0x01, 0x2c]);
    }

    #[test]
    fn cbor_unwrap_skips_tags() {
        // tag(310) = 0xd9 0x01 0x36
        let mut tagged = vec![0xd9, 0x01, 0x36];
        tagged.extend(cbor_wrap_bytes(b"psbt\xffabc"));
        assert_eq!(cbor_unwrap_bytes(&tagged).unwrap(), b"psbt\xffabc");
    }

    #[test]
    fn ur_multipart_roundtrip_any_order() {
        let raw = fake_psbt(1500);
        let b64 = bytes_to_base64(&raw);
        let out = psbt_to_ur_frames(&b64).unwrap();
        assert!(out.total_parts > 1);
        assert_eq!(out.frames.len(), out.total_parts * 2);
        assert!(out.frames[0].starts_with("ur:crypto-psbt/1-"));

        // Feed in reverse, uppercased (QR alphanumeric mode), skipping some.
        let mut asm = PsbtQrAssembler::new();
        let mut result = None;
        for (i, f) in out.frames.iter().rev().enumerate() {
            if i % 3 == 1 {
                continue;
            }
            if let Some(r) = asm.push(&f.to_uppercase()).unwrap() {
                result = Some(r);
                break;
            }
        }
        assert_eq!(result.unwrap(), b64);
    }

    #[test]
    fn ur_single_part_matches_bc_ur_shape() {
        let raw = fake_psbt(60);
        let b64 = bytes_to_base64(&raw);
        let out = psbt_to_ur_frames(&b64).unwrap();
        assert_eq!(out.total_parts, 1);
        assert_eq!(out.frames.len(), 3);
        let f = &out.frames[0];
        assert!(f.starts_with("ur:crypto-psbt/"));
        assert_eq!(f.matches('/').count(), 1);
        // Payload is CBOR bstr(len=60) = 0x58 0x3c ...
        let (_, cbor) = ur::decode(f).unwrap();
        assert_eq!(&cbor[..2], &[0x58, 60]);
        let mut asm = PsbtQrAssembler::new();
        assert_eq!(asm.push(f).unwrap().unwrap(), b64);
    }

    #[test]
    fn bbqr_roundtrip_out_of_order() {
        let raw = fake_psbt(6000);
        let b64 = bytes_to_base64(&raw);
        let out = psbt_to_bbqr_frames(&b64).unwrap();
        assert_eq!(out.frames.len(), out.total_parts);
        for f in &out.frames {
            assert!(f.starts_with("B$"));
            assert_eq!(&f[3..4], "P", "file type must be PSBT");
        }
        let mut asm = PsbtQrAssembler::new();
        let mut result = None;
        for f in out.frames.iter().rev() {
            result = asm.push(f).unwrap();
        }
        assert_eq!(result.unwrap(), b64);
        assert_eq!(asm.progress(), (out.total_parts, out.total_parts));
    }

    #[test]
    fn bbqr_multi_part_with_forced_split() {
        let raw = fake_psbt(3000);
        let split = Split::try_from_data(
            &raw,
            FileType::Psbt,
            SplitOptions {
                min_split_number: 4,
                min_version: Version::V05,
                max_version: Version::V10,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(split.parts.len() >= 4);
        let mut asm = PsbtQrAssembler::new();
        let mut result = None;
        for (i, p) in split.parts.iter().enumerate() {
            result = asm.push(p).unwrap();
            if i + 1 < split.parts.len() {
                assert!(result.is_none());
                assert_eq!(asm.progress(), (i + 1, split.parts.len()));
            }
        }
        assert_eq!(base64_to_bytes(&result.unwrap()).unwrap(), raw);
    }

    #[test]
    fn mixed_formats_rejected() {
        let b64 = bytes_to_base64(&fake_psbt(1500));
        let ur = psbt_to_ur_frames(&b64).unwrap();
        let bb = psbt_to_bbqr_frames(&b64).unwrap();
        let mut asm = PsbtQrAssembler::new();
        asm.push(&ur.frames[0]).unwrap();
        assert!(asm
            .push(&bb.frames[0])
            .unwrap_err()
            .contains("mixed QR formats"));
    }

    #[test]
    fn non_psbt_payload_rejected() {
        let ur = ur::encode(&cbor_wrap_bytes(b"hello world"), &ur::Type::Custom(UR_TYPE));
        assert!(PsbtQrAssembler::new().push(&ur).is_err());
    }
}
