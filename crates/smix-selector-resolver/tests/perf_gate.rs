#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! v3.1 c5 — perf gate for smix-selector-resolver (hot path).
//!
//! Numbers come from `cargo bench --bench resolver` + headroom. Release-
//! mode only. CI: `cargo test --release --test perf_gate -p
//! smix-selector-resolver`.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::resolve_selector;
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 20_000;
const WARMUP_FRAC: u32 = 10;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn mk(label: Option<String>, b: Rect, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        role: None,
        identifier: None,
        label,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: b,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

fn tree_100() -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut children = Vec::with_capacity(9);
        for j in 0..9 {
            let lab = if i == 6 && j == 7 {
                "Target".to_string()
            } else {
                format!("c{}-{}", i, j)
            };
            children.push(mk(
                Some(lab),
                rect(
                    20.0 + (j as f64) * 5.0,
                    100.0 + (i as f64) * 30.0,
                    50.0,
                    25.0,
                ),
                vec![],
            ));
        }
        top.push(mk(
            Some(format!("top-{}", i)),
            rect(0.0, (i as f64) * 60.0, 390.0, 60.0),
            children,
        ));
    }
    let mut root = mk(None, rect(0.0, 0.0, 390.0, 844.0), top);
    root.raw_type = "application".into();
    root
}

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

#[test]
fn perf_gate_text_hit_100_node() {
    let tree = tree_100();
    let s = Selector::Text {
        text: Pattern::text("Target"),
        modifiers: Modifiers::default(),
    };
    let ns = measure_ns(
        || {
            black_box(resolve_selector(black_box(&tree), black_box(&s)));
        },
        ITERATIONS,
    );
    // TS V8 ~ 3650 ns. Rust target < 2 μs (cached DFA + zero-alloc walk).
    assert!(
        ns < 2000.0,
        "resolve_selector text-hit 100-node exceeded 2μs budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_id_miss_100_node() {
    let tree = tree_100();
    let s = Selector::Id {
        id: "btn-nothere".into(),
        modifiers: Modifiers::default(),
    };
    let ns = measure_ns(
        || {
            black_box(resolve_selector(black_box(&tree), black_box(&s)));
        },
        ITERATIONS,
    );
    // Pure strict-equal walk, no regex compile. TS ~ 3230 ns; Rust target < 500 ns.
    assert!(
        ns < 500.0,
        "resolve_selector id-miss 100-node exceeded 500ns budget: {:.2} ns/iter",
        ns
    );
}
