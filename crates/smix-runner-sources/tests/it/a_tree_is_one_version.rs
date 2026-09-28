//! An extracted runner tree holds one version's sources, never two.
//!
//! Reported 2026-09-25 by a consumer on 10.1.0: `runner up --platform
//! android` failed compiling `ProbeTarget.kt` against a `WindowRules` that
//! was not in the tree. `ProbeTarget.kt` came from another build's
//! extraction into the same machine directory; the Android extract unpacked
//! over whatever was there and never removed a file, so the directory was
//! the union of two versions — and its stamp, written at the end of the
//! second unpack, said it was one of them.
//!
//! The Android half is `each_source_has_its_own_tree.rs` now: since AD2 a
//! set of sources has a directory of its own and nothing unpacks over
//! another's. What stays here is the explicit-path Swift extract
//! (`smix runner install <path>`), which still replaces one directory.

use smix_runner_sources::{VERSION_FILE, extract_to};

fn plant(dir: &std::path::Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().expect("a planted file has a parent"))
        .expect("create the planted file's directory");
    std::fs::write(p, body).expect("write the planted file");
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
