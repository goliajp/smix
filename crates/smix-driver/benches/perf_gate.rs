//! v3.28 c1 — explicit N/A marker. `smix-driver` has no pure
//! synchronous hot path: every public surface (`tap` / `fill` /
//! `find_one` / `wait_for` / `tree` / …) is `async` and crosses an io
//! boundary (HTTP runner + simctl subprocess). The two non-async
//! methods (`SimctlDriver::new` and `SimctlDriver::runner`) are
//! trivial moves / borrows that the optimizer collapses to zero cost
//! in any realistic call site — benching them would be measurement
//! noise, not signal.
//!
//! See `docs/design.md` §D1 — cement-vs-stone discussion explicitly
//! flags io-saturated stones as a boundary case where the perf_gate
//! discipline degrades into a marker rather than a real budget gate.
//! Hot-path coverage of the decide-layer fast-path (selector resolve
//! against an a11y tree) lives in the upstream
//! `smix-selector-resolver` crate's `perf_gate` bench; driver itself
//! is just the async fan-out shell around it.
//!
//! Run: `cargo bench --bench perf_gate -p smix-driver` (no-op).

use criterion::{Criterion, criterion_group, criterion_main};

fn noop(_c: &mut Criterion) {
    // intentionally empty — see top doc.
}

criterion_group!(benches, noop);
criterion_main!(benches);
