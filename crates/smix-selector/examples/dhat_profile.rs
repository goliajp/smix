//! dhat-heap mem profile for `smix_selector::Pattern::compile` +
//! `match_text_compiled`.
//!
//! Two phases: (1) compile the regex pattern once (one-time alloc),
//! then (2) loop `match_text_compiled` × 10,000 against a synth node —
//! the hot-path call must stay zero-alloc to amortize.
//!
//! Run: `cargo run --example dhat_profile -p smix-selector --release`

use smix_screen::{A11yNode, Rect};
use smix_selector::{Pattern, match_text_compiled};
use std::hint::black_box;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn synth_node(label: &str) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        role: None,
        identifier: None,
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 40.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn main() {
    let _profiler = dhat::Profiler::new_heap();
    // Phase 1: one-time compile.
    let pat = Pattern::regex("^Login$");
    let compiled = pat.compile().expect("regex compile");
    // Phase 2: 10,000 hot-loop matches. Each match should be zero-alloc.
    let node = synth_node("Login");
    for _ in 0..10_000 {
        black_box(match_text_compiled(black_box(&node), black_box(&compiled)));
    }
}
