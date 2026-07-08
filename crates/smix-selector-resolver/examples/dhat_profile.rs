//! dhat-heap mem profile for `smix_selector_resolver::resolve_selector`.
//!
//! Exercises the full DFS resolve pipeline (text selector against
//! 100-node tree) × 10,000 iterations. ResolverContext compile cache
//! + DFS candidate Vec are the dominant per-call allocations.
//!
//! Run: `cargo run --example dhat_profile -p smix-selector-resolver --release`

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::resolve_selector;
use std::hint::black_box;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn mk(label: &str, bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
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

fn build_tree_100() -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut kids = Vec::with_capacity(9);
        for j in 0..9 {
            let label = if i == 5 && j == 4 {
                "Login".to_string()
            } else {
                format!("c{}-{}", i, j)
            };
            kids.push(mk(
                &label,
                Rect {
                    x: 20.0 + (j as f64) * 5.0,
                    y: 100.0 + (i as f64) * 30.0,
                    w: 50.0,
                    h: 25.0,
                },
            ));
        }
        let mut t = mk(
            &format!("top-{}", i),
            Rect {
                x: 0.0,
                y: (i as f64) * 60.0,
                w: 390.0,
                h: 60.0,
            },
        );
        t.children = kids;
        top.push(t);
    }
    let mut root = mk(
        "root",
        Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        },
    );
    root.children = top;
    root
}

fn main() {
    let tree = build_tree_100();
    let sel = Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers::default(),
    };
    let _profiler = dhat::Profiler::new_heap();
    for _ in 0..10_000 {
        let r = resolve_selector(black_box(&tree), black_box(&sel));
        black_box(r);
    }
}
