//! v3.28 c1 — perf_gate real bench swap from v3.21 c1 placeholder.
//!
//! Mirrors v3.26 c1 `smix-selector/benches/perf_gate.rs` template:
//! the per-crate hot path lands in `perf_gate`; the broader matrix
//! stays in the sibling target (`benches/visibility.rs`).
//!
//! Hot path measured: `collect_visible_summaries(tree, limit)` (every
//! `/system-popups` and AI-driven `/find` triage call walks the tree
//! through this) plus `role_from_raw_type(raw_type)` (per-node lookup
//! invoked from `derive_roles_recursive`, fired on every tree fetch).
//!
//! Run: `cargo bench --bench perf_gate -p smix-screen`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_screen::{A11yNode, Rect, collect_visible_summaries, role_from_raw_type};
use std::hint::black_box;

fn mk(raw_type: &str, label: &str, bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: raw_type.into(),
        role: None,
        identifier: None,
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn build_tree_10() -> A11yNode {
    let mut root = mk("application", "root", rect(0.0, 0.0, 390.0, 844.0));
    for i in 0..9 {
        root.children.push(mk(
            "button",
            &format!("btn-{}", i),
            rect(20.0, 80.0 + (i as f64) * 60.0, 200.0, 44.0),
        ));
    }
    root
}

fn perf_gate(c: &mut Criterion) {
    let tree = build_tree_10();

    c.bench_function("collect_visible_summaries 10-node DFS", |b| {
        b.iter(|| collect_visible_summaries(black_box(&tree), black_box(50)))
    });
    c.bench_function("role_from_raw_type hit (button)", |b| {
        b.iter(|| role_from_raw_type(black_box("button")))
    });
    c.bench_function("role_from_raw_type miss (unknown)", |b| {
        b.iter(|| role_from_raw_type(black_box("zzz-not-a-real-type")))
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
