//! Is the screen still? Two frames compared on a grid.
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
//!
//! Here, below the SDK, because the driver waits on it too: a touch aimed
//! at a point waits for the screen under it to stop moving.
//!
//! PNG decoding uses the `png` crate rather than `image`, which is ~3MB for
//! a decode this narrow.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

/// What counts as still.
///
/// The defaults are measured against a booted simulator at device resolution
/// (1206×2622): a settled screen reads as still, a real launch transition
/// reads as motion for its whole duration, a caret-sized region is tolerated,
/// and a spinner-sized one is not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuiescenceParams {
    /// Frames are compared on a `grid × grid` lattice rather than pixel by
    /// pixel: a screenshot is millions of pixels, and comparing all of them
    /// would cost more than the wait being replaced.
    pub grid: usize,
    /// A sample counts as moved when its grayscale differs by more than this.
    ///
    /// A small guard rather than a noise filter. Measured on a settled sim
    /// screen, consecutive screenshots are *pixel-identical* — zero samples
    /// differ — because the renderer is deterministic and there is no camera
    /// or encoder in the path. This exists so a single level of rounding
    /// somewhere cannot read as motion.
    pub pixel_delta: u8,
    /// How many moved samples still count as still.
    ///
    /// This is the one doing real work, and it is deliberately not zero. A
    /// blinking caret never stops, so a screen with one would never be still
    /// and every wait on a text field would run to the ceiling — worse than
    /// the sleep this replaces. At device resolution a 3pt×20pt caret lands
    /// on a handful of samples; a spinner covers many more, and still reads
    /// as motion.
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
    Ok(grid_still(
        (a.w, a.h),
        |x, y| a.gray(x, y),
        |x, y| b.gray(x, y),
        params,
    ))
}

/// The stillness core every frame shape shares: sample both frames on a `grid × grid`
/// lattice and count how many samples moved past `pixel_delta`.
pub fn grid_still(
    dims: (usize, usize),
    a: impl Fn(usize, usize) -> u8,
    b: impl Fn(usize, usize) -> u8,
    params: &QuiescenceParams,
) -> bool {
    let (w, h) = dims;
    let grid = params.grid.max(1);
    let mut moved = 0usize;
    for gy in 0..grid {
        let sy = (gy * h) / grid;
        for gx in 0..grid {
            let sx = (gx * w) / grid;
            let delta = a(sx, sy).abs_diff(b(sx, sy));
            if delta > params.pixel_delta {
                moved += 1;
                if moved > params.max_moved_samples {
                    return false;
                }
            }
        }
    }
    true
}

/// A decoded PNG, sampled as grayscale on demand.
pub struct GrayView {
    raw: Vec<u8>,
    channels: usize,
    /// Image width in pixels.
    pub w: usize,
    /// Image height in pixels.
    pub h: usize,
}

impl GrayView {
    /// Grayscale value at `(x, y)`. Out-of-bounds reads return 0 rather than
    /// panicking — a caller sampling a grid should not have to bounds-check
    /// every point.
    pub fn gray(&self, x: usize, y: usize) -> u8 {
        if x >= self.w || y >= self.h {
            return 0;
        }
        let idx = (y * self.w + x) * self.channels;
        match self.channels {
            // Grayscale, and grayscale + alpha (alpha ignored).
            1 | 2 => self.raw[idx],
            3 | 4 => {
                let r = self.raw[idx] as u16;
                let g = self.raw[idx + 1] as u16;
                let b = self.raw[idx + 2] as u16;
                ((r + g + b) / 3) as u8
            }
            _ => 0,
        }
    }
}

/// Decode a PNG for grayscale sampling.
///
/// Returns `Err(DriverError)` for a malformed PNG, a zero dimension, or a
/// color type outside `{Rgb, Rgba, Grayscale, GrayscaleAlpha}` — an explicit
/// failure, never a silent no-op.
pub fn decode_gray(png_bytes: &[u8]) -> Result<GrayView, ExpectationFailure> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().map_err(|e| {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("PNG decode (read_info) failed: {e}"),
            ..Default::default()
        })
    })?;
    let size = reader.output_buffer_size().ok_or_else(|| {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "PNG decode: the image is too large to hold in memory".to_string(),
            ..Default::default()
        })
    })?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).map_err(|e| {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("PNG decode (next_frame) failed: {e}"),
            ..Default::default()
        })
    })?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3usize,
        png::ColorType::Rgba => 4usize,
        png::ColorType::Grayscale => 1usize,
        png::ColorType::GrayscaleAlpha => 2usize,
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
    let size = info.buffer_size();
    buf.truncate(size);
    Ok(GrayView {
        raw: buf,
        channels,
        w,
        h,
    })
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
    fn a_level_of_rounding_is_not_motion() {
        // A settled sim screen is pixel-identical frame to frame, so this
        // guard has nothing to absorb in practice. It is here so that one
        // level of difference somewhere cannot read as a moving screen.
        let a = flat(128);
        let b = encode(W, H, |x, y| if (x + y) % 2 == 0 { 130 } else { 126 });
        let p = QuiescenceParams::default();
        assert!(
            p.pixel_delta >= 2,
            "the case only means something below the threshold"
        );
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
