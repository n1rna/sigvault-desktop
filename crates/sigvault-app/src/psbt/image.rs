//! QR decoding from image files — the stand-in for the webcam scanner.
//!
//! GPUI exposes no camera API, so instead of streaming frames from
//! `getUserMedia` into jsQR, the user picks one or more screenshots /
//! photos of the signer's QR display. Each image may contain several QR
//! codes; every decoded string is returned for the assembler.

use std::path::Path;

/// Decode every QR code found in the image at `path`.
pub fn decode_qr_image(path: &Path) -> Result<Vec<String>, String> {
    let img = ::image::open(path)
        .map_err(|e| format!("Couldn't open {}: {e}", file_name(path)))?
        .to_luma8();
    let (w, h) = img.dimensions();
    let mut found = scan(w as usize, h as usize, |x, y| {
        img.get_pixel(x as u32, y as u32).0[0]
    });
    if found.is_empty() {
        // Light-on-dark QR (e.g. dark-mode screenshots): retry inverted.
        found = scan(w as usize, h as usize, |x, y| {
            255 - img.get_pixel(x as u32, y as u32).0[0]
        });
    }
    if found.is_empty() {
        return Err(format!("No QR code found in {}", file_name(path)));
    }
    Ok(found)
}

fn scan(w: usize, h: usize, px: impl FnMut(usize, usize) -> u8) -> Vec<String> {
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, px);
    prepared
        .detect_grids()
        .into_iter()
        .filter_map(|g| g.decode().ok().map(|(_, content)| content))
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psbt::bytes_to_base64;
    use crate::psbt::qr::{psbt_to_ur_frames, tests::fake_psbt, PsbtQrAssembler};

    /// Render `data` as a QR PNG (4px modules, 4-module quiet zone).
    fn write_qr_png(data: &str, path: &Path, invert: bool) {
        let code = qrcode::QrCode::new(data.as_bytes()).unwrap();
        let width = code.width();
        let colors = code.to_colors();
        let scale = 4u32;
        let quiet = 4u32;
        let side = (width as u32 + quiet * 2) * scale;
        let (dark, light) = if invert { (255u8, 0u8) } else { (0u8, 255u8) };
        let img = ::image::GrayImage::from_fn(side, side, |x, y| {
            let (mx, my) = (x / scale, y / scale);
            if mx < quiet || my < quiet || mx >= quiet + width as u32 || my >= quiet + width as u32
            {
                return ::image::Luma([light]);
            }
            let i = ((my - quiet) as usize) * width + (mx - quiet) as usize;
            ::image::Luma([if colors[i] == qrcode::Color::Dark {
                dark
            } else {
                light
            }])
        });
        img.save(path).unwrap();
    }

    #[test]
    fn decodes_ur_sequence_from_png_files() {
        let b64 = bytes_to_base64(&fake_psbt(700));
        let frames = psbt_to_ur_frames(&b64).unwrap();
        let mut asm = PsbtQrAssembler::new();
        let mut result = None;
        for (i, f) in frames.frames.iter().enumerate() {
            let p = std::env::temp_dir()
                .join(format!("sigvault-qr-test-{}-{i}.png", std::process::id()));
            write_qr_png(&f.to_uppercase(), &p, i % 2 == 1);
            let decoded = decode_qr_image(&p).unwrap();
            std::fs::remove_file(&p).ok();
            assert_eq!(decoded, vec![f.to_uppercase()]);
            if let Some(r) = asm.push(&decoded[0]).unwrap() {
                result = Some(r);
                break;
            }
        }
        assert_eq!(result.unwrap(), b64);
    }

    #[test]
    fn missing_qr_is_an_error() {
        let p = std::env::temp_dir().join(format!("sigvault-qr-blank-{}.png", std::process::id()));
        ::image::GrayImage::from_pixel(64, 64, ::image::Luma([255]))
            .save(&p)
            .unwrap();
        assert!(decode_qr_image(&p).unwrap_err().contains("No QR code"));
        std::fs::remove_file(p).ok();
    }
}
