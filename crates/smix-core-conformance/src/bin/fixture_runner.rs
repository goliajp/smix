//! fixture-runner — cross-SDK conformance binary.
//!
//! Usage: `fixture-runner <backend> <fixture-id>`
//!
//! - `<backend>`:`rust` (this crate runs smix-ffi directly).
//!   Future backends: `swift` / `kotlin` / `rn` (driven by sister
//!   harness binaries in `swift-bridge/` / `android-runner/sdk/` /
//!   `smix-sdk-rn/`).
//! - `<fixture-id>`:fixture filename root (e.g. `spike-001`).
//!
//! Output: JSON array of matched node ids on stdout (sorted).
//!
//! Exit:
//! - 0 = backend output == fixture.expected
//! - 1 = mismatch (prints diff to stderr)
//! - 2 = usage / IO / parse error
//! - 64 = unknown backend

use smix_core_conformance::load_fixture;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: fixture-runner <backend> <fixture-id>");
        eprintln!("  backend: rust | swift | kotlin | rn");
        return ExitCode::from(2);
    }
    let backend = &args[1];
    let id = &args[2];

    let fixture = match load_fixture(id) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("fixture-runner: load_fixture({id}) failed: {e}");
            return ExitCode::from(2);
        }
    };

    let actual = match backend.as_str() {
        "rust" => match run_rust(&fixture) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("fixture-runner rust backend failed: {e}");
                return ExitCode::from(2);
            }
        },
        "swift" | "kotlin" | "rn" => {
            eprintln!(
                "fixture-runner: backend '{backend}' not implemented in this binary; \
                 use sister harness in swift-bridge / android-runner / smix-sdk-rn"
            );
            return ExitCode::from(64);
        }
        _ => {
            eprintln!("fixture-runner: unknown backend '{backend}'");
            return ExitCode::from(64);
        }
    };

    let mut actual_sorted = actual.clone();
    actual_sorted.sort();
    let mut expected_sorted = fixture.expected.clone();
    expected_sorted.sort();

    // emit stdout as deterministic JSON array (sorted) for byte-identical diff
    match serde_json::to_string(&actual_sorted) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("fixture-runner: serialize actual failed: {e}");
            return ExitCode::from(2);
        }
    }

    if actual_sorted == expected_sorted {
        ExitCode::from(0)
    } else {
        eprintln!("MISMATCH:");
        eprintln!("  expected: {expected_sorted:?}");
        eprintln!("  actual:   {actual_sorted:?}");
        ExitCode::from(1)
    }
}

fn run_rust(fixture: &smix_core_conformance::Fixture) -> anyhow::Result<Vec<String>> {
    let tree_json = serde_json::to_string(&fixture.tree)?;
    let selector_json = serde_json::to_string(&fixture.selector)?;
    smix_ffi::resolve_selector(tree_json, selector_json)
        .map_err(|e| anyhow::anyhow!("smix_ffi::resolve_selector: {e}"))
}
