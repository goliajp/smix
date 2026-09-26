//! One directory per set of runner sources, named after them.
//!
//! A sync used to replace one shared directory (`runner/`,
//! `android-runner/`). C9g made the replacement whole, so the directory
//! was always one version; what it could not stop was the replacement
//! itself landing under a build in progress. Two binaries built from
//! different sources are ordinary on one machine — an installed release
//! beside the one a checkout builds — and the second one's sync swapped
//! the directory out from under the first one's gradle build, which then
//! looked for its APK by path and found the other version's (AD2).
//!
//! Here the sources name the directory, `<version>-<digest>`, so two sets
//! never share one and an installed tree is never replaced by another
//! set. The old shared directories are left exactly as they are: older
//! releases on the same machine still use them.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::{
    ANDROID_SOURCES_TAR_GZ, ANDROID_VERSION_FILE, BACKUPS_KEPT, ExtractError, SOURCES_TAR_GZ,
    Staging, VERSION_FILE, android_version_stamp, is_occupied, unpack, version_stamp,
};

/// Which runner a tree is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerPlatform {
    /// The Swift / XCUITest runner.
    Ios,
    /// The Kotlin / instrumentation runner.
    Android,
}

/// Touched every time a tree is resolved; its mtime is the tree's last
/// use, and pruning reads nothing else. Not shipped in either archive.
pub const LAST_USED_FILE: &str = ".smix-last-used";

/// A tree used this recently is never pruned, whatever the count: a
/// build may be running in it. An hour is longer than any runner build
/// measured here (a cold gradle build is minutes).
const IN_USE_WITHIN: Duration = Duration::from_secs(3600);

impl RunnerPlatform {
    fn subdir(self) -> &'static str {
        match self {
            RunnerPlatform::Ios => "ios",
            RunnerPlatform::Android => "android",
        }
    }

    fn archive(self) -> &'static [u8] {
        match self {
            RunnerPlatform::Ios => SOURCES_TAR_GZ,
            RunnerPlatform::Android => ANDROID_SOURCES_TAR_GZ,
        }
    }

    fn stamp_file(self) -> &'static str {
        match self {
            RunnerPlatform::Ios => VERSION_FILE,
            RunnerPlatform::Android => ANDROID_VERSION_FILE,
        }
    }

    fn stamp(self) -> String {
        match self {
            RunnerPlatform::Ios => version_stamp(),
            RunnerPlatform::Android => android_version_stamp(),
        }
    }
}

/// Where the tree for the embedded sources lives under a machine
/// directory: `<root>/runner-sources/<ios|android>/<version>-<digest>`.
///
/// The name is the stamp with its space made a dash, so the directory and
/// the stamp inside it cannot name different sources.
#[must_use]
pub fn tree_dir(root: &Path, platform: RunnerPlatform) -> PathBuf {
    root.join("runner-sources")
        .join(platform.subdir())
        .join(platform.stamp().replace(' ', "-"))
}

/// What [`ensure_tree`] did.
#[derive(Debug, Clone)]
pub struct Ensured {
    /// The tree, holding exactly the embedded sources.
    pub dir: PathBuf,
    /// Whether this call put it there (as opposed to finding it).
    pub extracted: bool,
    /// Trees of other sources removed by the rotation, oldest first.
    pub pruned: Vec<PathBuf>,
}

/// Make sure the tree for the embedded sources exists under `root`, and
/// return it.
///
/// A tree whose stamp names these sources is used as it is — never
/// replaced, so a build running in it is never pulled out from under.
/// A directory at that path whose stamp does not (hand-edited, or left
/// by a crash before 11.0 made installs whole) is moved aside and
/// removed, then the tree is built beside the path and moved in whole.
/// When two callers race, one tree wins and both use it.
///
/// Afterwards trees of other sources are rotated: the [`BACKUPS_KEPT`]
/// most recently used are kept, and so is any used in the last hour.
///
/// # Errors
///
/// I/O failures building or moving the tree; a tree that another caller
/// put in place with a stamp that is not these sources.
pub fn ensure_tree(root: &Path, platform: RunnerPlatform) -> Result<Ensured, ExtractError> {
    let dir = tree_dir(root, platform);
    let parent = dir.parent().expect("tree_dir always has a parent");
    std::fs::create_dir_all(parent)
        .map_err(|e| ExtractError::io(format!("mkdir -p {}", parent.display()), e))?;
    let extracted = if holds(&dir, platform) {
        false
    } else {
        install(&dir, platform)?
    };
    touch_last_used(&dir)?;
    let pruned = prune(parent, &dir).map_err(|e| {
        ExtractError::io(
            format!(
                "runner sources are at {}, but rotating older trees beside it failed",
                dir.display()
            ),
            e,
        )
    })?;
    Ok(Ensured {
        dir,
        extracted,
        pruned,
    })
}

/// Whether `dir`'s stamp names the embedded sources.
fn holds(dir: &Path, platform: RunnerPlatform) -> bool {
    std::fs::read_to_string(dir.join(platform.stamp_file()))
        .is_ok_and(|s| s.trim() == platform.stamp())
}

/// Build the tree beside `dir` and move it in. Returns whether this call's
/// tree is the one in place (false when a racing caller's got there first).
fn install(dir: &Path, platform: RunnerPlatform) -> Result<bool, ExtractError> {
    if dir.exists() {
        discard(dir)?;
    }
    let staging = Staging::beside(dir)?;
    unpack(platform.archive(), staging.path())?;
    let stamp_path = staging.path().join(platform.stamp_file());
    std::fs::write(&stamp_path, format!("{}\n", platform.stamp()))
        .map_err(|e| ExtractError::io(format!("writing {}", stamp_path.display()), e))?;
    match std::fs::rename(staging.path(), dir) {
        Ok(()) => {
            staging.moved_in();
            Ok(true)
        }
        // Another caller moved its tree in first. Same sources, same
        // bytes: use theirs, and ours is removed with the staging.
        Err(e) if is_occupied(&e) && holds(dir, platform) => Ok(false),
        Err(e) => Err(ExtractError::io(
            format!("moving {} into {}", staging.path().display(), dir.display()),
            e,
        )),
    }
}

/// Move a directory that is not these sources out of the way, then remove
/// it. The move is what makes the path free at once; the removal of what
/// was moved can fail without costing the install.
fn discard(dir: &Path) -> Result<(), ExtractError> {
    let parent = dir.parent().expect("tree_dir always has a parent");
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let aside = parent.join(format!(".{name}.discard-{}-{nanos}", std::process::id()));
    match std::fs::rename(dir, &aside) {
        Ok(()) => {
            // Left behind if it fails: a dot-directory, pruned by nothing,
            // and not worth replacing the install's result with.
            let _ = std::fs::remove_dir_all(&aside);
            Ok(())
        }
        // Gone already — a racing caller discarded it.
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ExtractError::io(
            format!("moving {} aside", dir.display()),
            e,
        )),
    }
}

fn touch_last_used(dir: &Path) -> Result<(), ExtractError> {
    let path = dir.join(LAST_USED_FILE);
    let f = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| ExtractError::io(format!("opening {}", path.display()), e))?;
    f.set_modified(SystemTime::now())
        .map_err(|e| ExtractError::io(format!("touching {}", path.display()), e))
}

/// Whether a directory name is a tree name: `<version>-<16 hex digits>`.
/// Anything else in the parent is not ours to remove.
fn is_tree_name(name: &str) -> bool {
    let Some((version, digest)) = name.rsplit_once('-') else {
        return false;
    };
    !version.is_empty()
        && !version.starts_with('.')
        && digest.len() == 16
        && digest.bytes().all(|b| b.is_ascii_hexdigit())
}

/// When a tree was last used: its [`LAST_USED_FILE`]'s mtime, or the
/// directory's own when it has none.
fn last_used(dir: &Path) -> SystemTime {
    std::fs::metadata(dir.join(LAST_USED_FILE))
        .or_else(|_| std::fs::metadata(dir))
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Remove trees of other sources beyond the rotation. Returns what went,
/// oldest first.
fn prune(parent: &Path, current: &Path) -> io::Result<Vec<PathBuf>> {
    let mut others: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path == current || !is_tree_name(&name) || !path.is_dir() {
            continue;
        }
        others.push((last_used(&path), path));
    }
    // Newest first: the first BACKUPS_KEPT are kept.
    others.sort_by_key(|(used, _)| std::cmp::Reverse(*used));
    let now = SystemTime::now();
    let mut pruned = Vec::new();
    for (used, path) in others.into_iter().skip(BACKUPS_KEPT) {
        let recent = now
            .duration_since(used)
            .map_or(true, |age| age < IN_USE_WITHIN);
        if recent {
            continue;
        }
        std::fs::remove_dir_all(&path)?;
        pruned.push(path);
    }
    pruned.reverse();
    Ok(pruned)
}
