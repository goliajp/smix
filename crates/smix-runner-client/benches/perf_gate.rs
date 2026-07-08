//! v3.28 c1 — explicit N/A marker.
//!
//! `smix-runner-client` is cement (design.md §D1) — a `reqwest` HTTP
//! client, fully async with outer network io; design.md §D1 explicitly
//! states "cement 不强求 stone 标准, perf_gate 普适但内涵 N/A 接受".
//!
//! Per design.md §"stone vs cement 维度 — D1 决策", perf_gate is the
//! one cross-cutting bench target uniformly present in all crates; for
//! crates without a pure synchronous hot path, the bench body is an
//! explicit no-op rather than fake measurement. Real performance for
//! this crate is bounded by outer io (subprocess / network / runtime)
//! and tracked at the end-to-end SLO layer, not per-fn.
//!
//! Run: `cargo bench --bench perf_gate -p smix-runner-client`

use criterion::{Criterion, criterion_group, criterion_main};

fn noop(_c: &mut Criterion) {
    // intentionally empty — see top doc.
}

criterion_group!(benches, noop);
criterion_main!(benches);
