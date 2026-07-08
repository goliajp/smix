#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! v3.3 c2 — perf gate for smix-host-coord-resolver.
//!
//! End-to-end resolve_to_norm_coord budget. Sits on top of
//! smix-selector-resolver (~1 μs text-100-node-hit) + 1 centroid + 1
//! normalize — total budget ~3 μs hit, ~2 μs miss, ~1 μs id-hit.

use smix_host_coord_resolver::resolve_to_norm_coord;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 20_000;
const WARMUP_FRAC: u32 = 10;

fn measure_ns<F: FnMut()>(mut body: F, iterations: u32) -> f64 {
    let warmup = iterations / WARMUP_FRAC;
    for _ in 0..warmup {
        body();
    }
    let measured = iterations - warmup;
    let start = Instant::now();
    for _ in 0..measured {
        body();
    }
    let elapsed = start.elapsed();
    elapsed.as_nanos() as f64 / measured as f64
}

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

fn build_tree_100(target: &str) -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut children = Vec::with_capacity(9);
        for j in 0..9 {
            let label = if i == 5 && j == 4 {
                target.to_string()
            } else {
                format!("c{}-{}", i, j)
            };
            children.push(mk(
                &label,
                rect(
                    20.0 + (j as f64) * 5.0,
                    100.0 + (i as f64) * 30.0,
                    50.0,
                    25.0,
                ),
            ));
        }
        let mut t = mk(
            &format!("top-{}", i),
            rect(0.0, (i as f64) * 60.0, 390.0, 60.0),
        );
        t.children = children;
        top.push(t);
    }
    let mut root = mk("root", rect(0.0, 0.0, 390.0, 844.0));
    root.children = top;
    root
}

#[test]
fn perf_gate_text_100_hit_under_3us() {
    let tree = build_tree_100("Login");
    let sel = Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers::default(),
    };
    let ns = measure_ns(
        || {
            let _ = resolve_to_norm_coord(black_box(&tree), black_box(&sel));
        },
        ITERATIONS,
    );
    assert!(
        ns < 3_000.0,
        "resolve_to_norm_coord text 100-node hit exceeded 3 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_text_100_miss_under_3us() {
    let tree = build_tree_100("Login");
    let sel = Selector::Text {
        text: Pattern::text("NonexistentXYZ"),
        modifiers: Modifiers::default(),
    };
    let ns = measure_ns(
        || {
            let _ = resolve_to_norm_coord(black_box(&tree), black_box(&sel));
        },
        ITERATIONS,
    );
    assert!(
        ns < 3_000.0,
        "resolve_to_norm_coord text miss exceeded 3 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_id_100_hit_under_1us() {
    let mut tree = build_tree_100("anything");
    tree.children[5].children[4].identifier = Some("login-btn".into());
    let sel = Selector::Id {
        id: "login-btn".into(),
        modifiers: Modifiers::default(),
    };
    let ns = measure_ns(
        || {
            let _ = resolve_to_norm_coord(black_box(&tree), black_box(&sel));
        },
        ITERATIONS,
    );
    assert!(
        ns < 1_000.0,
        "resolve_to_norm_coord id 100-node hit exceeded 1 μs: {:.0} ns/iter",
        ns
    );
}
