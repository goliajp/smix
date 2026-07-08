//! Perf gate for the SDK's synchronous hot paths.
//!
//! Hot path: ergonomic `Selector` factories (`text()` etc.) + the pure
//! planner `plan_launch_fresh_calls` exposed for `App::launch_fresh`.
//! These are the only synchronous, allocation-bounded hot-paths on the
//! SDK surface — everything else is async / io-bound (driver + simctl).
//! The factories are called once per SDK call from the generator or a
//! user test, and the planner runs once per `launch_fresh`, so this
//! bench locks down their cost as the cheapest visible cost in the
//! SDK fast-path.
//!
//! Run: `cargo bench --bench perf_gate -p smix-sdk`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_sdk::{plan_launch_fresh_calls, text};
use std::hint::black_box;

fn perf_gate(c: &mut Criterion) {
    c.bench_function("text() selector construction", |b| {
        b.iter(|| text(black_box("Login")));
    });

    c.bench_function("plan_launch_fresh_calls clear_state+app_path", |b| {
        let app_path = "/tmp/Example.app";
        b.iter(|| {
            plan_launch_fresh_calls(black_box(true), black_box(false), black_box(Some(app_path)))
        });
    });

    c.bench_function("plan_launch_fresh_calls fallback no app_path", |b| {
        b.iter(|| plan_launch_fresh_calls(black_box(true), black_box(true), black_box(None)));
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
