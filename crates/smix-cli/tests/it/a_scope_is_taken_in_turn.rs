//! `smix lease acquire` hands a scope out one holder at a time, in the
//! order they asked, and a device command on that scope from anyone else
//! is refused.
//!
//! A team of agents took turns on two devices with a lock of their own: a
//! directory each of them polled for. Nothing queued, so whoever polled
//! first after a release got it (one waited four hours); a holder that
//! stopped halfway left the directory behind; and smix knew nothing of
//! it, so an agent without the lock drove the other's device. These run
//! the real binary as separate processes, the way the agents do, against
//! a machine directory of their own.

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn machine(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "smix-scope-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn smix(m: &std::path::Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_smix"));
    c.args(args)
        .env("SMIX_MACHINE_DIR", m)
        .env_remove("SMIX_LEASE_HOLDER");
    c
}

fn run(m: &std::path::Path, args: &[&str]) -> Output {
    smix(m, args).output().expect("smix runs")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// A scope has to be declared before it is taken.
fn declared(m: &std::path::Path) {
    let o = run(m, &["lease", "scope", "s", "--device", "emulator-5998"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
}

fn wait_in_line(m: &std::path::Path, holder: &str) -> Child {
    smix(
        m,
        &[
            "lease",
            "acquire",
            "--scope",
            "s",
            "--holder",
            holder,
            "--wait",
            "--max-wait",
            "60",
        ],
    )
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn")
}

/// Polls `status` until `want` shows, so the test does not sleep a guess.
fn until_status(m: &std::path::Path, want: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        // a look that began before the deadline still counts
        let looked = Instant::now();
        let s = text(&run(m, &["lease", "status", "--scope", "s"]));
        if s.contains(want) || looked >= deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn holders_get_the_scope_in_the_order_they_asked() {
    let m = machine("order");
    declared(&m);
    assert_eq!(
        run(&m, &["lease", "acquire", "--scope", "s", "--holder", "a"])
            .status
            .code(),
        Some(0)
    );
    let b = wait_in_line(&m, "b");
    let s = until_status(&m, "1. b");
    assert!(s.contains("1. b"), "{s}");
    let c = wait_in_line(&m, "c");
    let s = until_status(&m, "2. c");
    assert!(s.contains("2. c"), "{s}");
    // b asking again from another process keeps its place
    let again = run(&m, &["lease", "acquire", "--scope", "s", "--holder", "b"]);
    assert_eq!(again.status.code(), Some(75), "{}", text(&again));
    let s = until_status(&m, "2. c");
    assert!(
        s.contains("1. b") && s.contains("2. c") && !s.contains("3."),
        "{s}"
    );

    assert_eq!(
        run(&m, &["lease", "release", "--scope", "s", "--holder", "a"])
            .status
            .code(),
        Some(0)
    );
    let b_out = b.wait_with_output().expect("b");
    assert_eq!(b_out.status.code(), Some(0), "{}", text(&b_out));
    assert!(until_status(&m, "held by b").contains("held by b"));

    assert_eq!(
        run(&m, &["lease", "release", "--scope", "s", "--holder", "b"])
            .status
            .code(),
        Some(0)
    );
    let c_out = c.wait_with_output().expect("c");
    assert_eq!(c_out.status.code(), Some(0), "{}", text(&c_out));
}

#[test]
fn a_hold_tied_to_a_process_ends_when_it_does() {
    let m = machine("pid");
    declared(&m);
    let mut sleeper = Command::new("sleep").arg("30").spawn().expect("sleep");
    let pid = sleeper.id().to_string();
    let took = run(
        &m,
        &[
            "lease", "acquire", "--scope", "s", "--holder", "a", "--pid", &pid,
        ],
    );
    assert_eq!(took.status.code(), Some(0), "{}", text(&took));
    let busy = run(&m, &["lease", "acquire", "--scope", "s", "--holder", "b"]);
    assert_eq!(busy.status.code(), Some(75), "{}", text(&busy));
    sleeper.kill().expect("kill");
    sleeper.wait().expect("reap");
    let got = run(&m, &["lease", "acquire", "--scope", "s", "--holder", "b"]);
    assert_eq!(got.status.code(), Some(0), "{}", text(&got));
    let s = text(&run(&m, &["lease", "status", "--scope", "s"]));
    assert!(
        s.contains("released automatically: a") && s.contains(&pid),
        "{s}"
    );
}

#[test]
fn a_device_command_on_a_held_scope_is_refused_unless_it_is_the_holders() {
    let m = machine("refuse");
    // a serial with an emulator's shape and nothing behind it: the check
    // runs before anything asks a device
    const DEVICE: &str = "emulator-5998";
    assert_eq!(
        run(&m, &["lease", "scope", "s", "--device", DEVICE])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        run(&m, &["lease", "acquire", "--scope", "s", "--holder", "a"])
            .status
            .code(),
        Some(0)
    );

    let args = ["sim", "frontmost", DEVICE];
    let other = smix(&m, &args)
        .env("SMIX_LEASE_HOLDER", "b")
        .output()
        .unwrap();
    assert_eq!(other.status.code(), Some(3), "{}", text(&other));
    assert!(
        text(&other).contains("held by a") || text(&other).contains("which a holds"),
        "{}",
        text(&other)
    );
    let unnamed = run(&m, &args);
    assert_eq!(unnamed.status.code(), Some(3), "{}", text(&unnamed));
    assert!(
        text(&unnamed).contains("SMIX_LEASE_HOLDER is not set"),
        "{}",
        text(&unnamed)
    );

    // the holder's own command goes past the check (and fails at the
    // device that is not there, which is a different failure)
    let mine = smix(&m, &args)
        .env("SMIX_LEASE_HOLDER", "a")
        .output()
        .unwrap();
    assert_ne!(mine.status.code(), Some(3), "{}", text(&mine));
    assert!(!text(&mine).contains("is in scope"), "{}", text(&mine));

    // a device outside the scope is not its business
    let outside = smix(&m, &["sim", "frontmost", "emulator-5996"])
        .output()
        .unwrap();
    assert!(!text(&outside).contains("scope"), "{}", text(&outside));

    // and once it is let go, anyone may
    assert_eq!(
        run(&m, &["lease", "release", "--scope", "s", "--holder", "a"])
            .status
            .code(),
        Some(0)
    );
    let free = run(&m, &args);
    assert!(!text(&free).contains("scope s"), "{}", text(&free));
}

#[test]
fn a_status_as_json_says_who_holds_it_and_null_when_nobody_does() {
    let m = machine("json");
    declared(&m);
    let read = |args: &[&str]| -> serde_json::Value {
        let o = run(&m, args);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&o)))
    };
    let one = ["lease", "status", "--scope", "s", "--json"];
    let free = read(&one);
    assert_eq!(free["scope"], "s");
    assert_eq!(free["devices"], serde_json::json!(["emulator-5998"]));
    assert!(free["holder"].is_null() && free.get("holder").is_some());
    assert_eq!(free["requireHolder"], false);

    let took = run(
        &m,
        &[
            "lease", "acquire", "--scope", "s", "--holder", "a", "--idle", "7",
        ],
    );
    assert_eq!(took.status.code(), Some(0), "{}", text(&took));
    let mut b = wait_in_line(&m, "b");
    assert!(until_status(&m, "1. b").contains("1. b"));
    let held = read(&one);
    assert_eq!(held["holder"]["holder"], "a");
    assert_eq!(held["holder"]["idleSecs"], 420);
    assert!(held["holder"]["lastSeen"].is_string() && held["holder"]["since"].is_string());
    assert!(
        held["holder"]
            .get("until")
            .is_some_and(serde_json::Value::is_null)
    );
    assert_eq!(held["queue"][0]["holder"], "b");
    assert_eq!(held["released"], serde_json::json!([]));

    let all = read(&["lease", "status", "--json", "--scope"]);
    assert_eq!(all.as_array().map(Vec::len), Some(1));
    assert_eq!(all[0]["scope"], "s");
    b.kill().expect("kill");
    b.wait().expect("reap");
}

#[test]
fn a_scope_nobody_declared_is_an_error_and_is_not_made() {
    let m = machine("undeclared");
    declared(&m);
    for args in [
        &["lease", "status", "--scope", "typo"][..],
        &["lease", "status", "--scope", "typo", "--json"],
        &["lease", "acquire", "--scope", "typo", "--holder", "a"],
        &["lease", "renew", "--scope", "typo", "--holder", "a"],
        &["lease", "release", "--scope", "typo", "--holder", "a"],
        &["lease", "scope", "typo", "--remove"],
    ] {
        let o = run(&m, args);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {}", text(&o));
        assert!(text(&o).contains("no scope typo"), "{}", text(&o));
    }
    let all = text(&run(&m, &["lease", "status", "--scope"]));
    assert!(all.contains("s: devices") && !all.contains("typo"), "{all}");
}

#[test]
fn a_scope_is_removed_only_when_nobody_holds_it() {
    let m = machine("remove");
    declared(&m);
    run(&m, &["lease", "acquire", "--scope", "s", "--holder", "a"]);
    let held = run(&m, &["lease", "scope", "s", "--remove"]);
    assert_eq!(held.status.code(), Some(3), "{}", text(&held));
    assert!(text(&held).contains("held by a"), "{}", text(&held));
    let still = text(&run(&m, &["lease", "status", "--scope", "s"]));
    assert!(still.contains("held by a"), "{still}");

    run(&m, &["lease", "release", "--scope", "s", "--holder", "a"]);
    let gone = run(&m, &["lease", "scope", "s", "--remove"]);
    assert_eq!(gone.status.code(), Some(0), "{}", text(&gone));
    let after = run(&m, &["lease", "status", "--scope", "s"]);
    assert_eq!(after.status.code(), Some(1), "{}", text(&after));
}

#[test]
fn a_scope_that_requires_a_holder_refuses_while_nobody_holds_it() {
    let m = machine("require");
    let declare = |extra: &[&str]| {
        let mut args = vec!["lease", "scope", "s", "--device", "emulator-5998"];
        args.extend(extra);
        let o = run(&m, &args);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    };
    declare(&["--require-holder"]);
    let as_holder = |holder: Option<&str>| {
        let mut c = smix(&m, &["sim", "frontmost", "emulator-5998"]);
        if let Some(h) = holder {
            c.env("SMIX_LEASE_HOLDER", h);
        }
        c.output().unwrap()
    };
    for who in [None, Some("a")] {
        let o = as_holder(who);
        assert_eq!(o.status.code(), Some(3), "{}", text(&o));
        assert!(text(&o).contains("nobody does"), "{}", text(&o));
    }
    run(&m, &["lease", "acquire", "--scope", "s", "--holder", "a"]);
    let mine = as_holder(Some("a"));
    assert!(!text(&mine).contains("scope s"), "{}", text(&mine));
    assert_eq!(as_holder(Some("b")).status.code(), Some(3));
    run(&m, &["lease", "release", "--scope", "s", "--holder", "a"]);
    // declared again without it, a free scope is anyone's
    declare(&[]);
    let free = as_holder(None);
    assert!(!text(&free).contains("scope s"), "{}", text(&free));
}

#[test]
fn a_relative_until_waits_for_the_grant_and_the_holder_can_change_it() {
    let m = machine("until");
    declared(&m);
    let until = |v: &serde_json::Value| v["until"].as_str().map(str::to_string);
    let read = || -> serde_json::Value {
        let o = run(&m, &["lease", "status", "--scope", "s", "--json"]);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&o)))
    };
    let took = run(&m, &["lease", "acquire", "--scope", "s", "--holder", "a"]);
    assert_eq!(took.status.code(), Some(0), "{}", text(&took));
    let mut b = smix(
        &m,
        &[
            "lease",
            "acquire",
            "--scope",
            "s",
            "--holder",
            "b",
            "--wait",
            "--max-wait",
            "60",
            "--until",
            "+60m build",
        ],
    )
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn");
    assert!(until_status(&m, "1. b").contains("1. b"));
    assert_eq!(
        until(&read()["queue"][0]).as_deref(),
        Some("+60m build"),
        "while it waits, the span has not started"
    );

    let o = run(&m, &["lease", "release", "--scope", "s", "--holder", "a"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(b.wait().expect("reap").code(), Some(0));
    let held = until(&read()["holder"]).expect("b said until");
    assert!(
        held != "+60m build" && held.ends_with(" build"),
        "granted, it is a clock time: {held}"
    );

    let o = run(
        &m,
        &[
            "lease", "renew", "--scope", "s", "--holder", "a", "--until", "+5m x",
        ],
    );
    assert_eq!(
        o.status.code(),
        Some(3),
        "a no longer holds it: {}",
        text(&o)
    );
    let o = run(
        &m,
        &[
            "lease",
            "renew",
            "--scope",
            "s",
            "--holder",
            "b",
            "--until",
            "+30m tests",
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let renewed = until(&read()["holder"]).expect("b said until");
    assert!(renewed.ends_with(" tests"), "{renewed}");
}
