//! v3.28 c1 — perf_gate real bench swap from v3.21 c1 placeholder.
//!
//! Mirrors v3.26 c1 `smix-selector/benches/perf_gate.rs` template:
//! the per-crate hot path lands in `perf_gate`; the broader matrix
//! stays in the sibling target (`benches/input.rs`).
//!
//! Hot path measured: `KeyName::as_str(self)` (every recorder JSON
//! emit + every runner wire encode goes through this enum-to-&'static
//! str accessor) and the `Display` impl that re-uses it (used by
//! `RecorderError` and any user-facing log line). Must stay single-
//! digit ns to avoid hot-loop drag.
//!
//! Run: `cargo bench --bench perf_gate -p smix-input`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_input::KeyName;
use std::hint::black_box;

fn perf_gate(c: &mut Criterion) {
    c.bench_function("KeyName::as_str return", |b| {
        b.iter(|| black_box(KeyName::Return).as_str())
    });
    c.bench_function("KeyName Display format!", |b| {
        b.iter(|| format!("{}", black_box(KeyName::ArrowDown)))
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
