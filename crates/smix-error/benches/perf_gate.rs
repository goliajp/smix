//! v3.28 c1 — perf_gate real bench swap from v3.21 c1 placeholder.
//!
//! Mirrors v3.26 c1 `smix-selector/benches/perf_gate.rs` template:
//! the per-crate hot path lands in `perf_gate`; the broader matrix
//! stays in the sibling target (`benches/error.rs`).
//!
//! Hot path measured: `similarity(a, b)` + `edit_distance(a, b)` (the
//! per-candidate scoring kernel inside `build_suggestions`) and
//! `build_suggestions(target, visible)` itself (the cold-but-AI-readable
//! summary every `ExpectationFailure::new` runs to populate
//! `suggestions[]`).
//!
//! Run: `cargo bench --bench perf_gate -p smix-error`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_error::{build_suggestions, edit_distance, similarity};
use smix_screen::{ElementSummary, Rect, Role};
use std::hint::black_box;

fn mk_summary(name: &str, role: Option<Role>) -> ElementSummary {
    ElementSummary {
        role,
        name: Some(name.to_string()),
        id: None,
        text: None,
        bounds: Rect {
            x: 50.0,
            y: 100.0,
            w: 200.0,
            h: 40.0,
        },
        enabled: true,
    }
}

fn ten_summaries() -> Vec<ElementSummary> {
    vec![
        mk_summary("Login", Some(Role::Button)),
        mk_summary("Logout", Some(Role::Button)),
        mk_summary("Settings", Some(Role::Button)),
        mk_summary("Profile", Some(Role::Button)),
        mk_summary("Username", Some(Role::TextField)),
        mk_summary("Password", Some(Role::SecureTextField)),
        mk_summary("Search", Some(Role::SearchField)),
        mk_summary("Cancel", Some(Role::Button)),
        mk_summary("Submit", Some(Role::Button)),
        mk_summary("Help", Some(Role::Link)),
    ]
}

fn perf_gate(c: &mut Criterion) {
    let visible = ten_summaries();

    c.bench_function("similarity short typo", |b| {
        b.iter(|| similarity(black_box("Login"), black_box("Logon")))
    });
    c.bench_function("edit_distance medium", |b| {
        b.iter(|| edit_distance(black_box("Settings"), black_box("Setings")))
    });
    c.bench_function("build_suggestions 10-element list", |b| {
        b.iter(|| build_suggestions(Some(black_box("Logn")), black_box(&visible)))
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
