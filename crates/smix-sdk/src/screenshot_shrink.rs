//! A screenshot made small enough to hand to a reader with a size limit.
//!
//! A full iPhone frame is about 1206×2622 px and half a megabyte of PNG;
//! as base64 text it is three quarters of a million characters, more than
//! an agent's tool result may hold. Each output pixel is the mean of the
//! source pixels under it, so thin text stays legible rather than being
//! sampled away.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

use crate::screenshot_compare::{decode_rgb, encode_rgb};

/// A PNG and the size it decodes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shrunk {
    /// The encoded image.
    pub png: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Width and height from a PNG's header, without decoding it. `None` for
/// bytes that are not a PNG.
#[must_use]
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || &png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((w, h))
}

/// `png` scaled to at most `max_width` pixels wide, keeping its aspect.
/// An image already that narrow comes back as it was.
///
/// # Errors
///
/// `DriverError` for a zero `max_width` or bytes that do not decode.
pub fn shrink_to_width(png: &[u8], max_width: u32) -> Result<Shrunk, ExpectationFailure> {
    if max_width == 0 {
        return Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "maxWidth must be at least 1 pixel".into(),
            ..Default::default()
        }));
    }
    let image = decode_rgb(png)?;
    let (w, h) = (image.w, image.h);
    if w <= max_width as usize {
        return Ok(Shrunk {
            png: png.to_vec(),
            width: w as u32,
            height: h as u32,
        });
    }
    let nw = max_width as usize;
    let nh = ((h * nw + w / 2) / w).max(1);
    let mut out = Vec::with_capacity(nw * nh * 3);
    for oy in 0..nh {
        let (y0, y1) = span(oy, nh, h);
        for ox in 0..nw {
            let (x0, x1) = span(ox, nw, w);
            let mut sum = [0u64; 3];
            for y in y0..y1 {
                for px in &image.px[y * w + x0..y * w + x1] {
                    for c in 0..3 {
                        sum[c] += u64::from(px[c]);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            out.extend(sum.iter().map(|s| ((s + n / 2) / n) as u8));
        }
    }
    let encoded = encode_rgb(nw as u32, nh as u32, &out)?;
    Ok(Shrunk {
        png: encoded,
        width: nw as u32,
        height: nh as u32,
    })
}

/// The source pixels that output pixel `i` of `n` covers, out of `len`;
/// never empty, because `n <= len`.
fn span(i: usize, n: usize, len: usize) -> (usize, usize) {
    let a = i * len / n;
    let b = ((i + 1) * len / n).max(a + 1);
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_of(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend(f(x, y));
            }
        }
        encode_rgb(w, h, &data).expect("encode")
    }

    #[test]
    fn the_header_gives_the_size_without_decoding() {
        let png = png_of(7, 3, |_, _| [0, 0, 0]);
        assert_eq!(png_size(&png), Some((7, 3)));
        assert_eq!(png_size(b"not a png at all, not at all"), None);
    }

    #[test]
    fn a_wide_image_comes_back_at_the_width_asked_for_with_its_aspect() {
        let png = png_of(1206, 2622, |_, _| [10, 20, 30]);
        let s = shrink_to_width(&png, 400).expect("shrink");
        assert_eq!((s.width, s.height), (400, 870));
        assert_eq!(png_size(&s.png), Some((400, 870)));
        assert!(s.png.len() < png.len());
    }

    #[test]
    fn each_output_pixel_is_the_mean_of_the_pixels_under_it() {
        // two columns, black and white, halved to one: mid grey
        let png = png_of(
            2,
            1,
            |x, _| if x == 0 { [0, 0, 0] } else { [255, 255, 255] },
        );
        let s = shrink_to_width(&png, 1).expect("shrink");
        let img = decode_rgb(&s.png).expect("decode");
        assert_eq!(img.px, vec![[128, 128, 128]]);
    }

    #[test]
    fn an_image_already_narrow_enough_is_returned_unchanged() {
        let png = png_of(300, 10, |_, _| [1, 2, 3]);
        let s = shrink_to_width(&png, 400).expect("shrink");
        assert_eq!(s.png, png);
        assert_eq!((s.width, s.height), (300, 10));
    }

    #[test]
    fn a_zero_width_is_refused() {
        let png = png_of(4, 4, |_, _| [0, 0, 0]);
        assert!(shrink_to_width(&png, 0).is_err());
    }
}
