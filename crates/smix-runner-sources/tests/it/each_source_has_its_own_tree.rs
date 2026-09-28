//! Each set of runner sources has a directory of its own, and a directory
//! that holds one set is never replaced by another.
//!
//! C9g made a sync swap in a whole tree instead of unpacking over the old
//! one, so a directory was always one version. It left one race (AD2): a
//! binary built from other sources swapped the directory while a gradle
//! build was running in it, and that build then looked for its APK by path
//! and found the other version's. Two binaries on one machine is ordinary —
//! an installed release beside the one a checkout builds — so the directory
//! is named after the sources, and two sets never share one.

use std::path::{Path, PathBuf};

use smix_runner_sources::{
    ANDROID_VERSION_FILE, RunnerPlatform, VERSION_FILE, android_version_stamp, ensure_tree,
    tree_dir, version_stamp,
};

const PLATFORMS: [RunnerPlatform; 2] = [RunnerPlatform::Ios, RunnerPlatform::Android];

fn stamp_file(p: RunnerPlatform) -> &'static str {
    match p {
        RunnerPlatform::Ios => VERSION_FILE,
        RunnerPlatform::Android => ANDROID_VERSION_FILE,
    }
}

fn stamp(p: RunnerPlatform) -> String {
    match p {
        RunnerPlatform::Ios => version_stamp(),
        RunnerPlatform::Android => android_version_stamp(),
    }
}

/// A sibling tree as another binary would have left it.
fn plant_sibling(root: &Path, p: RunnerPlatform, name: &str) -> PathBuf {
    let dir = tree_dir(root, p).with_file_name(name);
    std::fs::create_dir_all(dir.join("src")).expect("mkdir sibling");
    std::fs::write(dir.join("src/Other.kt"), "other version").expect("write sibling");
    std::fs::write(dir.join(stamp_file(p)), "0.0.0 0000000000000000\n").expect("stamp sibling");
    dir
}

#[test]
fn the_directory_is_named_after_the_sources() {
    let root = Path::new("machine-root");
    for p in PLATFORMS {
        let dir = tree_dir(root, p);
        let name = dir
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .to_string();
        let expected = stamp(p).replace(' ', "-");
        assert_eq!(name, expected, "{p:?}");
        assert!(
            dir.starts_with(root.join("runner-sources")),
            "{p:?}: {}",
            dir.display()
        );
        assert_ne!(
            tree_dir(root, RunnerPlatform::Ios).parent(),
            tree_dir(root, RunnerPlatform::Android).parent(),
            "the two platforms share a parent"
        );
    }
}

#[test]
fn another_sources_tree_is_left_as_it_was() {
    for p in PLATFORMS {
        let root = tempfile::tempdir().expect("tempdir");
        let other = plant_sibling(root.path(), p, "0.0.0-0000000000000000");
        let got = ensure_tree(root.path(), p).expect("ensure");
        assert!(got.extracted, "{p:?}: the first ensure extracts");
        assert_eq!(got.dir, tree_dir(root.path(), p));
        assert_eq!(
            std::fs::read_to_string(got.dir.join(stamp_file(p)))
                .expect("stamp")
                .trim(),
            stamp(p)
        );
        assert_eq!(
            std::fs::read_to_string(other.join("src/Other.kt")).expect("sibling intact"),
            "other version",
            "{p:?}: the other sources' tree was touched"
        );
    }
}

#[test]
fn the_same_sources_are_extracted_once() {
    for p in PLATFORMS {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(ensure_tree(root.path(), p).expect("first").extracted);
        // Something a build left in the tree: a second ensure must not
        // replace the tree it sits in.
        let built = tree_dir(root.path(), p).join("build/marker");
        std::fs::create_dir_all(built.parent().expect("parent")).expect("mkdir");
        std::fs::write(&built, "built").expect("write");
        assert!(
            !ensure_tree(root.path(), p).expect("second").extracted,
            "{p:?}"
        );
        assert!(built.is_file(), "{p:?}: a second ensure replaced the tree");
    }
}

#[test]
fn a_directory_whose_stamp_is_not_these_sources_is_replaced() {
    for p in PLATFORMS {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = tree_dir(root.path(), p);
        std::fs::create_dir_all(dir.join("src")).expect("mkdir");
        std::fs::write(dir.join("src/Stray.kt"), "stray").expect("write");
        assert!(
            ensure_tree(root.path(), p).expect("ensure").extracted,
            "{p:?}"
        );
        assert!(
            !dir.join("src/Stray.kt").exists(),
            "{p:?}: a stray file survived"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(stamp_file(p)))
                .expect("stamp")
                .trim(),
            stamp(p)
        );
    }
}

#[test]
fn two_ensures_at_once_leave_one_whole_tree() {
    for p in PLATFORMS {
        let root = tempfile::tempdir().expect("tempdir");
        let r = root.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let r = r.clone();
                std::thread::spawn(move || ensure_tree(&r, p).expect("ensure"))
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("join"))
            .collect();
        let extracted = results.iter().filter(|e| e.extracted).count();
        assert!(extracted >= 1, "{p:?}: nobody extracted");
        let dir = tree_dir(&r, p);
        assert_eq!(
            std::fs::read_to_string(dir.join(stamp_file(p)))
                .expect("stamp")
                .trim(),
            stamp(p),
            "{p:?}"
        );
        let beside: Vec<String> = std::fs::read_dir(dir.parent().expect("parent"))
            .expect("read parent")
            .map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with('.'))
            .collect();
        assert!(
            beside.is_empty(),
            "{p:?}: scratch left beside the tree: {beside:?}"
        );
    }
}

fn age(dir: &Path, secs: u64) {
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(secs);
    let f = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(smix_runner_sources::LAST_USED_FILE))
        .expect("open last-used");
    f.set_modified(t).expect("set mtime");
}

#[test]
fn old_trees_are_pruned_and_recent_ones_kept() {
    for p in PLATFORMS {
        let root = tempfile::tempdir().expect("tempdir");
        // Four other sets, used 5, 4, 3 and 2 hours ago, and one ten
        // minutes ago.
        let mut olds = Vec::new();
        for (i, hours) in [5u64, 4, 3, 2].iter().enumerate() {
            let d = plant_sibling(root.path(), p, &format!("0.0.{i}-000000000000000{i}"));
            age(&d, hours * 3600);
            olds.push(d);
        }
        let recent = plant_sibling(root.path(), p, "0.0.9-0000000000000009");
        age(&recent, 600);
        let got = ensure_tree(root.path(), p).expect("ensure");
        // Kept: this one, the newest BACKUPS_KEPT others by last use, and
        // anything used within the hour whatever the count.
        assert!(
            recent.is_dir(),
            "{p:?}: a tree used ten minutes ago was pruned"
        );
        assert!(olds[3].is_dir(), "{p:?}: the newest older tree was pruned");
        // BACKUPS_KEPT is 2: the ten-minute one and the two-hour one.
        assert_eq!(smix_runner_sources::BACKUPS_KEPT, 2);
        for old in &olds[..3] {
            assert!(!old.exists(), "{p:?}: {} was kept", old.display());
        }
        let mut pruned = got.pruned.clone();
        pruned.sort();
        let mut want = olds[..3].to_vec();
        want.sort();
        assert_eq!(pruned, want, "{p:?}");
        // Within the hour, kept even past the count: a build may be running
        // in it.
        let busy = plant_sibling(root.path(), p, "0.0.8-0000000000000008");
        age(&busy, 1200);
        let busy2 = plant_sibling(root.path(), p, "0.0.7-0000000000000007");
        age(&busy2, 1800);
        ensure_tree(root.path(), p).expect("again");
        assert!(
            busy.is_dir() && busy2.is_dir() && recent.is_dir(),
            "{p:?}: a tree used within the hour was pruned"
        );
        assert!(
            !olds[3].exists(),
            "{p:?}: past the count and past the hour, kept"
        );
        // Never a directory that is not a tree name.
        let foreign = tree_dir(root.path(), p).with_file_name("notes");
        std::fs::create_dir_all(&foreign).expect("mkdir");
        ensure_tree(root.path(), p).expect("again");
        assert!(
            foreign.is_dir(),
            "{p:?}: a directory that is not a tree was pruned"
        );
    }
}

#[test]
fn the_old_layout_is_not_touched() {
    // `runner/` and `android-runner/` are where 10.x and 11.0 builds put
    // their trees, and an installed release on this machine still uses
    // them. Nothing here reads, moves or removes them.
    let root = tempfile::tempdir().expect("tempdir");
    for (dir, file) in [
        ("runner", "Package.swift"),
        ("android-runner", "settings.gradle.kts"),
    ] {
        std::fs::create_dir_all(root.path().join(dir)).expect("mkdir");
        std::fs::write(root.path().join(dir).join(file), "old").expect("write");
        std::fs::create_dir_all(root.path().join(format!("{dir}.bak-1"))).expect("mkdir bak");
    }
    for p in PLATFORMS {
        ensure_tree(root.path(), p).expect("ensure");
    }
    for (dir, file) in [
        ("runner", "Package.swift"),
        ("android-runner", "settings.gradle.kts"),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.path().join(dir).join(file)).expect("old file"),
            "old"
        );
        assert!(root.path().join(format!("{dir}.bak-1")).is_dir());
    }
}
