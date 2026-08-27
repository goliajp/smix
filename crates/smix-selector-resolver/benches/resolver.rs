//! Criterion bench for the smix-selector-resolver hot path, across
//! 7 cases.

use criterion::{Criterion, criterion_group, criterion_main};
use smix_screen::{A11yNode, Rect};
use smix_selector::{AnchorBox, IndexModifiers, Modifiers, Pattern, Selector};
use smix_selector_resolver::resolve_selector;
use std::hint::black_box;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn mk(label: Option<String>, bounds: Rect, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

fn mk_app(bounds: Rect, children: Vec<A11yNode>) -> A11yNode {
    let mut n = mk(None, bounds, children);
    n.raw_type = "application".into();
    n
}

fn build_tree_100() -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut children = Vec::with_capacity(9);
        for j in 0..9 {
            let label = if i == 6 && j == 7 {
                "Target".to_string()
            } else {
                format!("c{}-{}", i, j)
            };
            children.push(mk(
                Some(label),
                rect(
                    20.0 + (j as f64) * 5.0,
                    100.0 + (i as f64) * 30.0,
                    50.0,
                    25.0,
                ),
                vec![],
            ));
        }
        top.push(mk(
            Some(format!("top-{}", i)),
            rect(0.0, (i as f64) * 60.0, 390.0, 60.0),
            children,
        ));
    }
    mk_app(rect(0.0, 0.0, 390.0, 844.0), top)
}

fn build_tree_anchor_below() -> A11yNode {
    mk_app(
        rect(0.0, 0.0, 390.0, 844.0),
        vec![
            mk(Some("Anchor".into()), rect(0.0, 50.0, 10.0, 10.0), vec![]),
            mk(Some("Below1".into()), rect(0.0, 100.0, 10.0, 10.0), vec![]),
            mk(Some("Below2".into()), rect(0.0, 150.0, 10.0, 10.0), vec![]),
            mk(Some("Below3".into()), rect(0.0, 200.0, 10.0, 10.0), vec![]),
        ],
    )
}

fn build_tree_multi_x() -> A11yNode {
    let children = (0..5)
        .map(|i| {
            mk(
                Some("X".into()),
                rect(0.0, (i as f64) * 30.0, 10.0, 10.0),
                vec![],
            )
        })
        .collect();
    mk_app(rect(0.0, 0.0, 390.0, 844.0), children)
}

fn bench_resolve(c: &mut Criterion) {
    let tree = build_tree_100();
    let tree_anchor = build_tree_anchor_below();
    let tree_multi = build_tree_multi_x();

    let sel_text_hit = Selector::Text {
        text: Pattern::text("Target"),
        modifiers: Modifiers::default(),
    };
    let sel_text_miss = Selector::Text {
        text: Pattern::text("NotInTree"),
        modifiers: Modifiers::default(),
    };
    let sel_text_below = Selector::Text {
        text: Pattern::text("Below1"),
        modifiers: Modifiers {
            below: Some(Box::new(Selector::Text {
                text: Pattern::text("Anchor"),
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    let sel_id_miss = Selector::Id {
        id: "btn-nothere".into(),
        modifiers: Modifiers::default(),
    };
    let sel_regex_hit = Selector::Text {
        text: Pattern::regex("^Target$"),
        modifiers: Modifiers::default(),
    };
    let sel_anchor_below_nth = Selector::Anchor {
        anchor: AnchorBox {
            below: Some(Box::new(Selector::Text {
                text: Pattern::text("Anchor"),
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
        index: IndexModifiers {
            nth: Some(1),
            ..Default::default()
        },
    };
    let sel_text_nth = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            nth: Some(2),
            ..Default::default()
        },
    };

    let mut g = c.benchmark_group("resolve_selector");
    g.bench_function("text base, 100-node tree, label hit (DFS depth 2)", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_text_hit)))
    });
    g.bench_function("text base, 100-node tree, miss (full DFS)", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_text_miss)))
    });
    g.bench_function(
        "text base + below modifier, 4-node tree, anchor resolve + spatial",
        |b| b.iter(|| resolve_selector(black_box(&tree_anchor), black_box(&sel_text_below))),
    );
    g.bench_function("id base, 100-node tree, miss (no identifier set)", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_id_miss)))
    });
    g.bench_function("regex text base, 100-node tree, label hit", |b| {
        b.iter(|| resolve_selector(black_box(&tree), black_box(&sel_regex_hit)))
    });
    g.bench_function("anchor-only base + below, 4-node tree, nth=1", |b| {
        b.iter(|| resolve_selector(black_box(&tree_anchor), black_box(&sel_anchor_below_nth)))
    });
    g.bench_function("text base + nth=2, multi-match 5-X tree", |b| {
        b.iter(|| resolve_selector(black_box(&tree_multi), black_box(&sel_text_nth)))
    });
    g.finish();
}

criterion_group!(benches, bench_resolve);
criterion_main!(benches);
