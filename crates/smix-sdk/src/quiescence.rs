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

use crate::png_gray::decode_gray;

/// What counts as still.
///
/// The defaults are reasoned rather than measured — see the note on
/// `max_moved_samples`. They want a real simulator before anyone treats them
/// as tuned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuiescenceParams {
    /// Frames are compared on a `grid × grid` lattice rather than pixel by
    /// pixel: a screenshot is millions of pixels, and comparing all of them
    /// every cadence would cost more than the wait being replaced.
    pub grid: usize,
    /// A sample counts as moved when its grayscale differs by more than this.
    /// Absorbs encoder noise and dithering, which are not motion.
    pub pixel_delta: u8,
    /// How many moved samples still count as still.
    ///
    /// Deliberately not zero. A blinking caret never stops, and a screen with
    /// one in it would never be still — so every wait on a text field would
    /// run to the ceiling, which is worse than the sleep this replaces. A
    /// caret lands on about one sample; a spinner or a transition covers
    /// considerably more.
    pub max_moved_samples: usize,
}

impl Default for QuiescenceParams {
    fn default() -> Self {
        Self {
            grid: 96,
            pixel_delta: 4,
            max_moved_samples: 4,
        }
    }
}

/// Whether two consecutive frames are still relative to each other.
///
/// A change in dimensions counts as motion — a rotation mid-wait is the screen
/// doing something, not an error.
///
/// Returns `Err(DriverError)` when a frame won't decode. A frame we cannot
/// read is not a still frame, and saying so beats guessing.
pub fn frames_are_still(
    previous_png: &[u8],
    next_png: &[u8],
    params: &QuiescenceParams,
) -> Result<bool, ExpectationFailure> {
    let a = decode_gray(previous_png)?;
    let b = decode_gray(next_png)?;

    if a.w != b.w || a.h != b.h {
        return Ok(false);
    }

    let grid = params.grid.max(1);
    let mut moved = 0usize;
    for gy in 0..grid {
        let sy = (gy * a.h) / grid;
        for gx in 0..grid {
            let sx = (gx * a.w) / grid;
            let delta = a.gray(sx, sy).abs_diff(b.gray(sx, sy));
            if delta > params.pixel_delta {
                moved += 1;
                if moved > params.max_moved_samples {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
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

    const W: u32 = 400;
    const H: u32 = 800;

    fn flat(v: u8) -> Vec<u8> {
        encode(W, H, |_, _| v)
    }

    #[test]
    fn identical_frames_are_still() {
        let a = flat(128);
        let b = flat(128);
        assert!(frames_are_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn a_whole_screen_change_is_motion() {
        let a = flat(0);
        let b = flat(255);
        assert!(!frames_are_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn noise_under_the_pixel_delta_is_not_motion() {
        // Encoder noise and dithering wobble a value or two. That is not the
        // screen doing something.
        let a = flat(128);
        let b = encode(W, H, |x, y| if (x + y) % 2 == 0 { 130 } else { 126 });
        let p = QuiescenceParams::default();
        assert!(p.pixel_delta >= 2, "the case only means something below the threshold");
        assert!(frames_are_still(&a, &b, &p).unwrap());
    }

    #[test]
    fn a_spinner_sized_region_is_motion() {
        // The failure mode that rules dhash out: something small, moving, in
        // one corner. On a 96 grid this covers enough samples to be seen.
        let a = flat(200);
        let b = encode(W, H, |x, y| if x < 60 && y < 60 { 20 } else { 200 });
        assert!(!frames_are_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn a_caret_sized_blink_is_tolerated() {
        // A caret never stops blinking. Calling it motion would mean every
        // wait on a text field ran to the ceiling.
        let a = flat(200);
        let b = encode(W, H, |x, y| {
            if (100..102).contains(&x) && (300..320).contains(&y) {
                20
            } else {
                200
            }
        });
        assert!(frames_are_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn a_rotation_is_motion() {
        let a = encode(400, 800, |_, _| 128);
        let b = encode(800, 400, |_, _| 128);
        assert!(!frames_are_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }

    #[test]
    fn an_undecodable_frame_is_an_error_not_a_still_frame() {
        let a = flat(128);
        let err = frames_are_still(&a, b"not a png", &QuiescenceParams::default()).unwrap_err();
        assert_eq!(err.code, smix_error::FailureCode::DriverError);
    }

    #[test]
    fn the_thresholds_are_parameters_not_magic_numbers() {
        // The same pair reads differently under different params — which is
        // the point: these want tuning against a real simulator.
        let a = flat(200);
        let b = encode(W, H, |x, y| if x < 60 && y < 60 { 20 } else { 200 });
        let forgiving = QuiescenceParams {
            max_moved_samples: 10_000,
            ..QuiescenceParams::default()
        };
        assert!(frames_are_still(&a, &b, &forgiving).unwrap());
        assert!(!frames_are_still(&a, &b, &QuiescenceParams::default()).unwrap());
    }
}
