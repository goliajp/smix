//! Where the runner handle lives.
//!
//! One file, `.smix/runner/state.json`, had two writers: `runner.rs`
//! and `runner_android.rs`, each with its own copy of `state_path()`
//! pointing at the same path. Bringing an Android runner up in a
//! workspace that already had an iOS one replaced the iOS record, and
//! the Android write was `let _ = std::fs::write(...)` — so that
//! replacement could also fail without a word. `smix runner down` then
//! tore down whichever record had won.
//!
//! Now: one implementation, one key per platform. `down` with no
//! arguments still means iOS and `down --device` still means Android,
//! exactly as before — the platforms simply stop sharing a slot.

use std::path::Path;

use crate::runner::RunnerState;

/// Which runner a record belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// The XCUITest runner on an iOS Simulator.
    Ios,
    /// The UiAutomator runner on an Android emulator.
    Android,
}

impl Platform {
    /// The singleton this platform's record occupies. One runner per
    /// platform per workspace, which is what the CLI already assumes.
    fn key(self) -> &'static str {
        match self {
            Platform::Ios => "runner-ios",
            Platform::Android => "runner-android",
        }
    }
}

fn store_root(root: &Path) -> std::path::PathBuf {
    root.join(".smix")
}

fn open(root: &Path) -> Result<smix_store::Store, String> {
    smix_store::Store::open(&store_root(root))
        .map_err(|e| format!("open runner store under {}: {e}", root.display()))
}

/// Read this platform's runner record.
///
/// `Ok(None)` means no runner. A record that exists but cannot be
/// understood is an error naming it: the old `.ok()?` read turned a
/// damaged file into "no runner", and `up` would then start a second
/// runner beside the one already there.
pub fn read(root: &Path, platform: Platform) -> Result<Option<RunnerState>, String> {
    let store = open(root)?;
    if let Some(state) = store
        .singleton(platform.key())
        .get_json::<RunnerState>()
        .map_err(|e| format!("read runner state: {e}"))?
    {
        return Ok(Some(state));
    }
    // A pre-store workspace still has the file. Read it once, and leave
    // it where it is — downgrading has to keep working.
    read_legacy(root)
}

fn read_legacy(root: &Path) -> Result<Option<RunnerState>, String> {
    let legacy = root.join(".smix/runner/state.json");
    let text = match std::fs::read_to_string(&legacy) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", legacy.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{} is not a runner state: {e}", legacy.display()))
}

/// Write this platform's runner record.
pub fn write(root: &Path, platform: Platform, state: &RunnerState) -> Result<(), String> {
    let store = open(root)?;
    store
        .singleton(platform.key())
        .put_json(state)
        .map_err(|e| format!("write runner state: {e}"))?;
    // The runner outlives this process, so its handle has to be on disk
    // before we return — a crash between here and the next command
    // would otherwise orphan it.
    store
        .sync()
        .map_err(|e| format!("persist runner state: {e}"))
}

/// Forget this platform's runner record.
pub fn clear(root: &Path, platform: Platform) -> Result<(), String> {
    let store = open(root)?;
    store
        .singleton(platform.key())
        .delete()
        .map_err(|e| format!("clear runner state: {e}"))?;
    store
        .sync()
        .map_err(|e| format!("persist runner state: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("smix-runner-state-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn state(udid: &str, port: u16) -> RunnerState {
        RunnerState {
            pid: 4242,
            udid: udid.to_string(),
            port,
            log: std::path::PathBuf::from("/tmp/runner.log"),
            bundle: Some("com.example.app".to_string()),
            supervisor_pid: None,
        }
    }

    #[test]
    fn state_round_trips_through_the_store() {
        let root = temp_root("roundtrip");
        write(&root, Platform::Ios, &state("UDID-1", 22087)).expect("write");
        let back = read(&root, Platform::Ios).expect("read").expect("present");
        assert_eq!(back.udid, "UDID-1");
        assert_eq!(back.port, 22087);
    }

    #[test]
    fn the_legacy_state_file_is_not_written() {
        let root = temp_root("no-file");
        write(&root, Platform::Ios, &state("UDID-1", 22087)).expect("write");
        assert!(
            !root.join(".smix/runner/state.json").exists(),
            "the legacy state.json is still being written"
        );
    }

    #[test]
    fn ios_and_android_do_not_overwrite_each_other() {
        let root = temp_root("two-platforms");
        write(&root, Platform::Ios, &state("IOS-UDID", 22087)).expect("write ios");
        write(&root, Platform::Android, &state("emulator-5554", 28080)).expect("write android");

        let ios = read(&root, Platform::Ios).expect("read").expect("present");
        let android = read(&root, Platform::Android)
            .expect("read")
            .expect("present");
        assert_eq!(
            ios.udid, "IOS-UDID",
            "the Android runner replaced the iOS record"
        );
        assert_eq!(android.udid, "emulator-5554");
    }

    #[test]
    fn an_absent_state_is_none_not_an_error() {
        let root = temp_root("absent");
        assert!(read(&root, Platform::Ios).expect("no error").is_none());
    }

    #[test]
    fn clearing_removes_only_that_platform() {
        let root = temp_root("clear");
        write(&root, Platform::Ios, &state("IOS", 22087)).expect("write");
        write(&root, Platform::Android, &state("droid", 28080)).expect("write");
        clear(&root, Platform::Ios).expect("clear");
        assert!(read(&root, Platform::Ios).expect("read").is_none());
        assert!(
            read(&root, Platform::Android).expect("read").is_some(),
            "clearing iOS took Android with it"
        );
    }

    #[test]
    fn a_pre_store_workspace_still_finds_its_runner() {
        let root = temp_root("legacy");
        let dir = root.join(".smix/runner");
        std::fs::create_dir_all(&dir).expect("dirs");
        std::fs::write(
            dir.join("state.json"),
            serde_json::to_string(&state("LEGACY-UDID", 22087)).expect("json"),
        )
        .expect("write legacy");

        let back = read(&root, Platform::Ios).expect("read").expect("present");
        assert_eq!(back.udid, "LEGACY-UDID");
        assert!(
            root.join(".smix/runner/state.json").exists(),
            "the legacy file must be left where it is"
        );
    }

    #[test]
    fn a_corrupt_stored_state_is_named_not_silently_absent() {
        let root = temp_root("corrupt");
        {
            let store = smix_store::Store::open(&root.join(".smix")).expect("open");
            store
                .singleton("runner-ios")
                .put_json(&"not a runner state")
                .expect("put");
        }
        let err = read(&root, Platform::Ios).expect_err("must not read as absent");
        assert!(
            err.contains("runner-ios"),
            "the error must name what it could not read: {err}"
        );
    }
}
