#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! Perf gate for smix-selector `match_text`.
//!
//! Hard ceiling on the hot-path text matching primitives. Numbers reflect
//! criterion bench median + 30% headroom. Release-mode only —
//! `cargo test --release --test perf_gate -p smix-selector`.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Pattern, match_text, match_text_compiled};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 200_000;
const WARMUP_FRAC: u32 = 10;

fn mk(label: &str) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
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
fn perf_gate_text_string_label_hit() {
    // Target eq_ignore_ascii_case zero-alloc loop ~10 ns.
    let n = mk("Settings");
    let p = Pattern::text("settings");
    let ns = measure_ns(
        || {
            black_box(match_text(black_box(&n), black_box(&p)));
        },
        ITERATIONS,
    );
    assert!(
        ns < 40.0,
        "match_text string label-hit exceeded 40 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_text_string_miss() {
    // Scans 6 candidates without allocation; ASCII case-insensitive cmp
    // ~5 ns each.
    let n = mk("Dashboard");
    let p = Pattern::text("NotInTree");
    let ns = measure_ns(
        || {
            black_box(match_text(black_box(&n), black_box(&p)));
        },
        ITERATIONS,
    );
    assert!(
        ns < 50.0,
        "match_text string miss exceeded 50 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_regex_compile_dominated() {
    // No cache at stone layer — every call compiles. regex compile is
    // ~5-20 μs for short patterns; budget reflects that. SDK convenience
    // path. Hot resolver loops should use match_text_compiled (see
    // perf_gate_regex_compiled below).
    let n = mk("Hello");
    let p = Pattern::regex_with_flags("^hello", "i");
    let ns = measure_ns(
        || {
            black_box(match_text(black_box(&n), black_box(&p)));
        },
        ITERATIONS,
    );
    assert!(
        ns < 30_000.0,
        "match_text regex (compile-on-call) exceeded 30μs budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_regex_compiled_hot_path() {
    // Hot path: pattern pre-compiled once (typical resolver pattern —
    // cache once per selector, reuse for every candidate). Budget
    // reflects cached DFA match cost (~10-50 ns).
    let n = mk("Hello");
    let p = Pattern::regex_with_flags("^hello", "i");
    let compiled = p.compile().expect("regex compiles");
    let ns = measure_ns(
        || {
            black_box(match_text_compiled(black_box(&n), black_box(&compiled)));
        },
        ITERATIONS,
    );
    assert!(
        ns < 50.0,
        "match_text_compiled regex hot path exceeded 50ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_text_compiled_hot_path() {
    // Hot path: text pattern compiled once (essentially a String clone).
    let n = mk("Settings");
    let p = Pattern::text("settings");
    let compiled = p.compile().expect("text compiles");
    let ns = measure_ns(
        || {
            black_box(match_text_compiled(black_box(&n), black_box(&compiled)));
        },
        ITERATIONS,
    );
    assert!(
        ns < 20.0,
        "match_text_compiled text hot path exceeded 20ns budget: {:.2} ns/iter",
        ns
    );
}
