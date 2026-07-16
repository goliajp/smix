#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive
//! Perf gate for smix-screen.
//!
//! Hard ceiling on the hot-path visibility primitives. Numbers come from
//! `cargo bench --bench visibility` + 30% headroom. CI runs
//! `cargo test --test perf_gate -p smix-screen` and fails if median
//! exceeds budget.
//!
//! Each test runs N iterations to stabilize the median, then asserts
//! `median_ns < budget_ns`. Same machine class assumed (M-series macOS dev
//! laptops); if these tests ever fail on a slow CI runner, raise the budget
//! once with a comment + a re-baseline criterion run, never silently.

use smix_screen::{A11yNode, Rect, is_visible_enough, visible_area};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 200_000;
const WARMUP_FRAC: u32 = 10; // 1/10 warmup

fn mk(bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: None,
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

/// Median per-iteration time in nanoseconds. Runs `iterations` times,
/// warmup phase first, then measures `iterations - warmup` reps and divides
/// the elapsed wall time by reps. Simple, deterministic — for CI gate
/// purposes the criterion-style multi-sample / outlier-trim isn't
/// necessary: we only need "did it explode by 10×?"
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
fn perf_gate_is_visible_enough_in_view() {
    let node = mk(rect(20.0, 100.0, 200.0, 44.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    let ns = measure_ns(
        || {
            black_box(is_visible_enough(black_box(&node), black_box(&tree)));
        },
        ITERATIONS,
    );
    // Target < 10 ns; budget 15 ns ceiling for CI tolerance
    // (M1/M2/CI scheduler jitter).
    assert!(
        ns < 15.0,
        "is_visible_enough in-view exceeded 15 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_is_visible_enough_zero_reject() {
    let node = mk(rect(50.0, 50.0, 0.0, 0.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    let ns = measure_ns(
        || {
            black_box(is_visible_enough(black_box(&node), black_box(&tree)));
        },
        ITERATIONS,
    );
    // Early-return branch on b.w<=0 — should be the fastest variant.
    assert!(
        ns < 10.0,
        "is_visible_enough zero-reject exceeded 10 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_visible_area_in_view() {
    let node = mk(rect(20.0, 100.0, 200.0, 44.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    let ns = measure_ns(
        || {
            black_box(visible_area(black_box(&node), black_box(&tree)));
        },
        ITERATIONS,
    );
    assert!(
        ns < 15.0,
        "visible_area in-view exceeded 15 ns budget: {:.2} ns/iter",
        ns
    );
}
