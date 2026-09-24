//! Visual regression perceptual hash (crate-internal).
//!
//! Standard **dhash 64-bit** (difference hash): decode PNG → grayscale →
//! resize 9×8 nearest-neighbor → row-wise left/right pixel diff → 64 bits.
//!
//! Algorithm intent: tolerate anti-aliasing / encoder metadata noise (where
//! byte-equality always fails) but flag substantive visual changes. Industry
//! standard for screenshot regression; maestro's BlinkDiff is in the same
//! family. SSIM / full pHash are not implemented.
//!
//! Threshold: hamming distance ≤ 5 = visually identical (hard-coded at
//! the adapter runtime layer; the SDK pub fn exposes `max_hamming`).

use smix_error::ExpectationFailure;

/// The flat value a masked sample reads, in both frames.
const MASKED: u8 = 0;

/// The 64-bit dhash of a PNG byte stream, with `masks` left out of the
/// frame (an empty slice hashes the whole of it).
///
/// Returns `Err(DriverError)` if the PNG is malformed or the color type
/// is outside `{Rgb, Rgba, Grayscale, GrayscaleAlpha}` — an explicit
/// failure, never a silent no-op.
pub(crate) fn compute_dhash_masked(
    png_bytes: &[u8],
    masks: &[ScreenMask],
) -> Result<u64, ExpectationFailure> {
    let frame = crate::png_gray::decode_gray(png_bytes)?;
    let (w, h) = (frame.w, frame.h);

    // Step 1: resize to 9×8 grayscale via nearest-neighbor (pure integer
    // arithmetic — no resize dep). Width 9 = 8 left/right pairs per row;
    // height 8 × 8 bits = 64 bits.
    let mut grid = [[0u8; 9]; 8];
    for (dy, row) in grid.iter_mut().enumerate() {
        for (dx, cell) in row.iter_mut().enumerate() {
            let sx = (dx * w) / 9;
            let sy = (dy * h) / 8;
            let (fx, fy) = (sx as f64 / w as f64, sy as f64 / h as f64);
            let masked = masks
                .iter()
                .any(|m| fx >= m.x && fx < m.x + m.width && fy >= m.y && fy < m.y + m.height);
            *cell = if masked { MASKED } else { frame.gray(sx, sy) };
        }
    }

    // Step 2: row-wise left/right diff → 64-bit MSB-first.
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

/// A region to leave out of a comparison, as shares (0..1) of the frame.
///
/// Leaving it out means both images read the same flat value there
/// before hashing, so whatever changes inside it cannot count: a video
/// playing under a control panel, a clock, a spinner. The hash samples a
/// 9×8 grid; a sample point inside a region reads the flat value in both
/// frames.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScreenMask {
    /// Left edge, as a share of the width.
    pub x: f64,
    /// Top edge, as a share of the height.
    pub y: f64,
    /// Width, as a share of the width.
    pub width: f64,
    /// Height, as a share of the height.
    pub height: f64,
}

/// Hamming distance between two 64-bit dhashes — number of differing bits.
/// 0 = identical; 64 = all bits flipped. Typical perceptual threshold ≤ 10.
pub(crate) fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_error::FailureCode;

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

    /// Two frames that differ only in their top half: the columns there
    /// run light-to-dark in one and dark-to-light in the other.
    fn frames_differing_on_top() -> (Vec<u8>, Vec<u8>) {
        let top = |flip: bool| {
            move |x: u32, y: u32| {
                if y < 40 {
                    let v = ((x * 255) / 89) as u8;
                    if flip { 255 - v } else { v }
                } else {
                    128
                }
            }
        };
        (
            encode_gray(90, 80, top(false)),
            encode_gray(90, 80, top(true)),
        )
    }

    #[test]
    fn a_masked_region_cannot_count() {
        let (a, b) = frames_differing_on_top();
        let unmasked = hamming_distance(
            compute_dhash_masked(&a, &[]).unwrap(),
            compute_dhash_masked(&b, &[]).unwrap(),
        );
        assert!(
            unmasked > 5,
            "the two frames do not differ enough to test with: {unmasked}"
        );
        let top = [ScreenMask {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 0.5,
        }];
        let masked = hamming_distance(
            compute_dhash_masked(&a, &top).unwrap(),
            compute_dhash_masked(&b, &top).unwrap(),
        );
        assert_eq!(masked, 0, "a change inside the mask still counted");
    }

    #[test]
    fn no_mask_hashes_exactly_as_before() {
        // Pinned to a value worked out apart from this code (the 9×8
        // nearest-neighbour grid and row differences, recomputed by hand
        // from the frame's pixel rule): the top four rows fall left to
        // right, the bottom four are flat. Comparing it with a second call
        // through the same code — there is only one path now — would pass
        // whatever that path did.
        let (_, b) = frames_differing_on_top();
        assert_eq!(
            compute_dhash_masked(&b, &[]).unwrap(),
            0xFFFF_FFFF_0000_0000
        );
    }

    #[test]
    fn a_mask_elsewhere_leaves_the_difference_standing() {
        // Masking the half that did not change must not hide the half
        // that did — the region is honoured, not the whole frame.
        let (a, b) = frames_differing_on_top();
        let bottom = [ScreenMask {
            x: 0.0,
            y: 0.5,
            width: 1.0,
            height: 0.5,
        }];
        let d = hamming_distance(
            compute_dhash_masked(&a, &bottom).unwrap(),
            compute_dhash_masked(&b, &bottom).unwrap(),
        );
        assert!(d > 5, "masking the unchanged half hid the changed one: {d}");
    }

    #[test]
    fn dhash_of_uniform_image_is_stable() {
        let png = encode_gray(8, 8, |_, _| 0);
        let h = compute_dhash_masked(&png, &[]).unwrap();
        assert_eq!(h, 0, "uniform image → no left/right diff bits");
    }

    #[test]
    fn dhash_of_left_right_split_is_nonzero() {
        // left-half WHITE / right-half BLACK so left > right triggers
        // diff bit = 1 (algorithm is `pixel[x] > pixel[x+1] → 1`).
        // Also use a 16-wide source so resize lands cleanly inside both halves.
        let png = encode_gray(16, 8, |x, _| if x < 8 { 255 } else { 0 });
        let h = compute_dhash_masked(&png, &[]).unwrap();
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
        let err = compute_dhash_masked(b"definitely not a png", &[]).unwrap_err();
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
        let h_gray = compute_dhash_masked(&gray, &[]).unwrap();
        let h_rgb = compute_dhash_masked(&rgb, &[]).unwrap();
        assert_eq!(h_gray, h_rgb);
    }
}
