//! XCUITest's query log, kept out of the log of the app under test.

/// The subsystem XCUITest logs every query under. It logs from inside the
/// process whose tree it reads, so most of the app's own log was this: 400
/// of the fixture's 656 lines over 20 tree reads, and none once the
/// subsystem was off, with the same 20 reads answering.
const XCTEST_SUBSYSTEM: &str = "com.apple.dt.xctest";

/// `xcrun` arguments that turn [`XCTEST_SUBSYSTEM`] off on `udid`. The
/// setting stays with the simulator until `log config --reset` for it.
fn off_argv(udid: &str) -> [&str; 9] {
    [
        "simctl",
        "spawn",
        udid,
        "log",
        "config",
        "--mode",
        "level:off",
        "--subsystem",
        XCTEST_SUBSYSTEM,
    ]
}

/// Turn XCUITest's query log off on the booted simulator `udid`. The
/// runner reads the same with it on, so a refusal is said and nothing
/// else changes.
pub(crate) fn quiet(udid: &str) {
    let argv = off_argv(udid);
    let why = match std::process::Command::new("xcrun").args(argv).output() {
        Ok(out) if out.status.success() => return,
        Ok(out) => format!(
            "`xcrun {}` exited {}: {}",
            argv.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) => format!("xcrun did not start: {e}"),
    };
    eprintln!("[runner] warning: XCUITest's query log stays in the app's log: {why}");
}
