//! v3.28 c1 — explicit N/A marker.
//!
//! `smix-simctl` is fully async public surface (`list_runtimes` / `boot`
//! / `launch` / ...), every fn spawns a `simctl` subprocess as outer io;
//! no pure synchronous hot path exists. Per design.md §D1, this is the
//! boundary case "subprocess but sim host tool, not third-party net io".
//!
//! Per design.md §"stone vs cement 维度 — D1 决策", perf_gate is the
//! one cross-cutting bench target uniformly present in all crates; for
//! crates without a pure synchronous hot path, the bench body is an
//! explicit no-op rather than fake measurement. Real performance for
//! this crate is bounded by outer io (subprocess / network / runtime)
//! and tracked at the end-to-end SLO layer, not per-fn.
//!
//! Run: `cargo bench --bench perf_gate -p smix-simctl`

use criterion::{Criterion, criterion_group, criterion_main};

fn noop(_c: &mut Criterion) {
    // intentionally empty — see top doc.
}

criterion_group!(benches, noop);
criterion_main!(benches);
