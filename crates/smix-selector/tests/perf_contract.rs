//! Perf contract regression guard.
//!
//! Hot `src/` paths inside the workspace must reach for
//! `match_text_compiled` (paired with a one-time `Pattern::compile()`
//! cache, as in `smix-selector-resolver::ResolverContext` and the
//! `smix-driver` selector pipeline). Calling bare `match_text` from a
//! hot loop forces `regex::Regex::new` on every node visit — a ~10000×
//! per-call regression vs the compiled path (see `BUDGETS.md`).
//!
//! The SDK convenience surface keeps `smix_selector::match_text` `pub`
//! for one-shot ad-hoc calls and re-exports it through `smix-sdk` /
//! `smix-driver`; both stay whitelisted via the `pub use` filter so
//! this guard fires only on actual bare hot-path callers in `src/`.
//!
//! Failure recovery: switch the offending call to `Pattern::compile()`
//! once + `match_text_compiled` per node, mirroring
//! `smix-selector-resolver::ResolverContext` cache pattern. See
//! `crates/smix-selector/README.md` § SDK convenience vs resolver hot
//! path.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn perf_contract_no_bare_match_text_in_src() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR should be <workspace>/crates/<crate>");
    let crates_dir = workspace_root.join("crates");

    // Restrict to `.rs` files so README / CHANGELOG / BUDGETS / .md
    // prose that legitimately *names* the symbol (e.g. "switch from
    // bare `smix_selector::match_text` to ...") never trips the guard.
    let output = Command::new("grep")
        .args(["-rn", "--include=*.rs", r"smix_selector::match_text\b"])
        .arg(&crates_dir)
        .output()
        .expect("grep command must succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let violations: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.contains("_compiled"))
        .filter(|line| !line.contains("/tests/"))
        .filter(|line| !line.contains("/benches/"))
        .filter(|line| !line.contains("/fuzz/"))
        .filter(|line| !line.contains("/examples/"))
        .filter(|line| !line.contains("pub use"))
        .collect();

    assert!(
        violations.is_empty(),
        "perf contract violation: {n} src/ caller(s) reach bare \
         `smix_selector::match_text` (forces `regex::Regex::new` on every \
         call, ~10000× slowdown vs `match_text_compiled`).\n\
         \n\
         Violations:\n{lines}\n\
         \n\
         Hot src/ should switch to `Pattern::compile()` once + per-node \
         `match_text_compiled`, mirroring \
         `smix-selector-resolver::ResolverContext` cache pattern. SDK \
         re-exports are written as `pub use` and stay whitelisted. See \
         `crates/smix-selector/README.md` § SDK convenience vs resolver \
         hot path.",
        n = violations.len(),
        lines = violations.join("\n"),
    );
}
