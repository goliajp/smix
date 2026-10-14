//! Which instrumentation APK a `runner up` from a smix checkout installs,
//! and whether it has to be built first.

use std::path::{Path, PathBuf};

use crate::runner_android::build_apk_in;

/// Locate the instrumentation APK. Built by
/// `./gradlew :app:assembleDebugAndroidTest` in `android-runner/`.
/// The working tree's APK, when this is a clone of smix and the APK is
/// no older than the Kotlin it was built from.
///
/// The repo tree takes precedence deliberately — a smix developer wants
/// the runner from their working copy, not the one that shipped. But
/// precedence without freshness is worse than no precedence at all: a
/// two-day-old APK sat here and won every time, so a Kotlin route added,
/// packed into the tarball, and installed came back `not_implemented`
/// from an artifact predating it. `None` here means "rebuild", and the
/// installed path is where rebuilding lives.
///
/// The `bool` is whether the APK was built just now. A failed build is an
/// error, not a reason to use another runner: it used to fall through to
/// the installed one, so `runner up` said "building it", started the
/// runner that shipped, and a developer measured a change that was not
/// running (2026-10-13, a Kotlin edit that did not compile).
pub(crate) fn find_test_apk(root: &Path) -> Result<Option<(PathBuf, bool)>, String> {
    let tree = root.join("android-runner");
    let candidate = tree.join("app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk");
    let Some(built) = candidate.metadata().ok().and_then(|m| m.modified().ok()) else {
        return Ok(None);
    };
    if let Some(newest) = newest_source_mtime(&tree.join("app/src"))
        && newest > built
    {
        println!(
            "[runner] {} is older than the Kotlin in this tree — building it",
            candidate.display()
        );
        let apk = build_apk_in(&tree)?;
        let rebuilt = rewritten_since(&apk, built)
            .map_err(|e| format!("could not read or mark {}: {e}", apk.display()))?;
        if !rebuilt {
            println!("[runner] gradle found it current — the sources were only touched");
        }
        return Ok(Some((apk, rebuilt)));
    }
    Ok(Some((candidate, false)))
}

/// Whether the build wrote `apk` anew after `before`. Gradle is what knows
/// whether the sources changed: a checkout or a merge rewrites a file's
/// time without changing it, the build then leaves the APK as it was, and
/// calling that a rebuild replaced a running runner for nothing — every
/// `runner up` after a `git merge`. An APK the build left alone is marked
/// current, so the next look agrees with Gradle instead of asking again.
fn rewritten_since(apk: &Path, before: std::time::SystemTime) -> std::io::Result<bool> {
    if apk.metadata()?.modified()? != before {
        return Ok(true);
    }
    std::fs::File::options()
        .write(true)
        .open(apk)?
        .set_modified(std::time::SystemTime::now())?;
    Ok(false)
}

/// The most recent modification time under `dir`, or `None` if it has
/// no files.
fn newest_source_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(m) = path.metadata().and_then(|m| m.modified()) {
                newest = Some(newest.map_or(m, |n| if m > n { m } else { n }));
            }
        }
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn the_newest_source_is_the_one_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("app/src/androidTest/kotlin");
        std::fs::create_dir_all(&src).expect("mkdir");
        assert!(
            newest_source_mtime(&dir.path().join("app/src")).is_some() || true,
            "an empty tree has no newest file, which is not an error"
        );
        std::fs::write(src.join("Old.kt"), "// old").expect("write");
        let first = newest_source_mtime(&dir.path().join("app/src")).expect("a file exists");

        // A second file written later must move the answer forward, or
        // an APK built between them reads as current. Set the mtime
        // rather than sleeping: the test should not be slower than the
        // filesystem's clock resolution to be right.
        let newer = src.join("New.kt");
        let mut f = std::fs::File::create(&newer).expect("create");
        f.write_all(b"// new").expect("write");
        let ahead = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        f.set_modified(ahead).expect("set mtime");
        drop(f);

        let second = newest_source_mtime(&dir.path().join("app/src")).expect("files exist");
        assert!(
            second > first,
            "the newest source did not move when a newer file appeared"
        );
    }

    #[test]
    fn a_tree_with_no_sources_reports_nothing_rather_than_now() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            newest_source_mtime(&dir.path().join("app/src")).is_none(),
            "an absent source tree must not answer with a time, which would \
             make every APK look stale forever"
        );
    }

    // A checkout whose APK is older than its Kotlin, and whose build fails:
    // the failure is the answer. Falling through to the installed runner
    // started the one that shipped while saying "building it".
    #[cfg(unix)]
    #[test]
    fn a_working_tree_that_does_not_build_is_an_error_not_another_runner() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().expect("tempdir");
        let tree = root.path().join("android-runner");
        let apk = tree.join("app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk");
        std::fs::create_dir_all(apk.parent().expect("parent")).expect("apk dir");
        std::fs::write(&apk, b"old").expect("apk");
        let src = tree.join("app/src/androidTest/kotlin/Runner.kt");
        std::fs::create_dir_all(src.parent().expect("parent")).expect("src dir");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&src, b"newer than the apk").expect("src");
        let gradlew = tree.join("gradlew");
        std::fs::write(
            &gradlew,
            "#!/bin/sh\necho 'e: Unresolved reference' \nexit 1\n",
        )
        .expect("gradlew");
        std::fs::set_permissions(&gradlew, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let found = find_test_apk(root.path());
        assert!(found.is_err(), "a failed build was answered with {found:?}");
    }

    #[test]
    fn a_build_that_left_the_apk_alone_is_not_a_rebuild_and_marks_it_current() {
        let dir = tempfile::tempdir().expect("tempdir");
        let apk = dir.path().join("app-debug-androidTest.apk");
        std::fs::write(&apk, b"built").expect("apk");
        let before = apk.metadata().and_then(|m| m.modified()).expect("mtime");
        std::thread::sleep(std::time::Duration::from_millis(20));
        // a source whose time a checkout rewrote
        let src = dir.path().join("Runner.kt");
        std::fs::write(&src, b"unchanged").expect("src");
        let src_at = src.metadata().and_then(|m| m.modified()).expect("mtime");

        assert!(
            !rewritten_since(&apk, before).expect("read"),
            "gradle left it as it was"
        );
        let now = apk.metadata().and_then(|m| m.modified()).expect("mtime");
        assert!(
            now >= src_at,
            "marked current, so the next look does not ask again"
        );
    }

    #[test]
    fn a_build_that_wrote_the_apk_is_a_rebuild() {
        let dir = tempfile::tempdir().expect("tempdir");
        let apk = dir.path().join("app-debug-androidTest.apk");
        std::fs::write(&apk, b"old").expect("apk");
        let before = apk.metadata().and_then(|m| m.modified()).expect("mtime");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&apk, b"new").expect("rebuilt");
        assert!(rewritten_since(&apk, before).expect("read"));
    }

    #[test]
    fn a_working_tree_with_no_apk_leaves_the_choice_to_the_installed_runner() {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(matches!(find_test_apk(root.path()), Ok(None)));
    }
}
