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

use smix_runner_sources::{extract_android_to, extract_to};

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

    // `Package.swift` is deliberately not the workspace's.
    //
    // The workspace manifest declares the SDK, the UniFFI bindings and a
    // `.binaryTarget` pointing at SmixCoreFFI.xcframework — 49 MB that
    // this archive excludes on purpose. SwiftPM resolves the whole graph
    // before building, so shipping that declaration without the file
    // stopped `runner up` on every machine except the one whose earlier
    // builds had left an xcframework behind. CI found it on the first
    // push.
    //
    // `scripts/release/runner-package-manifest.py` emits the manifest
    // the runner actually builds, and the packaging script stages it in.
    // Comparing it against the workspace copy would fail by design, so
    // it is excused here — and only it. The rest of the tree must still
    // match, which is what this test is for: a runner built from sources
    // older than the repository reports success while testing something
    // that is gone.
    let manifest = Path::new("Package.swift");
    let embedded_manifest = embedded.remove(manifest);
    assert!(
        embedded_manifest.is_some(),
        "the archive has no Package.swift. An earlier attempt at this \
         excluded the workspace copy and appended the trimmed one with a \
         second `-C`; bsdtar took the exclude and dropped the append, and \
         the archive shipped with no manifest at all."
    );
    on_disk.remove(manifest);

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

/// Mirrors the excludes in `scripts/release/build-android-runner-tarball.sh`.
fn is_excluded_android(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "sdk"
            || s == "scripts"
            || s == "build"
            || s == ".gradle"
            || s == ".kotlin"
            || s == ".idea"
            || s == "local.properties"
            || s == ".DS_Store"
    })
}

fn collect_with(
    root: &Path,
    base: &Path,
    excluded: &dyn Fn(&Path) -> bool,
    out: &mut BTreeMap<PathBuf, Vec<u8>>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(base) else {
            continue;
        };
        if excluded(rel) {
            continue;
        }
        if path.is_dir() {
            collect_with(&path, base, excluded, out);
        } else if let Ok(bytes) = fs::read(&path) {
            out.insert(rel.to_path_buf(), bytes);
        }
    }
}

/// The same gate for Android, for the same reason.
///
/// Android's runner sources only started shipping in 2.4.0 — before
/// that `runner up --platform android` told the caller to `cd
/// android-runner`, a directory that exists in this repository and
/// nowhere on a machine that merely installed smix. Now that the
/// project ships, it can go stale exactly the way the Swift one did,
/// and silently: the Kotlin edited here would be tested here while the
/// device ran the tarball's copy.
#[test]
fn the_embedded_android_tarball_matches_the_kotlin_sources_in_this_repository() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../android-runner")
        .canonicalize()
        .expect("android-runner must exist next to the crate");

    let dir = tempfile::tempdir().expect("tempdir");
    extract_android_to(dir.path()).expect("extract");

    let mut on_disk = BTreeMap::new();
    collect_with(&repo, &repo, &is_excluded_android, &mut on_disk);
    let mut embedded = BTreeMap::new();
    collect_with(dir.path(), dir.path(), &is_excluded_android, &mut embedded);
    embedded.remove(Path::new(smix_runner_sources::ANDROID_VERSION_FILE));

    // The one file that is *meant* to differ: the shipped tree drops
    // `include(":sdk")`, since `:sdk` is not carried. Compare it modulo
    // that line rather than exempting it — a settings file that lost
    // its repository declarations would still build nothing.
    let settings = Path::new("settings.gradle.kts");
    if let (Some(want), Some(got)) = (on_disk.get(settings), embedded.get(settings)) {
        let strip = |b: &Vec<u8>| {
            String::from_utf8_lossy(b)
                .lines()
                .filter(|l| !l.starts_with("include(\":sdk\")"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            strip(want),
            strip(got),
            "the shipped settings.gradle.kts differs by more than the :sdk include"
        );
        let e = embedded.remove(settings).expect("present");
        on_disk.insert(settings.to_path_buf(), e);
        embedded.insert(settings.to_path_buf(), on_disk[settings].clone());
    }

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
        "the Android runner sources shipped to consumers are not the ones in \
         this repository. Run `scripts/release/build-android-runner-tarball.sh`.\n{}",
        drifted.join("\n")
    );
}
