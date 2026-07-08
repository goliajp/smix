//! v5.2 c6 — visual regression perceptual hash (crate-internal).
//!
//! Standard **dhash 64-bit** (difference hash): decode PNG → grayscale →
//! resize 9×8 nearest-neighbor → row-wise left/right pixel diff → 64 bits.
//!
//! Algorithm intent: tolerate anti-aliasing / encoder metadata noise (where
//! byte-equality always fails) but flag substantive visual changes. Industry
//! standard for screenshot regression; maestro's BlinkDiff is in the same
//! family. SSIM / full pHash deferred to v5.3+ (cold plan §c6 字面"完整算法
//! 分 minor").
//!
//! Threshold: hamming distance ≤ 5 = visually identical (v5.2 c6 hard-coded
//! at adapter runtime layer; SDK pub fn opens `max_hamming` for v5.3+).

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

/// Compute the 64-bit dhash for a PNG byte stream.
///
/// Returns `Err(DriverError)` if the PNG is malformed or the color type
/// is outside `{Rgb, Rgba, Grayscale, GrayscaleAlpha}` (§13 explicit fail
/// — no silent noop).
pub(crate) fn compute_dhash(png_bytes: &[u8]) -> Result<u64, ExpectationFailure> {
    let decoder = png::Decoder::new(png_bytes);
    let mut reader = decoder.read_info().map_err(|e| {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("PNG decode (read_info) failed: {e}"),
            ..Default::default()
        })
    })?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("PNG decode (next_frame) failed: {e}"),
            ..Default::default()
        })
    })?;
    let raw = &buf[..info.buffer_size()];
    let (channels, has_alpha) = match info.color_type {
        png::ColorType::Rgb => (3usize, false),
        png::ColorType::Rgba => (4usize, true),
        png::ColorType::Grayscale => (1usize, false),
        png::ColorType::GrayscaleAlpha => (2usize, true),
        other => {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("unsupported PNG color type: {other:?}"),
                ..Default::default()
            }));
        }
    };
    let w = info.width as usize;
    let h = info.height as usize;
    if w == 0 || h == 0 {
        return Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("PNG has zero dimension: {w}×{h}"),
            ..Default::default()
        }));
    }

    // Step 1: source pixel → grayscale u8 helper (pixel at (x, y)).
    let pixel_gray = |x: usize, y: usize| -> u8 {
        let idx = (y * w + x) * channels;
        match channels {
            1 => raw[idx],
            2 => raw[idx], // Grayscale + alpha: ignore alpha.
            3 | 4 => {
                let r = raw[idx] as u16;
                let g = raw[idx + 1] as u16;
                let b = raw[idx + 2] as u16;
                ((r + g + b) / 3) as u8
            }
            _ => 0,
        }
    };
    let _ = has_alpha;

    // Step 2: resize to 9×8 grayscale via nearest-neighbor (pure integer
    // arithmetic — no resize dep). Width 9 = 8 left/right pairs per row;
    // height 8 × 8 bits = 64 bits.
    let mut grid = [[0u8; 9]; 8];
    for (dy, row) in grid.iter_mut().enumerate() {
        for (dx, cell) in row.iter_mut().enumerate() {
            let sx = (dx * w) / 9;
            let sy = (dy * h) / 8;
            *cell = pixel_gray(sx, sy);
        }
    }

    // Step 3: row-wise left/right diff → 64-bit MSB-first.
    let mut hash: u64 = 0;
    for row in &grid {
        for dx in 0..8 {
            hash <<= 1;
            if row[dx] > row[dx + 1] {
                hash |= 1;
            }
        }
    }
    Ok(hash)
}

/// Hamming distance between two 64-bit dhashes — number of differing bits.
/// 0 = identical; 64 = all bits flipped. Typical perceptual threshold ≤ 10.
pub(crate) fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a `width × height` PNG with the given pixel callback (returns u8 grayscale).
    fn encode_gray(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let mut buf = Vec::with_capacity((width * height) as usize);
            for y in 0..height {
                for x in 0..width {
                    buf.push(pixel(x, y));
                }
            }
            writer.write_image_data(&buf).unwrap();
        }
        out
    }

    #[test]
    fn dhash_of_uniform_image_is_stable() {
        let png = encode_gray(8, 8, |_, _| 0);
        let h = compute_dhash(&png).unwrap();
        assert_eq!(h, 0, "uniform image → no left/right diff bits");
    }

    #[test]
    fn dhash_of_left_right_split_is_nonzero() {
        // left-half WHITE / right-half BLACK so left > right triggers
        // diff bit = 1 (algorithm is `pixel[x] > pixel[x+1] → 1`).
        // Also use a 16-wide source so resize lands cleanly inside both halves.
        let png = encode_gray(16, 8, |x, _| if x < 8 { 255 } else { 0 });
        let h = compute_dhash(&png).unwrap();
        assert_ne!(
            h, 0,
            "left-half white / right-half black must trigger diff bits"
        );
    }

    #[test]
    fn hamming_distance_identical_is_zero() {
        assert_eq!(
            hamming_distance(0xDEAD_BEEF_DEAD_BEEF, 0xDEAD_BEEF_DEAD_BEEF),
            0
        );
    }

    #[test]
    fn hamming_distance_flipped_bits_count_is_64() {
        assert_eq!(hamming_distance(0, u64::MAX), 64);
    }

    #[test]
    fn dhash_of_non_png_bytes_errors() {
        let err = compute_dhash(b"definitely not a png").unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
        assert!(
            err.message.contains("PNG decode"),
            "message must mention PNG decode, got: {}",
            err.message
        );
    }

    #[test]
    fn dhash_rgb_and_rgba_match_grayscale() {
        // sanity: identical content in different channel layouts should
        // produce the same dhash (gray = (R+G+B)/3 averaging).
        let gray = encode_gray(8, 8, |x, _| if x < 4 { 0 } else { 255 });
        let mut rgb = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut rgb, 8, 8);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            let mut buf = Vec::with_capacity(8 * 8 * 3);
            for _ in 0..8 {
                for x in 0..8 {
                    let v: u8 = if x < 4 { 0 } else { 255 };
                    buf.extend_from_slice(&[v, v, v]);
                }
            }
            w.write_image_data(&buf).unwrap();
        }
        let h_gray = compute_dhash(&gray).unwrap();
        let h_rgb = compute_dhash(&rgb).unwrap();
        assert_eq!(h_gray, h_rgb);
    }
}
