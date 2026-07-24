//! The deletability fence the README promises, held by a test rather than
//! prose: delete `smix-ai-tier` and the sense path still compiles, because
//! nothing that senses depends on it. The only crates allowed to are the
//! flow runtime that gates the AI-assertion verbs and the authoring aid that
//! borrows the local-claude primitive — neither of which senses.
//!
//! Direct-edge scanning is sufficient: any crate that transitively pulls this
//! one in does so through some workspace crate's direct edge, and that crate
//! is caught here unless allowlisted. A sense crate (resolver / driver /
//! screen / selector) reaching ai-tier would surface as an unlisted direct
//! dependent on its own path.

use std::process::Command;

/// The two legitimate, non-sensing consumers: `smix-adapter-maestro` wires the
/// AI-assertion verbs into the flow runtime, `smix-authoring-propose` borrows
/// the local-claude invocation. Neither is on the sense path, so the README's
/// "delete this and the sense path still compiles" holds.
const ALLOWED: &[&str] = &["smix-adapter-maestro", "smix-authoring-propose"];

#[test]
fn nothing_that_senses_depends_on_this_crate() {
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
                .any(|d| d["name"] == "smix-ai-tier")
        })
        .map(|p| p["name"].as_str().expect("package name"))
        .collect();

    let offenders: Vec<&&str> = dependents.iter().filter(|d| !ALLOWED.contains(*d)).collect();
    assert!(
        offenders.is_empty(),
        "a crate took a dependency on the AI-assertion tier that is not one of \
         its two allowed non-sensing consumers — if this is a sense crate, the \
         fence the README promises is breached: {offenders:?}"
    );
}
