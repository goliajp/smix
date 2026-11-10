//! Is the screen still?
//!
//! `waitForAnimationToEnd` has been a fixed sleep: guess 400 ms, hope the
//! animation was shorter, pay the 400 ms even when it wasn't running. This
//! answers the question the verb is actually asking.
//!
//! Not dhash. That hash exists next door and looks like it would fit, but it
//! resizes to 9×8 and is built to *tolerate* small differences so that
//! anti-aliasing doesn't fail a visual regression. Stillness is the opposite
//! question — whether any small thing moved — and on a 72-pixel hash a spinner
//! in the corner is invisible. Using it here would report "idle" over exactly
//! the animations this is meant to wait out.

use smix_error::ExpectationFailure;

use crate::png_gray::sampler_for;
use smix_simctl::surface_capture::CapturedFrame;

pub use smix_driver::still::{QuiescenceParams, frames_are_still};

/// Same stillness test over [`CapturedFrame`]s — the fast diff-loop path.
///
/// A raw BGRA frame is sampled in place (no PNG decode); a PNG frame (the
/// `simctl` fallback) is decoded once. Comparing a BGRA frame against a PNG
/// frame is fine — both resolve to the same grayscale grid — which keeps the
/// path correct across a mid-loop fallback.
pub fn frames_still(
    previous: &CapturedFrame,
    next: &CapturedFrame,
    params: &QuiescenceParams,
) -> Result<bool, ExpectationFailure> {
    let a = sampler_for(previous)?;
    let b = sampler_for(next)?;
    if a.dims() != b.dims() {
        return Ok(false);
    }
    let (w, h) = a.dims();
    Ok(smix_driver::still::grid_still(
        (w, h),
        |x, y| a.gray(x, y),
        |x, y| b.gray(x, y),
        params,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a `width × height` grayscale PNG from a pixel callback.
    fn encode(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let mut data = Vec::with_capacity((width * height) as usize);
            for y in 0..height {
                for x in 0..width {
                    data.push(pixel(x, y));
                }
            }
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    /// Build a solid-color BGRA frame.
    fn bgra_flat(w: u32, h: u32, b: u8, g: u8, r: u8) -> CapturedFrame {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            data.extend_from_slice(&[b, g, r, 0xff]);
        }
        CapturedFrame::Bgra {
            width: w,
            height: h,
            data,
        }
    }

    #[test]
    fn identical_bgra_frames_are_still() {
        let a = bgra_flat(64, 64, 10, 20, 30);
        let b = bgra_flat(64, 64, 10, 20, 30);
        assert!(frames_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn a_whole_bgra_screen_change_is_motion() {
        let a = bgra_flat(64, 64, 0, 0, 0);
        let b = bgra_flat(64, 64, 255, 255, 255);
        assert!(!frames_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn a_bgra_dimension_change_is_motion() {
        let a = bgra_flat(64, 64, 128, 128, 128);
        let b = bgra_flat(32, 128, 128, 128, 128);
        assert!(!frames_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn bgra_and_its_png_encoding_compare_equal() {
        // The fallback correctness contract: a raw BGRA frame and a PNG of the
        // SAME pixels must read as still relative to each other. If the BGRA
        // channel order were mishandled the grayscale means would diverge.
        let (w, h) = (40u32, 40u32);
        let bgra = bgra_flat(w, h, 30, 90, 200); // B=30 G=90 R=200
        let png = encode(w, h, |_, _| ((30u16 + 90 + 200) / 3) as u8);
        let png_frame = CapturedFrame::Png(png);
        assert!(frames_still(&bgra, &png_frame, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn a_truncated_bgra_frame_is_an_error() {
        let a = bgra_flat(64, 64, 0, 0, 0);
        let bad = CapturedFrame::Bgra {
            width: 64,
            height: 64,
            data: vec![0u8; 10],
        };
        let err = frames_still(&a, &bad, &QuiescenceParams::default()).unwrap_err();
        assert_eq!(err.code, smix_error::FailureCode::DriverError);
    }
}
