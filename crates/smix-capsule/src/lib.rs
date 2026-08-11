//! Runner lifecycle: bring an XCUITest runner up on a simulator, ask it
//! whether it is alive, take it down.
//!
//! This lived inside the `smix-cli` binary, which meant no other crate
//! could reach it. The MCP server needs exactly this — an agent that has
//! to ask a human to run `capsule up` in another terminal first is not
//! driving anything — and its only way in would have been to shell out to
//! the CLI. That puts the capability on the consumer's side of the
//! boundary, and leaves two programs agreeing about argv rather than
//! about types.
#![deny(missing_docs)]
// `deny` rather than `forbid`: the tests mutate process environment, which
// edition 2024 makes unsafe, and forbid cannot be lifted where that is the
// point of the test. No non-test code here uses unsafe.
#![deny(unsafe_code)]

pub mod reconcile;
pub mod runner;
pub mod runner_android;
pub mod runner_state;
pub mod runner_view;
pub mod signing;

pub use runner::{health_ok, xcodebuild_argv};
