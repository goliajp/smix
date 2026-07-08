//! smix-recorder — host-side recorder for smix test sessions.
//!
//! Ported from now-retired TS sources (was `legacy/src/recorder/{session,generator-*,cleanup}.ts`, retired in v3.22).
//! Cement crate, smix-specific (depends on smix-sdk's `App`).
//!
//! # Architecture (v2.0 c3 lock-in)
//!
//! The user runs a test session against smix via [`smix_sdk::App`], but
//! routes the calls through [`RecordingApp`] (a wrapper that delegates
//! every side-effecting method to the underlying App AND pushes a
//! matching [`IRAction`] into a [`RecordSession`] buffer). At the end
//! of the session, a generator emits an equivalent test file:
//! - [`generator_rust::generate_rust`] — emit a Rust `cargo test`-style
//!   source file targeting the smix-sdk public surface
//! - [`generator_maestro_yaml::generate_maestro_yaml`] — emit a
//!   maestro-compatible yaml flow (cross-tool compatibility)
//!
//! [`cleanup::cleanup`] optionally pipes either output through the
//! `claude` CLI for a constrained AI cleanup pass (naming, `wait_for`
//! injection between phases). Cleanup is a sweetener; callers should
//! fall back to the raw output on cleanup failure.

#![doc(html_root_url = "https://docs.smix.dev/smix-recorder")]

pub mod cleanup;
pub mod generator_maestro_yaml;
pub mod generator_rust;
pub mod session;

pub use cleanup::{CleanupConfig, CleanupError, cleanup};
pub use generator_maestro_yaml::generate_maestro_yaml;
pub use generator_rust::generate_rust;
pub use session::{RecordSession, RecordingApp};

// Re-export the upstream IRAction/RecorderError for convenience.
pub use smix_recorder_ir::{IRAction, RecorderError, RecorderErrorReason};
