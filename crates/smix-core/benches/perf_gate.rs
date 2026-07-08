//! v3.28 c1 — explicit N/A marker.
//!
//! `smix-core` exposes only the `__CRATE_VERSION` const and carries no
//! pure synchronous hot path; design.md §D1's "stone = pure types + no
//! outer io" criterion has no surface area to measure here.
//!
//! Per design.md §"stone vs cement 维度 — D1 决策", perf_gate is the
//! one cross-cutting bench target uniformly present in all crates; for
//! crates without a pure synchronous hot path, the bench body is an
//! explicit no-op rather than fake measurement. Real performance for
//! this crate is bounded by outer io (subprocess / network / runtime)
//! and tracked at the end-to-end SLO layer, not per-fn.
//!
//! Run: `cargo bench --bench perf_gate -p smix-core`

use criterion::{Criterion, criterion_group, criterion_main};

fn noop(_c: &mut Criterion) {
    // intentionally empty — see top doc.
}

criterion_group!(benches, noop);
criterion_main!(benches);
