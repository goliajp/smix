#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! v3.3 c2 — perf gate for smix-input.
//!
//! Hard ceilings on enum -> &'static str accessors + serde encode/decode.
//! Numbers come from `cargo bench --bench input` + 3-5× headroom.

use smix_input::{KeyName, SwipeDirection};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 1_000_000;
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

#[test]
fn perf_gate_swipe_as_str_under_5ns() {
    let ns = measure_ns(
        || {
            black_box(SwipeDirection::Up.as_str());
        },
        ITERATIONS,
    );
    assert!(
        ns < 5.0,
        "SwipeDirection::as_str exceeded 5 ns budget: {:.3} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_key_as_str_under_5ns() {
    let ns = measure_ns(
        || {
            black_box(KeyName::ArrowDown.as_str());
        },
        ITERATIONS,
    );
    assert!(
        ns < 5.0,
        "KeyName::as_str exceeded 5 ns budget: {:.3} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_serde_to_string_under_200ns() {
    let ns = measure_ns(
        || {
            black_box(serde_json::to_string(&KeyName::Return).unwrap());
        },
        ITERATIONS / 10,
    );
    assert!(
        ns < 200.0,
        "KeyName serde to_string exceeded 200 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_serde_from_str_under_300ns() {
    let ns = measure_ns(
        || {
            let _: SwipeDirection = serde_json::from_str("\"left\"").unwrap();
        },
        ITERATIONS / 10,
    );
    assert!(
        ns < 300.0,
        "SwipeDirection serde from_str exceeded 300 ns budget: {:.2} ns/iter",
        ns
    );
}
