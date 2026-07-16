#![no_main]
//! `smix-core` is intentionally an empty workspace anchor / version
//! sentinel (see crate root docs + README stone table — the sense /
//! decide / act primitives live in dedicated stones so consumers pay
//! only for what they pull in). This fuzz target exercises the const-
//! version link path so the fuzz scaffolding stays compile-ready
//! alongside the rest of the workspace.
//!
//! This IS the terminal shape, not a placeholder. smix-core does not and
//! will not gain a parser "port surface" — that would contradict the
//! anchor-crate design. The A11yNode JSON-parse fuzz an earlier TODO
//! speculated about already lives where the type does:
//! `smix-screen/fuzz/fuzz_targets/a11y_node_parse.rs`. Moving or aliasing
//! `smix-screen::A11yNode` into smix-core is deliberately rejected: A11yNode
//! is a screen-layer type with 20+ stone dependents; moving it violates
//! the thin-umbrella design for zero functional gain, and the parse fuzz
//! is already correctly homed in its owning stone.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = smix_core::__CRATE_VERSION;
    let _ = data;
});
