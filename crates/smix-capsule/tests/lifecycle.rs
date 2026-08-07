//! The runner lifecycle has two consumers, and that is why it is a crate.
//!
//! It used to live inside the `smix-cli` binary, where nothing else could
//! reach it — so an MCP server that wanted to bring a runner up had to
//! shell out to the CLI, which puts the capability on the consumer's side
//! of the boundary and lets the two drift apart at the argv layer.

use std::process::Command;

/// Both consumers depend on this crate directly.
///
/// Read off `cargo metadata` rather than asserted in prose: the whole
/// point of the move is that there is one implementation, and the way
/// that stops being true is a copy reappearing in one of them.
#[test]
fn both_consumers_depend_on_the_lifecycle_crate() {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .expect("cargo metadata");
    assert!(out.status.success(), "cargo metadata failed");
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("metadata is not JSON");

    let dependents: Vec<String> = meta["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter(|p| {
            p["dependencies"]
                .as_array()
                .is_some_and(|deps| deps.iter().any(|d| d["name"] == "smix-capsule"))
        })
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect();

    for consumer in ["smix-cli", "smix-mcp"] {
        assert!(
            dependents.iter().any(|d| d == consumer),
            "{consumer} should depend on smix-capsule; dependents are {dependents:?}"
        );
    }
}

#[test]
fn the_surface_both_consumers_need_is_public() {
    // Compilation is the assertion: a move that left one of these behind
    // in the binary would not build here.
    // Both faces, pinned: the 2.x two-argument form (what every caller
    // before physical devices compiled against, and must keep compiling
    // against), and the target-carrying form beside it.
    let _: fn(&std::path::Path, &str) -> Vec<String> = smix_capsule::xcodebuild_argv;
    let _: fn(&std::path::Path, &str, smix_capsule::runner::RunnerTarget<'_>) -> Vec<String> =
        smix_capsule::runner::xcodebuild_argv_for;
    let _: fn(u16) -> bool = smix_capsule::health_ok;
}
