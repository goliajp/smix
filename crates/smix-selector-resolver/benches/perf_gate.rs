//! Perf gate bench for the full resolver pipeline.
//!
//! `ResolverContext` holds `compiled: HashMap<*const Pattern,
//! CompiledPattern>`, populated once per `resolve_selector` call by
//! `cache_pattern` / `compile_anchor`, and the hot DFS calls
//! `match_text_compiled(node, cp)` per candidate.
//!
//! Cases mirror the selector-side `perf_gate.rs` layout (plain
//! hit / regex hit / miss) but bench the **full resolver pipeline**:
//! `resolve_selector` builds the `ResolverContext` (cache prepass over
//! the selector tree), runs `dfs_collect` over the a11y tree
//! (15 node), applies visibility / modal / ancestor / spatial /
//! tappable / index filters, returns the first survivor. The numbers
//! tier above selector-side `match_text_compiled` (~5 ns) because the
//! resolver adds cache build, DFS frame overhead, visibility filter,
//! tappable filter, and index pick on top. Expected tier: ns ~ low µs
//! for a 15-node tree.
//!
//! Run: `cargo bench --bench perf_gate -p smix-selector-resolver`
//!
//! # ctx-reused path
//!
//! Mirrors the 3 baseline cases with `resolve_selector_compiled` after
//! building a single `ResolverContext` outside the `b.iter` loop. This
//! is the production retry-loop pattern (driver `wait_for` / `scroll`).
//! The plain hit case sees a marginal speedup (HashMap lookup already
//! ~ns); the regex hit case sees the big win (regex compile lifted out
//! of the hot loop). Validates the cross-call cache optimization.

use criterion::{Criterion, criterion_group, criterion_main};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::{ResolverContext, resolve_selector, resolve_selector_compiled};
use std::hint::black_box;

fn mk_leaf(label: &str, y: f64) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 20.0,
            y,
            w: 100.0,
            h: 25.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn mk_app(children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "application".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

// 15-node tree: 1 app root + 14 labelled leaves; "Target" is the hit
// node sitting near the middle of the DFS pre-order (forces partial
// walk before the match, neither degenerate first-frame nor full-walk
// rejection).
fn build_tree() -> A11yNode {
    let mut leaves: Vec<A11yNode> = (0..14)
        .map(|i| {
            let label = if i == 7 {
                "Target".to_string()
            } else {
                format!("leaf-{}", i)
            };
            mk_leaf(&label, 50.0 + (i as f64) * 30.0)
        })
        .collect();
    leaves.shrink_to_fit();
    mk_app(leaves)
}

fn perf_gate(c: &mut Criterion) {
    let tree = build_tree();

    let sel_plain_hit = Selector::Text {
        text: Pattern::text("Target"),
        modifiers: Modifiers::default(),
    };
    let sel_regex_hit = Selector::Text {
        text: Pattern::regex("^Target$"),
        modifiers: Modifiers::default(),
    };
    let sel_miss = Selector::Text {
        text: Pattern::text("NotInTree"),
        modifiers: Modifiers::default(),
    };

    c.bench_function("resolve_selector plain hit (15-node tree)", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_plain_hit)));
    });
    c.bench_function("resolve_selector regex hit (15-node tree)", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_regex_hit)));
    });
    c.bench_function("resolve_selector miss (15-node tree, full DFS)", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_miss)));
    });

    // ctx-reused variants. Build context once outside `b.iter` so the
    // regex compile prepass is paid at setup, not per iteration —
    // production pattern in `smix-driver::wait_for` / `scroll`.
    let ctx_plain = ResolverContext::new(&sel_plain_hit).expect("plain compiles");
    let ctx_regex = ResolverContext::new(&sel_regex_hit).expect("regex compiles");
    let ctx_miss = ResolverContext::new(&sel_miss).expect("miss plain compiles");
    c.bench_function(
        "resolve_selector_compiled plain hit (15-node tree, ctx-reused)",
        |b| {
            b.iter(|| {
                resolve_selector_compiled(
                    black_box(&tree),
                    black_box(&sel_plain_hit),
                    black_box(&ctx_plain),
                )
            });
        },
    );
    c.bench_function(
        "resolve_selector_compiled regex hit (15-node tree, ctx-reused)",
        |b| {
            b.iter(|| {
                resolve_selector_compiled(
                    black_box(&tree),
                    black_box(&sel_regex_hit),
                    black_box(&ctx_regex),
                )
            });
        },
    );
    c.bench_function(
        "resolve_selector_compiled miss (15-node tree, ctx-reused, full DFS)",
        |b| {
            b.iter(|| {
                resolve_selector_compiled(
                    black_box(&tree),
                    black_box(&sel_miss),
                    black_box(&ctx_miss),
                )
            });
        },
    );
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
