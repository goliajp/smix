//! The two parts of maestro's `assertScreenshot` that are not a hash:
//! cropping to one element, and comparing pixel by pixel.
//!
//! maestro (`ScreenshotMatch.kt`, main `c436d39f`) passes a comparison
//! when the share of pixels that match is at least `thresholdPercentage`
//! (default 95). Two pixels match when their RGB colours are within
//! `0.1 · √(3 · 255²)` of each other, Euclidean. Images of different
//! sizes do not compare at all; they fail.
//!
//! `cropOn` (`Orchestra.kt`) finds an element, refuses one with no area,
//! and compares only its region — so the baseline is itself a cropped
//! image, and `takeScreenshot` takes `cropOn` too, to make it.
//!
//! What smix adds is `mask`: a region neither image is judged on. Here a
//! masked pixel counts neither as matching nor as differing — it leaves
//! the denominator as well as the numerator, so a mask cannot make a
//! comparison easier or harder than the unmasked part deserves.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_screen::Rect;

use crate::ScreenMask;

/// maestro's per-pixel tolerance, as a share of the largest possible
/// RGB distance (`DEFAULT_PIXEL_TOLERANCE` in `ScreenshotMatch.kt`).
pub const PIXEL_TOLERANCE: f64 = 0.1;

/// A decoded image as 8-bit RGB, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbImage {
    /// Width in pixels.
    pub w: usize,
    /// Height in pixels.
    pub h: usize,
    px: Vec<[u8; 3]>,
}

impl RgbImage {
    /// An image from its pixels, row-major; `None` when the count is not
    /// `w * h`.
    #[must_use]
    pub fn from_pixels(w: usize, h: usize, px: Vec<[u8; 3]>) -> Option<Self> {
        (px.len() == w * h).then_some(Self { w, h, px })
    }

    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.px[y * self.w + x]
    }
}

/// How two images compared, pixel by pixel.
#[derive(Clone, Debug, PartialEq)]
pub enum PixelMatch {
    /// Both the same size: the share (0..=100) of compared pixels that
    /// match, and how many pixels were compared.
    Compared {
        /// Share of compared pixels that match, 0..=100.
        percent: f64,
        /// Pixels outside every mask — the denominator.
        compared: u64,
    },
    /// Different sizes — maestro fails these without comparing, and so
    /// does this.
    SizeMismatch {
        /// Baseline `(w, h)`.
        expected: (usize, usize),
        /// Current `(w, h)`.
        actual: (usize, usize),
    },
}

fn failure(code: FailureCode, message: String) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(code),
        message,
        ..Default::default()
    })
}

/// Decode a PNG to 8-bit RGB. Palette, 16-bit and grayscale images are
/// expanded; alpha is dropped (a screenshot has nothing behind it).
///
/// # Errors
///
/// `DriverError` for a malformed PNG or a zero dimension.
pub fn decode_rgb(png_bytes: &[u8]) -> Result<RgbImage, ExpectationFailure> {
    let mut decoder = png::Decoder::new(png_bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| {
        failure(
            FailureCode::DriverError,
            format!("PNG decode (read_info) failed: {e}"),
        )
    })?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| {
        failure(
            FailureCode::DriverError,
            format!("PNG decode (next_frame) failed: {e}"),
        )
    })?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 {
        return Err(failure(
            FailureCode::DriverError,
            format!("PNG has zero dimension: {w}×{h}"),
        ));
    }
    let channels = info.color_type.samples();
    let px = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|c| match channels {
            1 | 2 => [c[0], c[0], c[0]],
            _ => [c[0], c[1], c[2]],
        })
        .collect();
    Ok(RgbImage { w, h, px })
}

fn masked(masks: &[ScreenMask], fx: f64, fy: f64) -> bool {
    masks
        .iter()
        .any(|m| fx >= m.x && fx < m.x + m.width && fy >= m.y && fy < m.y + m.height)
}

/// maestro's pixel comparison, with `masks` left out of both images.
///
/// # Errors
///
/// `AssertionFailed` when the masks cover every pixel: a comparison of
/// nothing would pass whatever was on the screen.
pub fn pixel_match_percent(
    expected: &RgbImage,
    actual: &RgbImage,
    masks: &[ScreenMask],
) -> Result<PixelMatch, ExpectationFailure> {
    if (expected.w, expected.h) != (actual.w, actual.h) {
        return Ok(PixelMatch::SizeMismatch {
            expected: (expected.w, expected.h),
            actual: (actual.w, actual.h),
        });
    }
    let limit = (PIXEL_TOLERANCE * (3.0 * 255.0_f64 * 255.0).sqrt()).powi(2);
    let (mut compared, mut differing) = (0u64, 0u64);
    for y in 0..expected.h {
        for x in 0..expected.w {
            let (fx, fy) = (x as f64 / expected.w as f64, y as f64 / expected.h as f64);
            if masked(masks, fx, fy) {
                continue;
            }
            compared += 1;
            let (e, a) = (expected.at(x, y), actual.at(x, y));
            if e == a {
                continue;
            }
            let sq: f64 = (0..3)
                .map(|i| {
                    let d = f64::from(a[i]) - f64::from(e[i]);
                    d * d
                })
                .sum();
            if sq > limit {
                differing += 1;
            }
        }
    }
    if compared == 0 {
        return Err(failure(
            FailureCode::AssertionFailed,
            format!(
                "assertScreenshot: the masks cover all {}×{} pixels, so nothing is \
                 left to compare — a comparison of nothing would pass whatever was \
                 on the screen",
                expected.w, expected.h
            ),
        ));
    }
    let percent = 100.0 - (differing as f64 / compared as f64) * 100.0;
    Ok(PixelMatch::Compared { percent, compared })
}

/// A region of a screenshot, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// Where an element is in a screenshot's pixels.
///
/// The tree and the screenshot measure in different units — points on
/// iOS, pixels on Android — so the scale is not a device constant to
/// look up but the ratio of two readings of the same screen: the image's
/// width over the tree root's. The region is clipped to the image, and
/// grows outward to whole pixels so an edge is not shaved off.
///
/// # Errors
///
/// A sentence when the element has no area inside the image.
pub fn crop_rect_in_pixels(
    element: &Rect,
    root: &Rect,
    png_w: u32,
    png_h: u32,
) -> Result<PixelRect, String> {
    if root.w <= 0.0 || root.h <= 0.0 {
        return Err(format!(
            "the screen's root has no size ({}×{}), so an element's place in the \
             screenshot cannot be worked out",
            root.w, root.h
        ));
    }
    let (sx, sy) = (f64::from(png_w) / root.w, f64::from(png_h) / root.h);
    let x0 = ((element.x - root.x) * sx).floor().max(0.0);
    let y0 = ((element.y - root.y) * sy).floor().max(0.0);
    let x1 = ((element.x - root.x + element.w) * sx)
        .ceil()
        .min(f64::from(png_w));
    let y1 = ((element.y - root.y + element.h) * sy)
        .ceil()
        .min(f64::from(png_h));
    if x1 <= x0 || y1 <= y0 {
        return Err(format!(
            "the element at ({}, {}, {}×{}) has no area inside the {png_w}×{png_h} \
             screenshot",
            element.x, element.y, element.w, element.h
        ));
    }
    Ok(PixelRect {
        x: x0 as u32,
        y: y0 as u32,
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
    })
}

/// A PNG of `region` of `png_bytes`, 8-bit RGB.
///
/// # Errors
///
/// `DriverError` for a malformed PNG, a region reaching outside the
/// image, or an encoder failure.
pub fn crop_png(png_bytes: &[u8], region: PixelRect) -> Result<Vec<u8>, ExpectationFailure> {
    let img = decode_rgb(png_bytes)?;
    let (x, y, w, h) = (
        region.x as usize,
        region.y as usize,
        region.w as usize,
        region.h as usize,
    );
    if w == 0 || h == 0 || x + w > img.w || y + h > img.h {
        return Err(failure(
            FailureCode::DriverError,
            format!(
                "crop {region:?} reaches outside the {}×{} image",
                img.w, img.h
            ),
        ));
    }
    let mut data = Vec::with_capacity(w * h * 3);
    for row in y..y + h {
        for col in x..x + w {
            data.extend_from_slice(&img.at(col, row));
        }
    }
    encode_rgb(w as u32, h as u32, &data)
}

/// How a screenshot is judged against its baseline.
///
/// Two different measures, not two settings of one: a perceptual hash
/// counts how many of 64 brightness gradients flipped, maestro's
/// comparison counts how many pixels kept their colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScreenshotCompare {
    /// 9×8 dhash; passes when at most `max_hamming` of its 64 bits differ.
    Hash {
        /// Bits allowed to differ.
        max_hamming: u32,
    },
    /// maestro's pixel comparison; passes when at least
    /// `min_match_percent` of the compared pixels match.
    Pixels {
        /// Share of matching pixels required, 0..=100.
        min_match_percent: f64,
    },
}

/// An element to crop a screenshot to, with the tree root it was
/// measured against — the two readings the scale is derived from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropTo {
    /// The element's bounds, in tree units.
    pub element: Rect,
    /// The tree root's bounds, in the same units.
    pub root: Rect,
}

/// One `assertScreenshot`: where its baseline is, how it is judged, what
/// is left out, and what it is cropped to.
///
/// Masks are shares of the image being compared — the cropped one when
/// there is a crop.
#[derive(Clone, Copy, Debug)]
pub struct ScreenshotCheck<'a> {
    /// Baseline PNG; written from the (cropped) capture when missing and
    /// not strict.
    pub baseline: &'a std::path::Path,
    /// How the two images are judged.
    pub compare: ScreenshotCompare,
    /// Regions neither image is judged on.
    pub masks: &'a [ScreenMask],
    /// The element to crop to, if any.
    pub crop: Option<CropTo>,
}

/// `png_bytes` cropped to `crop`'s element.
///
/// # Errors
///
/// `NotVisible` when the element has no area inside the screenshot;
/// `DriverError` for a malformed PNG.
pub fn crop_to(png_bytes: &[u8], crop: &CropTo) -> Result<Vec<u8>, ExpectationFailure> {
    let (w, h) = png_size(png_bytes)?;
    let region = crop_rect_in_pixels(&crop.element, &crop.root, w, h)
        .map_err(|why| failure(FailureCode::NotVisible, format!("cropOn: {why}")))?;
    crop_png(png_bytes, region)
}

fn png_size(png_bytes: &[u8]) -> Result<(u32, u32), ExpectationFailure> {
    let reader = png::Decoder::new(png_bytes).read_info().map_err(|e| {
        failure(
            FailureCode::DriverError,
            format!("PNG decode (read_info) failed: {e}"),
        )
    })?;
    let info = reader.info();
    Ok((info.width, info.height))
}

/// Judge `current` against `baseline` by maestro's pixel rule.
///
/// # Errors
///
/// `AssertionFailed` when the sizes differ, when fewer than
/// `min_match_percent` of the compared pixels match, or when the masks
/// leave nothing to compare.
pub(crate) fn judge_pixels(
    current: &[u8],
    baseline: &[u8],
    min_match_percent: f64,
    masks: &[ScreenMask],
) -> Result<f64, ExpectationFailure> {
    let (actual, expected) = (decode_rgb(current)?, decode_rgb(baseline)?);
    match pixel_match_percent(&expected, &actual, masks)? {
        PixelMatch::SizeMismatch { expected, actual } => Err(failure(
            FailureCode::AssertionFailed,
            format!(
                "assertScreenshot: the screenshot is {}×{} and the baseline {}×{}; images \
                 of different sizes do not compare",
                actual.0, actual.1, expected.0, expected.1
            ),
        )),
        PixelMatch::Compared { percent, compared } if percent < min_match_percent => Err(failure(
            FailureCode::AssertionFailed,
            format!(
                "assertScreenshot: {percent:.2}% of {compared} compared pixels match the \
                     baseline; thresholdPercentage asks for {min_match_percent}%"
            ),
        )),
        PixelMatch::Compared { percent, .. } => Ok(percent),
    }
}

fn encode_rgb(w: u32, h: u32, data: &[u8]) -> Result<Vec<u8>, ExpectationFailure> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| failure(FailureCode::DriverError, format!("PNG encode: {e}")))?;
    writer
        .write_image_data(data)
        .map_err(|e| failure(FailureCode::DriverError, format!("PNG encode: {e}")))?;
    writer
        .finish()
        .map_err(|e| failure(FailureCode::DriverError, format!("PNG encode: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixels(w, h, vec![c; w * h]).expect("w*h pixels")
    }

    /// A 10×10 image whose first `n` pixels (row-major) are white on black.
    fn with_white_prefix(n: usize) -> RgbImage {
        let mut px = vec![[0, 0, 0]; 100];
        for p in px.iter_mut().take(n) {
            *p = [255, 255, 255];
        }
        RgbImage::from_pixels(10, 10, px).expect("100 pixels")
    }

    #[test]
    fn three_differing_pixels_in_a_hundred_match_ninety_seven_percent() {
        let got = pixel_match_percent(&solid(10, 10, [0, 0, 0]), &with_white_prefix(3), &[])
            .expect("something is compared");
        assert_eq!(
            got,
            PixelMatch::Compared {
                percent: 97.0,
                compared: 100
            }
        );
    }

    #[test]
    fn a_difference_within_maestros_tolerance_is_a_match() {
        // 0.1 of the largest RGB distance is ~44 on each channel together;
        // 20 on each is well inside it, 60 on each is outside.
        let near = solid(10, 10, [20, 20, 20]);
        let far = solid(10, 10, [60, 60, 60]);
        let base = solid(10, 10, [0, 0, 0]);
        assert_eq!(
            pixel_match_percent(&base, &near, &[]).unwrap(),
            PixelMatch::Compared {
                percent: 100.0,
                compared: 100
            }
        );
        assert_eq!(
            pixel_match_percent(&base, &far, &[]).unwrap(),
            PixelMatch::Compared {
                percent: 0.0,
                compared: 100
            }
        );
    }

    #[test]
    fn a_masked_pixel_leaves_the_denominator_as_well_as_the_numerator() {
        // The top row differs entirely; masking it leaves 90 pixels, all
        // equal. Counting the masked ten as matches would read 100 of 100
        // and hide that the mask decided the answer; leaving them out of
        // the numerator only would read 90 of 100 and fail a clean image.
        let top_row = ScreenMask {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 0.1,
        };
        let got = pixel_match_percent(
            &solid(10, 10, [0, 0, 0]),
            &with_white_prefix(10),
            &[top_row],
        )
        .unwrap();
        assert_eq!(
            got,
            PixelMatch::Compared {
                percent: 100.0,
                compared: 90
            }
        );
    }

    #[test]
    fn images_of_different_sizes_do_not_compare() {
        let got = pixel_match_percent(&solid(10, 10, [0; 3]), &solid(10, 12, [0; 3]), &[]).unwrap();
        assert_eq!(
            got,
            PixelMatch::SizeMismatch {
                expected: (10, 10),
                actual: (10, 12)
            }
        );
    }

    #[test]
    fn a_mask_over_everything_is_refused_not_passed() {
        let all = ScreenMask {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        let err = pixel_match_percent(&solid(4, 4, [0; 3]), &solid(4, 4, [255; 3]), &[all])
            .expect_err("nothing is left to compare");
        assert!(
            err.message.contains("nothing is left to compare"),
            "{}",
            err.message
        );
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn an_element_in_points_lands_on_the_pixels_the_screenshot_has() {
        // iOS: a 402-point-wide tree, a 1206-pixel-wide image — three
        // pixels to the point, read off the two, not looked up.
        let got = crop_rect_in_pixels(
            &rect(10.0, 20.0, 100.0, 50.0),
            &rect(0.0, 0.0, 402.0, 874.0),
            1206,
            2622,
        )
        .unwrap();
        assert_eq!(
            got,
            PixelRect {
                x: 30,
                y: 60,
                w: 300,
                h: 150
            }
        );
    }

    #[test]
    fn an_element_past_the_edge_is_clipped_to_the_screenshot() {
        let got = crop_rect_in_pixels(
            &rect(350.0, 800.0, 100.0, 200.0),
            &rect(0.0, 0.0, 400.0, 900.0),
            400,
            900,
        )
        .unwrap();
        assert_eq!(
            got,
            PixelRect {
                x: 350,
                y: 800,
                w: 50,
                h: 100
            }
        );
    }

    #[test]
    fn an_element_with_no_area_on_screen_is_refused() {
        let err = crop_rect_in_pixels(
            &rect(500.0, 10.0, 40.0, 40.0),
            &rect(0.0, 0.0, 400.0, 900.0),
            400,
            900,
        )
        .expect_err("it is off to the right");
        assert!(err.contains("no area"), "{err}");
    }

    #[test]
    fn a_crop_keeps_exactly_the_pixels_inside_it() {
        // A 4×4 image, each pixel's red channel its index; crop the
        // middle 2×2 and read back which indices came out.
        let data: Vec<u8> = (0..16u8).flat_map(|i| [i, 0, 0]).collect();
        let png = encode_rgb(4, 4, &data).unwrap();
        let cut = crop_png(
            &png,
            PixelRect {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            },
        )
        .unwrap();
        let back = decode_rgb(&cut).unwrap();
        assert_eq!((back.w, back.h), (2, 2));
        let reds: Vec<u8> = (0..2)
            .flat_map(|y| (0..2).map(move |x| (x, y)))
            .map(|(x, y)| back.at(x, y)[0])
            .collect();
        assert_eq!(reds, vec![5, 6, 9, 10]);
    }
}
