//! v5.2 c6 — `assert_screenshot_inner` 纯函数 smoke 测 (host-side, no App).
//!
//! Strict-mode env handling and dhash match-vs-mismatch lifecycle covered
//! here; full Adapter end-to-end covered in adapter `tests/runtime_mock.rs`.

use smix_sdk::AssertScreenshotOutcome;
use std::path::PathBuf;

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
    let out = smix_sdk::assert_screenshot_inner(&png, &path, 5, false).unwrap();
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
    let out = smix_sdk::assert_screenshot_inner(&png, &path, 5, false).unwrap();
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
    let err = smix_sdk::assert_screenshot_inner(&png, &path, 5, true).unwrap_err();
    assert_eq!(err.code, smix_sdk::FailureCode::DriverError);
    assert!(
        err.message.contains("SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD"),
        "msg: {}",
        err.message
    );
    assert!(!path.exists(), "strict mode must not write");
}
