//! Integration tests for [`smix_runner_sources::extract_to`].

use std::fs;

use smix_runner_sources::{
    ExtractError, SOURCES_VERSION, VERSION_FILE, extract_to, read_installed_version,
};

#[test]
fn extract_writes_expected_landmark_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let report = extract_to(dir.path(), false).expect("extract");

    assert_eq!(report.destination, dir.path());
    assert!(
        report.file_count > 100,
        "expected many files, got {}",
        report.file_count
    );
    assert!(
        report.backup.is_none(),
        "empty target should not trigger backup"
    );
    assert_eq!(report.version_written, SOURCES_VERSION);

    // Landmark files that MUST exist for xcodebuild to succeed. If any
    // of these are missing the tarball generation excluded too much.
    let landmarks = [
        "Package.swift",
        "SmixRunner.xcodeproj/project.pbxproj",
        "SmixRunnerUITests/SmixRunnerUITests.swift",
        "Sources/SmixRunnerCore/SmixRunnerServer.swift",
    ];
    for path in landmarks {
        let full = dir.path().join(path);
        assert!(full.exists(), "missing landmark file: {}", full.display());
    }
}

#[test]
fn extract_writes_version_file_matching_crate_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    extract_to(dir.path(), false).expect("extract");

    let version_path = dir.path().join(VERSION_FILE);
    let content = fs::read_to_string(&version_path).expect("read version file");
    assert_eq!(content.trim(), SOURCES_VERSION);
}

#[test]
fn read_installed_version_returns_none_for_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let observed = read_installed_version(dir.path()).expect("read");
    assert!(
        observed.is_none(),
        "unexpected version in empty dir: {observed:?}"
    );
}

#[test]
fn read_installed_version_returns_written_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    extract_to(dir.path(), false).expect("extract");
    let observed = read_installed_version(dir.path()).expect("read");
    assert_eq!(observed.as_deref(), Some(SOURCES_VERSION));
}

#[test]
fn extract_refuses_non_empty_destination_without_force() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("stray.txt"), b"pre-existing").expect("seed");
    let err = extract_to(dir.path(), false).expect_err("should refuse");
    match err {
        ExtractError::DestinationNotEmpty(p) => assert_eq!(p, dir.path().to_path_buf()),
        other => panic!("wrong error: {other:?}"),
    }
    // Original file left untouched — the failure must be a no-op.
    let content = fs::read_to_string(dir.path().join("stray.txt")).expect("read");
    assert_eq!(content, "pre-existing");
}

#[test]
fn extract_with_force_backs_up_existing_contents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join("runner");
    fs::create_dir_all(&dst).expect("mkdir");
    fs::write(dst.join("stale.txt"), b"stale content").expect("seed");

    let report = extract_to(&dst, true).expect("extract force");

    let backup = report.backup.expect("backup path present");
    assert!(backup.exists(), "backup should exist: {}", backup.display());
    let backed_up = fs::read_to_string(backup.join("stale.txt")).expect("read backup");
    assert_eq!(backed_up, "stale content");
    // Destination now contains the fresh extract, not the stale file.
    assert!(
        !dst.join("stale.txt").exists(),
        "stale file must not leak into fresh extract"
    );
    assert!(
        dst.join("Package.swift").exists(),
        "fresh extract must land"
    );
}

#[test]
fn extract_carries_over_xcframework_from_backup() {
    // Regression: the xcframework is a 13 MB binary that is
    // intentionally excluded from the shipped tarball (fetched or
    // built separately). But on auto-sync, a consumer who already
    // had a working xcframework in the pre-extract tree must not
    // lose it — otherwise the very next `smix runner up` fails at
    // Swift Package Graph resolve time with `binary target ... does
    // not contain a binary artifact`. Caught on real-sim during
    // v1.0.10 §D8 validation.
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join("runner");
    fs::create_dir_all(&dst).expect("mkdir");
    fs::write(dst.join("stale.txt"), b"stale").expect("seed source file");
    let xcf = dst.join("SmixCoreFFI.xcframework");
    fs::create_dir_all(xcf.join("ios-arm64-simulator")).expect("mkdir xcf");
    fs::write(xcf.join("Info.plist"), b"<?xml version=\"1.0\"?>").expect("seed xcf");
    fs::write(
        xcf.join("ios-arm64-simulator/marker.bin"),
        b"binary-artifact-marker",
    )
    .expect("seed xcf leaf");

    let report = smix_runner_sources::extract_to(&dst, true).expect("extract");
    assert!(
        report.carried_xcframework_from.is_some(),
        "must carry xcframework"
    );
    let carried = dst.join("SmixCoreFFI.xcframework");
    assert!(
        carried.exists(),
        "xcframework directory must be in new tree"
    );
    assert!(
        carried.join("Info.plist").exists(),
        "xcframework Info.plist must survive"
    );
    let leaf = carried.join("ios-arm64-simulator/marker.bin");
    assert!(
        leaf.exists(),
        "xcframework leaf file must survive: {}",
        leaf.display()
    );
    assert_eq!(
        fs::read(&leaf).unwrap(),
        b"binary-artifact-marker",
        "carried xcframework must be byte-identical"
    );
    assert!(
        dst.join("Package.swift").exists(),
        "fresh tarball contents must still land"
    );
}

#[test]
fn extract_excludes_xcframework_binary() {
    // The xcframework is 13MB; if the exclude regressed, the tarball
    // bloats and consumers pay download cost.
    let dir = tempfile::tempdir().expect("tempdir");
    extract_to(dir.path(), false).expect("extract");
    let excluded = dir.path().join("SmixCoreFFI.xcframework");
    assert!(
        !excluded.exists(),
        "xcframework must NOT be in the tarball (it's a 13MB binary; \
         must be fetched separately at consumer install time)"
    );
}
