//! Pure-function smoke tests for `assert_screenshot_inner` (host-side,
//! no App).
//!
//! Strict-mode env handling and dhash match-vs-mismatch lifecycle covered
//! here; full Adapter end-to-end covered in adapter `tests/runtime_mock.rs`.

use smix_sdk::{
    AssertScreenshotOutcome, CropTo, FailureCode, Rect, ScreenshotCheck, ScreenshotCompare,
};
use std::path::{Path, PathBuf};

fn hash_check(path: &Path) -> ScreenshotCheck<'_> {
    ScreenshotCheck {
        baseline: path,
        compare: ScreenshotCompare::Hash { max_hamming: 5 },
        masks: &[],
        crop: None,
    }
}

fn pixel_check(path: &Path, min_match_percent: f64) -> ScreenshotCheck<'_> {
    ScreenshotCheck {
        baseline: path,
        compare: ScreenshotCompare::Pixels { min_match_percent },
        masks: &[],
        crop: None,
    }
}

/// A `w × h` RGB PNG; `pixel(x, y)` gives each colour.
fn rgb_png(w: u32, h: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&pixel(x, y));
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&data).unwrap();
    }
    out
}

/// 10×10 black with the first `n` pixels (row-major) white.
fn black_with_white(n: u32) -> Vec<u8> {
    rgb_png(
        10,
        10,
        |x, y| if y * 10 + x < n { [255; 3] } else { [0; 3] },
    )
}

fn png_size(png: &[u8]) -> (u32, u32) {
    let reader = png::Decoder::new(png).read_info().unwrap();
    let info = reader.info();
    (info.width, info.height)
}

/// Tiny 2×2 grayscale PNG used as both "current screenshot" and seeded baseline.
fn tiny_png(value: u8) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 2, 2);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[value; 4]).unwrap();
    }
    out
}

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let unique = format!(
        "smix-c6-{name}-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    p.push(unique);
    p
}

#[test]
fn auto_records_when_baseline_missing() {
    let path = tmp_path("auto-record");
    assert!(!path.exists());
    let png = tiny_png(0);
    let out = smix_sdk::assert_screenshot_inner(&png, &hash_check(&path), false).unwrap();
    match out {
        AssertScreenshotOutcome::Recorded { path: p } => {
            assert_eq!(p, path);
            assert!(p.exists(), "baseline file should be written");
        }
        other => panic!("expected Recorded, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn matches_identical_baseline_with_zero_hamming() {
    let path = tmp_path("identical");
    let png = tiny_png(128);
    // seed baseline = current PNG
    std::fs::write(&path, &png).unwrap();
    let out = smix_sdk::assert_screenshot_inner(&png, &hash_check(&path), false).unwrap();
    match out {
        AssertScreenshotOutcome::Matched { hamming } => assert_eq!(hamming, 0),
        other => panic!("expected Matched 0, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn strict_mode_fails_when_baseline_missing() {
    let path = tmp_path("strict-missing");
    assert!(!path.exists());
    let png = tiny_png(0);
    let err = smix_sdk::assert_screenshot_inner(&png, &hash_check(&path), true).unwrap_err();
    assert_eq!(err.code, smix_sdk::FailureCode::DriverError);
    assert!(
        err.message.contains("SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD"),
        "msg: {}",
        err.message
    );
    assert!(!path.exists(), "strict mode must not write");
}

#[test]
fn pixels_pass_at_the_share_that_matched() {
    let path = tmp_path("pixels-pass");
    std::fs::write(&path, black_with_white(0)).unwrap();
    let out =
        smix_sdk::assert_screenshot_inner(&black_with_white(3), &pixel_check(&path, 95.0), false)
            .unwrap();
    match out {
        AssertScreenshotOutcome::MatchedPixels { percent } => {
            assert!((percent - 97.0).abs() < 1e-9, "percent {percent}")
        }
        other => panic!("expected MatchedPixels 97, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn pixels_fail_below_the_share_and_say_how_much_matched() {
    let path = tmp_path("pixels-fail");
    std::fs::write(&path, black_with_white(0)).unwrap();
    let err =
        smix_sdk::assert_screenshot_inner(&black_with_white(3), &pixel_check(&path, 98.0), false)
            .unwrap_err();
    assert_eq!(err.code, FailureCode::AssertionFailed);
    assert!(err.message.contains("97"), "msg: {}", err.message);
    assert!(err.message.contains("98"), "msg: {}", err.message);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn pixels_fail_when_the_sizes_differ() {
    let path = tmp_path("pixels-size");
    std::fs::write(&path, black_with_white(0)).unwrap();
    let taller = rgb_png(10, 20, |_, _| [0; 3]);
    let err =
        smix_sdk::assert_screenshot_inner(&taller, &pixel_check(&path, 0.0), false).unwrap_err();
    assert_eq!(err.code, FailureCode::AssertionFailed);
    assert!(err.message.contains("10×20"), "msg: {}", err.message);
    let _ = std::fs::remove_file(&path);
}

/// A 20×20 capture of a 10×10-unit tree (scale 2, like a 2× iOS
/// screen): an element at (5, 5, 5×5) is pixels (10, 10, 10×10).
#[test]
fn a_crop_records_only_the_element_at_the_derived_scale() {
    let path = tmp_path("crop-record");
    let capture = rgb_png(20, 20, |x, y| {
        if x >= 10 && y >= 10 {
            [255, 0, 0]
        } else {
            [0; 3]
        }
    });
    let check = ScreenshotCheck {
        crop: Some(CropTo {
            element: Rect {
                x: 5.0,
                y: 5.0,
                w: 5.0,
                h: 5.0,
            },
            root: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
        }),
        ..hash_check(&path)
    };
    let out = smix_sdk::assert_screenshot_inner(&capture, &check, false).unwrap();
    assert!(matches!(out, AssertScreenshotOutcome::Recorded { .. }));
    let written = std::fs::read(&path).unwrap();
    assert_eq!(png_size(&written), (10, 10));
    let all_red = rgb_png(10, 10, |_, _| [255, 0, 0]);
    let same =
        smix_sdk::assert_screenshot_inner(&all_red, &pixel_check(&path, 100.0), false).unwrap();
    assert!(matches!(
        same,
        AssertScreenshotOutcome::MatchedPixels { .. }
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_crop_onto_an_element_outside_the_screen_is_not_visible() {
    let path = tmp_path("crop-outside");
    let capture = rgb_png(20, 20, |_, _| [0; 3]);
    let check = ScreenshotCheck {
        crop: Some(CropTo {
            element: Rect {
                x: 12.0,
                y: 12.0,
                w: 5.0,
                h: 5.0,
            },
            root: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
        }),
        ..hash_check(&path)
    };
    let err = smix_sdk::assert_screenshot_inner(&capture, &check, false).unwrap_err();
    assert_eq!(err.code, FailureCode::NotVisible);
    assert!(
        !path.exists(),
        "nothing is recorded from an element with no area"
    );
}
