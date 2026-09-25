//! An extracted runner tree holds one version's sources, never two.
//!
//! Reported 2026-09-25 by a consumer on 10.1.0: `runner up --platform
//! android` failed compiling `ProbeTarget.kt` against a `WindowRules` that
//! was not in the tree. `ProbeTarget.kt` came from another build's
//! extraction into the same machine directory; the Android extract unpacked
//! over whatever was there and never removed a file, so the directory was
//! the union of two versions — and its stamp, written at the end of the
//! second unpack, said it was one of them.

use smix_runner_sources::{ANDROID_VERSION_FILE, VERSION_FILE, extract_android_to, extract_to};

/// A file no shipped version has: whatever put it there, it is not these
/// sources.
const LEFT_BEHIND: &str = "app/src/main/kotlin/dev/smix/runner/LeftBehind.kt";

fn plant(dir: &std::path::Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().expect("a planted file has a parent"))
        .expect("create the planted file's directory");
    std::fs::write(p, body).expect("write the planted file");
}

#[test]
fn an_android_tree_keeps_no_file_another_version_left() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("android-runner");
    plant(&dir, LEFT_BEHIND, "class LeftBehind");
    plant(&dir, ANDROID_VERSION_FILE, "0.0.0 0000000000000000\n");

    assert!(
        extract_android_to(&dir).expect("extract"),
        "a stale stamp must extract"
    );

    assert!(
        !dir.join(LEFT_BEHIND).exists(),
        "a source file from another extraction is still in the tree, so it \
         compiles as part of this version"
    );
    assert!(
        dir.join("app/build.gradle.kts").is_file(),
        "the tree this version ships was not put in place"
    );
}

#[test]
fn an_android_tree_keeps_its_build_output() {
    // The build is the expensive part and gradle decides for itself what
    // in it is out of date — the same as checking out another commit in a
    // working tree. Throwing it away would cost every upgrade a cold build.
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("android-runner");
    plant(&dir, "app/build/outputs/marker", "built");
    plant(&dir, ".gradle/marker", "cache");
    plant(&dir, ANDROID_VERSION_FILE, "0.0.0 0000000000000000\n");

    extract_android_to(&dir).expect("extract");

    assert!(
        dir.join("app/build/outputs/marker").is_file(),
        "app/build was dropped"
    );
    assert!(dir.join(".gradle/marker").is_file(), ".gradle was dropped");
}

#[test]
fn nothing_is_left_beside_the_tree_but_its_backups() {
    // The tree is built beside the destination and moved in. What is built
    // there must not outlive the move — a crash-free run leaves the
    // destination and the rotation's backups, and nothing else.
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("android-runner");
    plant(&dir, ANDROID_VERSION_FILE, "0.0.0 0000000000000000\n");
    extract_android_to(&dir).expect("extract");
    plant(&dir, ANDROID_VERSION_FILE, "0.0.0 0000000000000000\n");
    extract_android_to(&dir).expect("extract again");

    let mut beside: Vec<String> = std::fs::read_dir(root.path())
        .expect("read the parent")
        .map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
        .filter(|n| n != "android-runner" && !n.starts_with("android-runner.bak-"))
        .collect();
    beside.sort();
    assert!(beside.is_empty(), "left beside the tree: {beside:?}");
}

#[test]
fn a_swift_tree_keeps_no_file_another_version_left() {
    // The Swift extract moves the old tree aside before unpacking, so a
    // sequential sync already starts from nothing. Asserted rather than
    // read off the code: the Android one looked just as reasonable.
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("runner");
    plant(
        &dir,
        "SmixRunnerCore/LeftBehind.swift",
        "struct LeftBehind {}",
    );
    plant(&dir, VERSION_FILE, "0.0.0 0000000000000000\n");

    extract_to(&dir, true).expect("extract");

    assert!(
        !dir.join("SmixRunnerCore/LeftBehind.swift").exists(),
        "a source file from another extraction is still in the Swift tree"
    );
}

#[test]
fn android_backups_are_rotated() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("android-runner");
    for _ in 0..5 {
        plant(&dir, ANDROID_VERSION_FILE, "0.0.0 0000000000000000\n");
        extract_android_to(&dir).expect("extract");
    }
    let backups = std::fs::read_dir(root.path())
        .expect("read the parent")
        .filter(|e| {
            e.as_ref()
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with("android-runner.bak-")
        })
        .count();
    assert_eq!(
        backups,
        smix_runner_sources::BACKUPS_KEPT,
        "five syncs should leave the rotation's number of backups"
    );
}
