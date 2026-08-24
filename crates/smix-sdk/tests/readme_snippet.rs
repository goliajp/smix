//! The top-level README's Rust example, compiled.
//!
//! Nothing compiled it before: the per-crate READMEs ride their crates'
//! `#![doc = include_str!]` as doctests, but the repo README belongs to
//! no crate, so its flagship snippet could rot freely (and this cycle
//! it did once, after an error-type rename). The snippet below is the
//! README's, byte for byte — the string test proves the README matches
//! this file, and the compiler proves this file matches the SDK.

const README: &str = include_str!("../../../README.md");

/// The snippet as compiled code. `rustfmt::skip` keeps it at column
/// zero so it can be compared verbatim against the README block; the
/// function is never called — connecting to a runner is the runtime's
/// job, the API surface is the compiler's.
#[rustfmt::skip]
#[allow(dead_code)]
async fn readme_example() -> Result<(), smix_sdk::ExpectationFailure> {
use smix_sdk::{App, text, KeyName};
use std::time::Duration;

let udid = std::env::var("SMIX_UDID").expect("SMIX_UDID env var required");
let app = App::connect_to_runner(22087, Some(&udid)).await?
    .with_bundle_id("com.example.app");
app.launch("com.example.app").await?;
app.wait_for(&text("Dashboard"), Duration::from_secs(5)).await?;
app.tap(&text("Sign In")).await?;
app.fill(&text("Email"), "user@example.com").await?;
app.press_key(KeyName::Return).await?;
app.assert_visible(&text("Dashboard")).await?;
Ok(())
}

#[test]
fn the_readme_rust_snippet_is_the_code_above() {
    let start = README
        .find("```rust")
        .expect("README lost its rust snippet — update this test's premise");
    let block = &README[start + "```rust\n".len()..];
    let end = block
        .find("```")
        .expect("unterminated rust fence in README");
    let block = &block[..end];

    let this_file = include_str!("readme_snippet.rs");
    for line in block.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            this_file.contains(line),
            "the README's snippet has a line this compiled copy does not:\n  {line}\n\
             Update readme_example() to match the README (or vice versa) — \
             the pair exists so the README cannot claim API the SDK lacks."
        );
    }
}
