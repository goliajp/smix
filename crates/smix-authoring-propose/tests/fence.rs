//! The deletability fence, held by a test rather than prose: delete
//! `smix-authoring-propose` and the sense/act path still compiles, because
//! nothing on it may depend on this crate. Only the CLI's thin
//! `smix authoring propose` wire is allowed to.
//!
//! Direct-edge scanning is sufficient: any crate that transitively pulls this
//! one in does so through some workspace crate's direct edge, and that crate
//! is caught here unless allowlisted.

use std::process::Command;

const ALLOWED: &[&str] = &["smix-cli"];

#[test]
fn nothing_but_the_cli_wire_depends_on_this_crate() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata runs");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));

    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata parses");
    let packages = meta["packages"].as_array().expect("packages array");
    assert!(!packages.is_empty(), "metadata listed no packages — the fence read nothing");

    let dependents: Vec<&str> = packages
        .iter()
        .filter(|p| {
            p["dependencies"]
                .as_array()
                .expect("dependencies array")
                .iter()
                .any(|d| d["name"] == "smix-authoring-propose")
        })
        .map(|p| p["name"].as_str().expect("package name"))
        .collect();

    let offenders: Vec<&&str> = dependents.iter().filter(|d| !ALLOWED.contains(*d)).collect();
    assert!(
        offenders.is_empty(),
        "a crate on the sense/act path took a dependency on the fenced \
         authoring-propose tier — the fence is that only the CLI wire may: {offenders:?}"
    );
}
