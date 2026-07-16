//! Hot path: serde round-trip on the request body sent on every `/tap`
//! call (the most-issued wire endpoint on a typical run) plus the
//! cheap `IncludeScope::query_value` helper that runs once per
//! tree / find / system-popups call. Together they bound the
//! per-call serialization cost on the host side — the rest of the
//! crate is plain wire types with derived serde impls and shares the
//! same code path.
//!
//! Note: `benches/wire.rs` already covers broader serde shape
//! coverage; this gate keeps a tight 2-case budget anchor.
//!
//! Run: `cargo bench --bench perf_gate -p smix-runner-wire`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_runner_wire::{IncludeScope, TapMode, TapRequest};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;

fn sample_tap_request() -> TapRequest {
    TapRequest {
        selector: Selector::Text {
            text: Pattern::Text("Login".to_string()),
            modifiers: Modifiers::default(),
        },
        mode: TapMode::Resolve,
    }
}

fn perf_gate(c: &mut Criterion) {
    let req = sample_tap_request();
    let json = serde_json::to_string(&req).expect("TapRequest must serialize");

    c.bench_function("TapRequest serde roundtrip", |b| {
        b.iter(|| {
            let s = serde_json::to_string(black_box(&req)).expect("serialize");
            let back: TapRequest = serde_json::from_str(black_box(&s)).expect("deserialize");
            back
        });
    });

    c.bench_function("TapRequest deserialize from cached json", |b| {
        b.iter(|| {
            let back: TapRequest =
                serde_json::from_str(black_box(json.as_str())).expect("deserialize");
            back
        });
    });

    c.bench_function("IncludeScope::query_value", |b| {
        b.iter(|| black_box(IncludeScope::AllWindows).query_value());
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
