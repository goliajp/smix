//! v3.1 c3 — criterion bench for smix-screen visibility primitives.
//!
//! Mirrors TS `src/core/__bench__/visibility.bench.ts` 6 cases so the
//! Rust vs TS numbers in PERFORMANCE.md §4 are apples-to-apples.
//!
//! Run: `cargo bench --bench visibility --package smix-screen`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_screen::{A11yNode, Rect, is_visible_enough, visible_area};
use std::hint::black_box;

fn mk(bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn build_tree_100() -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut children = Vec::with_capacity(9);
        for j in 0..9 {
            let mut child = mk(rect(
                20.0 + (j as f64) * 5.0,
                100.0 + (i as f64) * 30.0,
                50.0,
                25.0,
            ));
            child.label = Some(format!("c{}-{}", i, j));
            children.push(child);
        }
        let mut t = mk(rect(0.0, (i as f64) * 60.0, 390.0, 60.0));
        t.label = Some(format!("top-{}", i));
        t.children = children;
        top.push(t);
    }
    let mut root = mk(rect(0.0, 0.0, 390.0, 844.0));
    root.raw_type = "application".into();
    root.children = top;
    root
}

fn dfs_collect_visible<'a>(tree: &'a A11yNode, out: &mut Vec<&'a A11yNode>) {
    fn walk<'a>(n: &'a A11yNode, root: &'a A11yNode, out: &mut Vec<&'a A11yNode>) {
        if is_visible_enough(n, root) {
            out.push(n);
        }
        for c in &n.children {
            walk(c, root, out);
        }
    }
    walk(tree, tree, out)
}

fn bench_visibility(c: &mut Criterion) {
    let root_normal = mk(rect(0.0, 0.0, 390.0, 844.0));
    let root_unknown = mk(rect(0.0, 0.0, 0.0, 0.0));
    let node_in_view = mk(rect(20.0, 100.0, 200.0, 44.0));
    let node_zero = mk(rect(50.0, 50.0, 0.0, 0.0));
    let node_offscreen = mk(rect(500.0, 1000.0, 50.0, 50.0));
    let tree_100 = build_tree_100();

    let mut g = c.benchmark_group("visibility");
    g.bench_function("is_visible_enough — in-view (happy path)", |b| {
        b.iter(|| is_visible_enough(black_box(&node_in_view), black_box(&root_normal)))
    });
    g.bench_function("is_visible_enough — zero-bounds early reject", |b| {
        b.iter(|| is_visible_enough(black_box(&node_zero), black_box(&root_normal)))
    });
    g.bench_function(
        "is_visible_enough — unknown-root conservative pass",
        |b| b.iter(|| is_visible_enough(black_box(&node_in_view), black_box(&root_unknown))),
    );
    g.bench_function("is_visible_enough — offscreen", |b| {
        b.iter(|| is_visible_enough(black_box(&node_offscreen), black_box(&root_normal)))
    });
    g.bench_function("visible_area — intersection arithmetic", |b| {
        b.iter(|| visible_area(black_box(&node_in_view), black_box(&root_normal)))
    });
    g.bench_function("dfsCollect 100-node tree (composite)", |b| {
        b.iter(|| {
            let mut out = Vec::with_capacity(100);
            dfs_collect_visible(black_box(&tree_100), &mut out);
            black_box(out);
        })
    });
    g.finish();
}

criterion_group!(benches, bench_visibility);
criterion_main!(benches);
