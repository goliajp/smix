//! v3.28 c1 — perf_gate real bench swap from v3.21 c1 placeholder.
//!
//! Mirrors v3.26 c1 `smix-selector/benches/perf_gate.rs` template:
//! the per-crate hot path lands in `perf_gate`; the broader matrix
//! stays in the sibling target (`benches/host_coord.rs`).
//!
//! Hot path measured: `resolve_to_norm_coord(tree, selector)` — every
//! `driver.tap` walks this once before injecting the tap event, so it
//! must stay sub-μs even on the miss branch (largest tree-walk cost).
//!
//! Run: `cargo bench --bench perf_gate -p smix-host-coord-resolver`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_host_coord_resolver::resolve_to_norm_coord;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;

fn mk(label: &str, bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
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

fn build_small_tree(target_label: &str) -> A11yNode {
    let mut root = mk("root", rect(0.0, 0.0, 390.0, 844.0));
    for i in 0..9 {
        let label = if i == 4 {
            target_label.to_string()
        } else {
            format!("c-{}", i)
        };
        root.children.push(mk(
            &label,
            rect(20.0, 80.0 + (i as f64) * 60.0, 200.0, 44.0),
        ));
    }
    root
}

fn perf_gate(c: &mut Criterion) {
    let tree = build_small_tree("Login");
    let sel_hit = Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers::default(),
    };
    let sel_miss = Selector::Text {
        text: Pattern::text("NonexistentXYZ"),
        modifiers: Modifiers::default(),
    };

    c.bench_function("resolve_to_norm_coord text 10-node hit", |b| {
        b.iter(|| resolve_to_norm_coord(black_box(&tree), black_box(&sel_hit)))
    });
    c.bench_function("resolve_to_norm_coord text 10-node miss", |b| {
        b.iter(|| resolve_to_norm_coord(black_box(&tree), black_box(&sel_miss)))
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
