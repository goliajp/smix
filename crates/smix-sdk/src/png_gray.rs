//! PNG → grayscale sampling, shared by the screenshot comparisons.
//!
//! Both callers want the same thing — a grayscale value at (x, y) — and
//! neither wants the whole image materialized: dhash reads 72 points, and
//! quiescence reads a grid. So this hands back a view over the decoded rows
//! rather than a converted buffer.
//!
//! The PNG decode itself is the driver's (`smix_driver::still`).

pub(crate) use smix_driver::still::{GrayView, decode_gray};
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_simctl::surface_capture::CapturedFrame;

/// A grayscale sampler over a captured frame, without materializing a
/// converted buffer. A raw BGRA frame is sampled in place (no decode, no
/// copy); a PNG frame is decoded once and sampled from the decoded rows.
pub(crate) enum GraySampler<'a> {
    /// Decoded from a PNG (owns the decoded rows).
    Decoded(GrayView),
    /// Borrowed raw BGRA8888 pixels, `w*h*4` bytes, no row padding.
    Bgra {
        /// Borrowed BGRA bytes.
        data: &'a [u8],
        /// Width in pixels.
        w: usize,
        /// Height in pixels.
        h: usize,
    },
}

impl GraySampler<'_> {
    /// `(width, height)` in pixels.
    pub(crate) fn dims(&self) -> (usize, usize) {
        match self {
            GraySampler::Decoded(g) => (g.w, g.h),
            GraySampler::Bgra { w, h, .. } => (*w, *h),
        }
    }

    /// Grayscale value at `(x, y)`; out-of-bounds reads return 0. For BGRA the
    /// channel mean is order-independent, so `(B+G+R)/3` equals `(R+G+B)/3`.
    pub(crate) fn gray(&self, x: usize, y: usize) -> u8 {
        match self {
            GraySampler::Decoded(g) => g.gray(x, y),
            GraySampler::Bgra { data, w, h } => {
                if x >= *w || y >= *h {
                    return 0;
                }
                let idx = (y * w + x) * 4;
                let b = data[idx] as u16;
                let g = data[idx + 1] as u16;
                let r = data[idx + 2] as u16;
                ((b + g + r) / 3) as u8
            }
        }
    }
}

/// Build a grayscale sampler over a captured frame. Raw BGRA is sampled in
/// place; a PNG is decoded once. Returns `Err(DriverError)` for a malformed
/// PNG or a BGRA frame whose byte length disagrees with `w*h*4`.
pub(crate) fn sampler_for(frame: &CapturedFrame) -> Result<GraySampler<'_>, ExpectationFailure> {
    match frame {
        CapturedFrame::Png(bytes) => Ok(GraySampler::Decoded(decode_gray(bytes)?)),
        CapturedFrame::Bgra {
            width,
            height,
            data,
        } => {
            let w = *width as usize;
            let h = *height as usize;
            if w == 0 || h == 0 {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("BGRA frame has zero dimension: {w}×{h}"),
                    ..Default::default()
                }));
            }
            if data.len() < w * h * 4 {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!(
                        "BGRA frame too short: {} bytes for {w}×{h} (need {})",
                        data.len(),
                        w * h * 4
                    ),
                    ..Default::default()
                }));
            }
            Ok(GraySampler::Bgra { data, w, h })
        }
    }
}
