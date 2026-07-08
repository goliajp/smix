//! v3.3 c2 — criterion bench for smix-recorder-ir.
//!
//! IR is cold-ish (one append per user-recorded action) but generators
//! parse/serialize the full session and recorders may merge 100+ events
//! before flushing — keep accessor + sort + serde reasonably fast.
//!
//! Run: `cargo bench --bench ir -p smix-recorder-ir`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_input::{KeyName, SwipeDirection};
use smix_recorder_ir::{IRAction, sort_by_timestamp};
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

fn one_hundred_actions() -> Vec<IRAction> {
    let mut actions = Vec::with_capacity(100);
    for i in 0..100 {
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

fn bench_accessors(c: &mut Criterion) {
    let a = tap(1234.5);
    c.bench_function("IRAction::kind", |b| b.iter(|| black_box(&a).kind()));
    c.bench_function("IRAction::timestamp_ms", |b| {
        b.iter(|| black_box(&a).timestamp_ms())
    });
}

fn bench_sort(c: &mut Criterion) {
    let actions = one_hundred_actions();
    c.bench_function("sort_by_timestamp 100", |b| {
        b.iter(|| sort_by_timestamp(black_box(&actions)))
    });
}

fn bench_serde(c: &mut Criterion) {
    let a = tap(1234.5);
    let json = serde_json::to_string(&a).unwrap();
    c.bench_function("IRAction tap encode", |b| {
        b.iter(|| serde_json::to_string(black_box(&a)).unwrap())
    });
    c.bench_function("IRAction tap decode", |b| {
        b.iter(|| {
            let r: IRAction = serde_json::from_str(black_box(&json)).unwrap();
            black_box(r);
        })
    });
}

criterion_group!(benches, bench_accessors, bench_sort, bench_serde);
criterion_main!(benches);
