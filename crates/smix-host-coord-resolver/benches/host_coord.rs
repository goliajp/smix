//! Criterion bench for smix-host-coord-resolver.
//!
//! End-to-end resolve-to-normalized-coord pipeline. Hot path — every
//! driver.tap runs this once before injecting the tap event.
//!
//! Run: `cargo bench --bench host_coord -p smix-host-coord-resolver`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_host_coord_resolver::resolve_to_norm_coord;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;

fn mk(label: &str, bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: Some(label.into()),
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

fn build_tree_100(target_label: &str) -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut children = Vec::with_capacity(9);
        for j in 0..9 {
            let label = if i == 5 && j == 4 {
                target_label.to_string()
            } else {
                format!("c{}-{}", i, j)
            };
            children.push(mk(
                &label,
                rect(
                    20.0 + (j as f64) * 5.0,
                    100.0 + (i as f64) * 30.0,
                    50.0,
                    25.0,
                ),
            ));
        }
        let mut t = mk(
            &format!("top-{}", i),
            rect(0.0, (i as f64) * 60.0, 390.0, 60.0),
        );
        t.children = children;
        top.push(t);
    }
    let mut root = mk("root", rect(0.0, 0.0, 390.0, 844.0));
    root.children = top;
    root
}

fn bench_hit_path(c: &mut Criterion) {
    let tree = build_tree_100("Login");
    let sel = Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers::default(),
    };
    c.bench_function("resolve_to_norm_coord text 100-node hit", |b| {
        b.iter(|| resolve_to_norm_coord(black_box(&tree), black_box(&sel)))
    });
}

fn bench_miss_path(c: &mut Criterion) {
    let tree = build_tree_100("Login");
    let sel = Selector::Text {
        text: Pattern::text("NonexistentXYZ"),
        modifiers: Modifiers::default(),
    };
    c.bench_function("resolve_to_norm_coord miss", |b| {
        b.iter(|| resolve_to_norm_coord(black_box(&tree), black_box(&sel)))
    });
}

fn bench_id_hit(c: &mut Criterion) {
    let mut tree = build_tree_100("anything");
    tree.children[5].children[4].identifier = Some("login-btn".into());
    let sel = Selector::Id {
        id: "login-btn".into(),
        modifiers: Modifiers::default(),
    };
    c.bench_function("resolve_to_norm_coord id 100-node hit", |b| {
        b.iter(|| resolve_to_norm_coord(black_box(&tree), black_box(&sel)))
    });
}

criterion_group!(benches, bench_hit_path, bench_miss_path, bench_id_hit);
criterion_main!(benches);
