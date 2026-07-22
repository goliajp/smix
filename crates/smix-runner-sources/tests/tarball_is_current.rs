//! The embedded tarball must match `swift-bridge/`.
//!
//! `smix runner up` extracts this tarball onto the consumer's machine
//! and builds it — so the Swift the consumer runs is whatever was in
//! the tarball when the binary was compiled, not whatever is in this
//! repository. Nothing regenerates it automatically;
//! `scripts/release/build-runner-tarball.sh` is run by hand.
//!
//! That is the whole of the v1.0.10 cycle's root cause, and it stayed
//! live afterwards: the script's own header says a ship gate compares
//! its SHA256, and no such gate existed. A Swift route was edited,
//! tested, and driven against a device in this repository while the
//! runner on that device ran the previous version and said nothing.
//!
//! So the gate is here, in `cargo test --workspace`, where it cannot be
//! skipped by whoever forgets.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use smix_runner_sources::extract_to;

/// Mirrors the excludes in `scripts/release/build-runner-tarball.sh`.
///
/// A path excluded from the tarball is not drift when it differs — it
/// was never meant to be there.
fn is_excluded(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "SmixCoreFFI.xcframework"
            || s == "SmixCoreFFI.xcframework.zip"
            || s == "SmixCoreFFI.xcframework.zip.sha256"
            || s == ".swiftpm"
            || s == "DerivedData"
            || s == "xcuserdata"
            || s == ".build"
            || s == ".DS_Store"
            || s == "__MACOSX"
            || s.starts_with(".bak-")
    })
}

fn collect(root: &Path, base: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(base) else {
            continue;
        };
        if is_excluded(rel) {
            continue;
        }
        if path.is_dir() {
            collect(&path, base, out);
        } else if let Ok(bytes) = fs::read(&path) {
            out.insert(rel.to_path_buf(), bytes);
        }
    }
}

#[test]
fn the_embedded_tarball_matches_the_swift_sources_in_this_repository() {
    let repo_swift = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../swift-bridge")
        .canonicalize()
        .expect("swift-bridge must exist next to the crate");

    let dir = tempfile::tempdir().expect("tempdir");
    extract_to(dir.path(), false).expect("extract");

    let mut on_disk = BTreeMap::new();
    collect(&repo_swift, &repo_swift, &mut on_disk);
    let mut embedded = BTreeMap::new();
    collect(dir.path(), dir.path(), &mut embedded);
    // `extract_to` writes its own version stamp, which has no
    // counterpart in the source tree.
    embedded.remove(Path::new(smix_runner_sources::VERSION_FILE));

    let mut drifted: Vec<String> = Vec::new();
    for (rel, want) in &on_disk {
        match embedded.get(rel) {
            None => drifted.push(format!("missing from tarball: {}", rel.display())),
            Some(got) if got != want => {
                drifted.push(format!("stale in tarball: {}", rel.display()));
            }
            Some(_) => {}
        }
    }
    for rel in embedded.keys() {
        if !on_disk.contains_key(rel) {
            drifted.push(format!(
                "deleted from the repo but still shipped: {}",
                rel.display()
            ));
        }
    }

    assert!(
        drifted.is_empty(),
        "the runner sources shipped to consumers are not the ones in this \
         repository, so `smix runner up` would build the old Swift and \
         report success. Run `scripts/release/build-runner-tarball.sh`.\n{}",
        drifted.join("\n")
    );
}
