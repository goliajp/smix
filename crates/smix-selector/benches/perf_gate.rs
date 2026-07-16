//! Perf gate bench for the real hot path (`match_text_compiled`).
//!
//! The SDK convenience surface (`match_text(node, &Pattern)`) calls
//! `regex::Regex::new` on every call, and is not what
//! `smix-selector-resolver` or `smix-driver` actually invoke at runtime
//! — both walk via `Pattern::compile()` (once) +
//! `match_text_compiled(node, &CompiledPattern)` (per node), amortizing
//! compile cost across the tree walk. So this bench mirrors the
//! resolver / driver path: compile patterns once in setup, then bench
//! `match_text_compiled` per iteration. Per-call compile + match cost
//! (the SDK path) is captured separately in `benches/matchtext.rs`
//! § "Pattern::compile" group.
//!
//! Cases exercise the 6-field-OR matcher: plain-text hit (most common),
//! plain-text miss (most-called rejection branch when scanning a large
//! a11y tree), regex hit, and regex miss.
//!
//! Run: `cargo bench --bench perf_gate -p smix-selector`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Pattern, match_text_compiled};
use std::hint::black_box;

fn make_node(text: &str) -> A11yNode {
    A11yNode {
        raw_type: "any".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: Some(text.into()),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 30.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn perf_gate(c: &mut Criterion) {
    let node_hit = make_node("Counting");
    let node_miss = make_node("Something completely different");
    let regex_node = make_node("Week");

    // Compile once in setup — this is what resolver / driver do per
    // selector before entering the DFS loop. Compile cost itself is
    // benched separately in benches/matchtext.rs § "Pattern::compile".
    let text_cp = Pattern::text("Counting")
        .compile()
        .expect("text compile is infallible");
    let regex_cp = Pattern::regex("Day|Week|Month")
        .compile()
        .expect("regex compile must succeed");

    c.bench_function("match_text_compiled plain hit", |b| {
        b.iter(|| match_text_compiled(black_box(&node_hit), black_box(&text_cp)));
    });
    c.bench_function("match_text_compiled plain miss", |b| {
        b.iter(|| match_text_compiled(black_box(&node_miss), black_box(&text_cp)));
    });
    c.bench_function("match_text_compiled regex hit", |b| {
        b.iter(|| match_text_compiled(black_box(&regex_node), black_box(&regex_cp)));
    });
    c.bench_function("match_text_compiled regex miss", |b| {
        b.iter(|| match_text_compiled(black_box(&node_miss), black_box(&regex_cp)));
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
