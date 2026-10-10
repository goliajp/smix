//! Which `adb` smix runs, and whether it can run at all.
//!
//! Every adb call in smix goes through [`adb_path`]. It used to be the first
//! `adb` on `PATH` at each of a dozen call sites, and on a Mac driven over
//! ssh that was Homebrew's: a binary carrying `com.apple.quarantine`, whose
//! first launch waits for Gatekeeper to ask someone who is not there. Every
//! adb child sat in `_dyld_start`, and a consumer's `smix run` waited 26
//! minutes on its first flow with nothing on the CPU. The SDK's own adb
//! carries no such attribute, and an SDK is what an Android emulator needs
//! anyway, so it comes first.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Where the adb in use was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbSource {
    /// `$ANDROID_HOME/platform-tools/adb`.
    AndroidHome,
    /// `$ANDROID_SDK_ROOT/platform-tools/adb`.
    AndroidSdkRoot,
    /// The SDK's usual place for this user: `~/Library/Android/sdk` on a
    /// Mac, `~/Android/Sdk` elsewhere.
    DefaultSdk,
    /// The first `adb` on `PATH`.
    Path,
}

impl std::fmt::Display for AdbSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AdbSource::AndroidHome => "$ANDROID_HOME",
            AdbSource::AndroidSdkRoot => "$ANDROID_SDK_ROOT",
            AdbSource::DefaultSdk => "the SDK's default location",
            AdbSource::Path => "PATH",
        })
    }
}

/// The Android SDK directories to look in, in order, each with what named
/// it. Pure: `env` is the environment to read.
#[must_use]
pub fn sdk_candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<(PathBuf, AdbSource)> {
    let mut out = Vec::new();
    for (var, source) in [
        ("ANDROID_HOME", AdbSource::AndroidHome),
        ("ANDROID_SDK_ROOT", AdbSource::AndroidSdkRoot),
    ] {
        if let Some(v) = env(var).filter(|v| !v.is_empty()) {
            out.push((PathBuf::from(v), source));
        }
    }
    if let Some(home) = env("HOME").filter(|h| !h.is_empty()) {
        let rel = if cfg!(target_os = "macos") {
            "Library/Android/sdk"
        } else {
            "Android/Sdk"
        };
        out.push((Path::new(&home).join(rel), AdbSource::DefaultSdk));
    }
    out
}

/// The SDK this machine has, if any: the first candidate that exists.
#[must_use]
pub fn sdk_root() -> Option<PathBuf> {
    sdk_candidates(&|k| std::env::var(k).ok())
        .into_iter()
        .map(|(p, _)| p)
        .find(|p| p.is_dir())
}

/// The adb to run and where it came from. Pure: `env` and `exists` stand
/// for the environment and the filesystem.
#[must_use]
pub fn resolve_adb(
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> (PathBuf, AdbSource) {
    sdk_candidates(env)
        .into_iter()
        .map(|(sdk, source)| (sdk.join("platform-tools").join("adb"), source))
        .find(|(adb, _)| exists(adb))
        .unwrap_or_else(|| (PathBuf::from("adb"), AdbSource::Path))
}

/// The adb smix runs, resolved once per process.
pub fn adb_path() -> &'static (PathBuf, AdbSource) {
    static ADB: OnceLock<(PathBuf, AdbSource)> = OnceLock::new();
    ADB.get_or_init(|| resolve_adb(&|k| std::env::var(k).ok(), &|p| p.is_file()))
}

/// A blocking `adb` command, for callers outside an async runtime. Fails
/// when this adb cannot start (see [`adb_usable`]).
pub fn std_command() -> Result<std::process::Command, String> {
    adb_usable()?;
    Ok(std::process::Command::new(&adb_path().0))
}

/// How long `adb version` may take before smix says this adb cannot start.
/// It answers in milliseconds from a binary that can.
const STARTS_WITHIN: Duration = Duration::from_secs(15);

/// Whether this adb starts at all, asked once per process with a time limit.
///
/// # Errors
///
/// What stopped it, in words, including a quarantined binary on a Mac.
pub fn adb_usable() -> Result<(), String> {
    static USABLE: OnceLock<Result<(), String>> = OnceLock::new();
    USABLE
        .get_or_init(|| {
            let (path, source) = adb_path();
            check_starts(path, *source, STARTS_WITHIN)
        })
        .clone()
}

fn check_starts(path: &Path, source: AdbSource, within: Duration) -> Result<(), String> {
    let mut child = match std::process::Command::new(path)
        .arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        // Absence is reported where adb is first used, as it always was.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(format!(
                "adb at {} (from {source}) could not be started: {e}",
                path.display()
            ));
        }
    };
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if began.elapsed() < within => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(did_not_start(path, source, within, quarantined(path)));
            }
        }
    }
}

/// The refusal for an adb that did not start. Pure.
#[must_use]
pub fn did_not_start(
    path: &Path,
    source: AdbSource,
    within: Duration,
    quarantined: bool,
) -> String {
    let mut s = format!(
        "adb at {} (from {source}) did not finish `adb version` within {} s, so no adb \
         command can be trusted to return.",
        path.display(),
        within.as_secs()
    );
    if quarantined {
        s.push_str(&format!(
            " It carries the com.apple.quarantine attribute: macOS holds its first launch \
             for a Gatekeeper check that, over ssh, has nobody to answer it. Remove the \
             attribute (`xattr -d com.apple.quarantine {}`) or point ANDROID_HOME at the \
             Android SDK, whose own adb is used first.",
            path.display()
        ));
    } else {
        s.push_str(" Check that it runs by hand: `adb version`.");
    }
    s
}

#[cfg(target_os = "macos")]
fn quarantined(path: &Path) -> bool {
    let resolved = if path.components().count() == 1 {
        which(path)
    } else {
        Some(path.to_path_buf())
    };
    resolved.is_some_and(|p| {
        std::process::Command::new("xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(&p)
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

#[cfg(not(target_os = "macos"))]
fn quarantined(_path: &Path) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn which(name: &Path) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn the_sdks_own_adb_comes_before_path() {
        let env = env_of(&[("HOME", "/Users/me")]);
        let (adb, source) = resolve_adb(&env, &|p| p.ends_with("platform-tools/adb"));
        let want = if cfg!(target_os = "macos") {
            "/Users/me/Library/Android/sdk/platform-tools/adb"
        } else {
            "/Users/me/Android/Sdk/platform-tools/adb"
        };
        assert_eq!(adb, PathBuf::from(want));
        assert_eq!(source, AdbSource::DefaultSdk);
    }

    #[test]
    fn android_home_comes_first_and_path_last() {
        let env = env_of(&[
            ("ANDROID_HOME", "/opt/sdk"),
            ("ANDROID_SDK_ROOT", "/x"),
            ("HOME", "/h"),
        ]);
        assert_eq!(
            resolve_adb(&env, &|_| true),
            (
                PathBuf::from("/opt/sdk/platform-tools/adb"),
                AdbSource::AndroidHome
            )
        );
        assert_eq!(
            resolve_adb(&env, &|_| false),
            (PathBuf::from("adb"), AdbSource::Path)
        );
        let only_root = env_of(&[("ANDROID_SDK_ROOT", "/x"), ("HOME", "/h")]);
        assert_eq!(
            resolve_adb(&only_root, &|p| p.starts_with("/x")),
            (
                PathBuf::from("/x/platform-tools/adb"),
                AdbSource::AndroidSdkRoot
            )
        );
    }

    #[test]
    fn an_empty_variable_names_no_sdk() {
        let env = env_of(&[("ANDROID_HOME", ""), ("HOME", "/h")]);
        assert!(
            sdk_candidates(&env)
                .iter()
                .all(|(_, s)| *s != AdbSource::AndroidHome)
        );
    }

    #[test]
    fn a_quarantined_adb_is_named_with_the_way_out() {
        let said = did_not_start(
            Path::new("/opt/homebrew/bin/adb"),
            AdbSource::Path,
            STARTS_WITHIN,
            true,
        );
        assert!(said.contains("com.apple.quarantine"), "{said}");
        assert!(
            said.contains("xattr -d com.apple.quarantine /opt/homebrew/bin/adb"),
            "{said}"
        );
        assert!(said.contains("ANDROID_HOME"), "{said}");
        let plain = did_not_start(Path::new("/x/adb"), AdbSource::Path, STARTS_WITHIN, false);
        assert!(!plain.contains("quarantine"), "{plain}");
    }

    #[test]
    fn a_binary_that_never_returns_is_stopped_at_the_limit() {
        // `yes` never finishes, as an adb held at its first launch does not.
        let began = Instant::now();
        let err = check_starts(
            Path::new("/usr/bin/yes"),
            AdbSource::Path,
            Duration::from_millis(300),
        )
        .expect_err("a child that does not finish is refused");
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        assert!(err.contains("did not finish"), "{err}");
    }
}
