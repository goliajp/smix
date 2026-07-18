//! Clap parser unit cases for the `smix-maestro` CLI.
//!
//! Uses `#[path]` attribute to pull the bin source into the integration
//! test crate (the same-crate `pub(crate)` items become accessible
//! because the bin source compiles as a module of *this* test target).
//! No SDK / runner / sim calls — pure argv → struct parse assertions.

#![allow(clippy::unwrap_used)]

// The bin source defines main / runtime / helper fns we don't exercise
// from this test target; only Cli / Cmd are touched. Suppress dead_code
// at the module boundary so workspace `warnings = "deny"` stays happy.
#[allow(dead_code)]
#[path = "../src/bin/smix-maestro.rs"]
mod smix_maestro_bin;

use clap::Parser;
use smix_maestro_bin::{Cli, Cmd};
use std::path::PathBuf;

// Mutex guards against parallel tests touching the same env vars (the
// global process env is shared across tokio threads / vitest-style
// runners). serial within this module is enough — clap derive reads env
// lazily inside try_parse_from.
use std::sync::Mutex;
static ENV_LOCK: Mutex<()> = Mutex::new(());

const ENV_KEYS: &[&str] = &["SMIX_UDID", "SMIX_BUNDLE_ID", "SMIX_RUNNER_PORT"];

fn clear_env() {
    for k in ENV_KEYS {
        // SAFETY: tests serialize via ENV_LOCK; no other thread mutates
        // these vars concurrently.
        unsafe {
            std::env::remove_var(k);
        }
    }
}

#[test]
fn parses_test_subcommand_with_positional_flow() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_env();
    let cli = Cli::try_parse_from(["smix-maestro", "test", "flow.yaml"]).expect("parse");
    match cli.cmd {
        Cmd::Test {
            flow,
            udid,
            bundle_id,
            runner_port,
            no_launch,
            ..
        } => {
            assert_eq!(flow, PathBuf::from("flow.yaml"));
            assert_eq!(udid, None);
            // No placeholder default any more — the flow's appId takes over.
            assert_eq!(bundle_id, None);
            assert_eq!(runner_port, 22087);
            assert!(!no_launch);
        }
    }
}

#[test]
fn parses_all_flags() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_env();
    let cli = Cli::try_parse_from([
        "smix-maestro",
        "test",
        "/abs/path/flow.yaml",
        "--udid",
        "ABCD-1234",
        "--bundle-id",
        "com.test.app",
        "--runner-port",
        "33333",
        "--no-launch",
    ])
    .expect("parse");
    match cli.cmd {
        Cmd::Test {
            flow,
            udid,
            bundle_id,
            runner_port,
            no_launch,
            ..
        } => {
            assert_eq!(flow, PathBuf::from("/abs/path/flow.yaml"));
            assert_eq!(udid.as_deref(), Some("ABCD-1234"));
            assert_eq!(bundle_id.as_deref(), Some("com.test.app"));
            assert_eq!(runner_port, 33333);
            assert!(no_launch);
        }
    }
}

#[test]
fn env_vars_default_when_no_flag() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_env();
    // SAFETY: see clear_env doc.
    unsafe {
        std::env::set_var("SMIX_RUNNER_PORT", "12345");
        std::env::set_var("SMIX_UDID", "DEFG-5678");
        std::env::set_var("SMIX_BUNDLE_ID", "com.env.test");
    }
    let cli = Cli::try_parse_from(["smix-maestro", "test", "flow.yaml"]).expect("parse");
    match cli.cmd {
        Cmd::Test {
            udid,
            bundle_id,
            runner_port,
            ..
        } => {
            assert_eq!(udid.as_deref(), Some("DEFG-5678"));
            assert_eq!(bundle_id.as_deref(), Some("com.env.test"));
            assert_eq!(runner_port, 12345);
        }
    }
    clear_env();
}

#[test]
fn flag_overrides_env_var() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_env();
    // SAFETY: see clear_env doc.
    unsafe {
        std::env::set_var("SMIX_RUNNER_PORT", "12345");
    }
    let cli = Cli::try_parse_from([
        "smix-maestro",
        "test",
        "flow.yaml",
        "--runner-port",
        "22087",
    ])
    .expect("parse");
    match cli.cmd {
        Cmd::Test { runner_port, .. } => {
            assert_eq!(runner_port, 22087);
        }
    }
    clear_env();
}

#[test]
fn missing_subcommand_fails() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_env();
    let res = Cli::try_parse_from(["smix-maestro"]);
    assert!(res.is_err(), "expected parse error for missing subcommand");
}

#[test]
fn no_launch_flag_parses() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_env();
    let cli =
        Cli::try_parse_from(["smix-maestro", "test", "flow.yaml", "--no-launch"]).expect("parse");
    match cli.cmd {
        Cmd::Test { no_launch, .. } => assert!(no_launch),
    }
}
