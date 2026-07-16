//! Explicit no-op perf gate.
//!
//! `smix-cli` is a binary crate of async business orchestration over
//! `smix-simctl` / `smix-runner-client`; it has no pure synchronous hot
//! path to gate at the per-fn level.
//!
//! `perf_gate` is the one cross-cutting bench target uniformly present in
//! all crates; for crates without a pure synchronous hot path, the bench
//! body is an explicit no-op rather than fake measurement. Real
//! performance for this crate is bounded by outer io (subprocess /
//! network / runtime) and tracked at the end-to-end SLO layer, not
//! per-fn.
//!
//! Run: `cargo bench --bench perf_gate -p smix-cli`

use criterion::{Criterion, criterion_group, criterion_main};

fn noop(_c: &mut Criterion) {
    // intentionally empty — see top doc.
}

criterion_group!(benches, noop);
criterion_main!(benches);
