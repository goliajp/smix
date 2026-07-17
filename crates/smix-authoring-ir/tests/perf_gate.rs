#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! Perf gate for smix-authoring-ir.
//!
//! Accessors + sort + serde encode/decode budgets.

use smix_input::{KeyName, SwipeDirection};
use smix_authoring_ir::{IRAction, sort_by_timestamp};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 200_000;
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

fn tap(ts: f64) -> IRAction {
    IRAction::Tap {
        selector: Selector::Text {
            text: Pattern::text("Login"),
            modifiers: Modifiers::default(),
        },
        timestamp_ms: ts,
    }
}

fn one_hundred_actions() -> Vec<IRAction> {
    let mut actions = Vec::with_capacity(100);
    for i in 0..100 {
        let ts = (i * 73 % 1000) as f64;
        actions.push(match i % 4 {
            0 => tap(ts),
            1 => IRAction::PressKey {
                key: KeyName::Return,
                timestamp_ms: ts,
            },
            2 => IRAction::Swipe {
                direction: SwipeDirection::Up,
                from: None,
                timestamp_ms: ts,
            },
            _ => IRAction::HideKeyboard { timestamp_ms: ts },
        });
    }
    actions
}

#[test]
fn perf_gate_kind_accessor_under_5ns() {
    let a = tap(1.0);
    let ns = measure_ns(
        || {
            black_box(black_box(&a).kind());
        },
        ITERATIONS,
    );
    assert!(ns < 5.0, "IRAction::kind exceeded 5 ns: {:.3} ns/iter", ns);
}

#[test]
fn perf_gate_timestamp_accessor_under_5ns() {
    let a = tap(1.0);
    let ns = measure_ns(
        || {
            black_box(black_box(&a).timestamp_ms());
        },
        ITERATIONS,
    );
    assert!(
        ns < 5.0,
        "IRAction::timestamp_ms exceeded 5 ns: {:.3} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_sort_by_timestamp_100_under_15us() {
    let actions = one_hundred_actions();
    let ns = measure_ns(
        || {
            black_box(sort_by_timestamp(black_box(&actions)));
        },
        ITERATIONS / 20,
    );
    assert!(
        ns < 15_000.0,
        "sort_by_timestamp 100-action exceeded 15 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_serde_encode_tap_under_2us() {
    let a = tap(1234.5);
    let ns = measure_ns(
        || {
            black_box(serde_json::to_string(black_box(&a)).unwrap());
        },
        ITERATIONS / 10,
    );
    assert!(
        ns < 2_000.0,
        "IRAction tap encode exceeded 2 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_serde_decode_tap_under_10us() {
    let a = tap(1234.5);
    let json = serde_json::to_string(&a).unwrap();
    let ns = measure_ns(
        || {
            let _: IRAction = serde_json::from_str(black_box(&json)).unwrap();
        },
        ITERATIONS / 10,
    );
    assert!(
        ns < 10_000.0,
        "IRAction tap decode exceeded 10 μs: {:.0} ns/iter",
        ns
    );
}
