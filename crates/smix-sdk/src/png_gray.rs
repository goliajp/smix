//! PNG → grayscale sampling, shared by the screenshot comparisons.
//!
//! Both callers want the same thing — a grayscale value at (x, y) — and
//! neither wants the whole image materialized: dhash reads 72 points, and
//! quiescence reads a grid. So this hands back a view over the decoded rows
//! rather than a converted buffer.
//!
//! Uses the `png` crate rather than `image`, which is ~3MB for a decode this
//! narrow.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

/// A decoded PNG, sampled as grayscale on demand.
pub(crate) struct GrayView {
    raw: Vec<u8>,
    channels: usize,
    /// Image width in pixels.
    pub(crate) w: usize,
    /// Image height in pixels.
    pub(crate) h: usize,
}

impl GrayView {
    /// Grayscale value at `(x, y)`. Out-of-bounds reads return 0 rather than
    /// panicking — a caller sampling a grid should not have to bounds-check
    /// every point.
    pub(crate) fn gray(&self, x: usize, y: usize) -> u8 {
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
pub(crate) fn decode_gray(png_bytes: &[u8]) -> Result<GrayView, ExpectationFailure> {
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
