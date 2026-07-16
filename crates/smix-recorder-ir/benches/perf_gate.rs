//! Mirrors the `smix-selector/benches/perf_gate.rs` template:
//! the per-crate hot path lands in `perf_gate`; the broader matrix
//! stays in the sibling target (`benches/ir.rs`).
//!
//! Hot path measured: `sort_by_timestamp(actions)` (every recorder
//! flush + every generator parse runs this over the accumulated event
//! buffer, hits 100+ entries before flush) and
//! `RecorderErrorReason` Display (fired on every recorder failure
//! emit; AI-readable surface).
//!
//! Run: `cargo bench --bench perf_gate -p smix-recorder-ir`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_input::{KeyName, SwipeDirection};
use smix_recorder_ir::{IRAction, RecorderErrorReason, sort_by_timestamp};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;

fn tap(ts: f64) -> IRAction {
    IRAction::Tap {
        selector: Selector::Text {
            text: Pattern::text("Login"),
            modifiers: Modifiers::default(),
        },
        timestamp_ms: ts,
    }
}

fn ten_actions() -> Vec<IRAction> {
    let mut actions = Vec::with_capacity(10);
    for i in 0..10 {
        let ts = (i * 73 % 1000) as f64; // pseudo-random order
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

fn perf_gate(c: &mut Criterion) {
    let actions = ten_actions();

    c.bench_function("sort_by_timestamp 10", |b| {
        b.iter(|| sort_by_timestamp(black_box(&actions)))
    });
    c.bench_function("RecorderErrorReason Display", |b| {
        b.iter(|| format!("{}", black_box(RecorderErrorReason::MalformedAction)))
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
