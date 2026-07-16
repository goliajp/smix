//! Criterion bench for smix-input enum primitives.
//!
//! enum -> &'static str is the hot accessor used by every recorder JSON
//! emit + every runner wire encode. Must stay single-digit ns.
//!
//! Run: `cargo bench --bench input -p smix-input`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_input::{KeyName, SwipeDirection};
use std::hint::black_box;

fn bench_swipe_as_str(c: &mut Criterion) {
    c.bench_function("SwipeDirection::as_str up", |b| {
        b.iter(|| black_box(SwipeDirection::Up).as_str())
    });
    c.bench_function("SwipeDirection::as_str right", |b| {
        b.iter(|| black_box(SwipeDirection::Right).as_str())
    });
}

fn bench_key_as_str(c: &mut Criterion) {
    c.bench_function("KeyName::as_str return", |b| {
        b.iter(|| black_box(KeyName::Return).as_str())
    });
    c.bench_function("KeyName::as_str arrowDown", |b| {
        b.iter(|| black_box(KeyName::ArrowDown).as_str())
    });
}

fn bench_serde(c: &mut Criterion) {
    c.bench_function("SwipeDirection serde to_string", |b| {
        b.iter(|| serde_json::to_string(black_box(&SwipeDirection::Down)).unwrap())
    });
    c.bench_function("KeyName serde from_str", |b| {
        b.iter(|| {
            let s: KeyName = serde_json::from_str(black_box("\"arrowUp\"")).unwrap();
            black_box(s);
        })
    });
}

criterion_group!(benches, bench_swipe_as_str, bench_key_as_str, bench_serde);
criterion_main!(benches);
