//! Criterion bench for smix-selector `match_text`, across 6 cases.
//!
//! Note: regex variants compile inside `match_text` (no cache at the
//! stone layer). Numbers therefore reflect compile + match overhead per
//! call; the cache layer lives in `smix-selector-resolver`.
//!
//! Run: `cargo bench --bench matchtext --package smix-selector`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Pattern, match_text_compiled};
use std::hint::black_box;

fn mk_with(field: NodeField) -> A11yNode {
    let mut n = A11yNode {
        hittable: None,
        raw_type: "other".into(),
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
            w: 10.0,
            h: 10.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    };
    match field {
        NodeField::Label(s) => n.label = Some(s),
        NodeField::Identifier(s) => n.identifier = Some(s),
        NodeField::LabelTitle(label, title) => {
            n.label = Some(label);
            n.title = Some(title);
        }
    }
    n
}

enum NodeField {
    Label(String),
    Identifier(String),
    LabelTitle(String, String),
}

fn bench_match_text(c: &mut Criterion) {
    let node_label = mk_with(NodeField::Label("Settings".to_string()));
    let node_ident = mk_with(NodeField::Identifier("btn-settings".to_string()));
    let node_miss = mk_with(NodeField::LabelTitle("Dashboard".into(), "Main".into()));
    let node_long = mk_with(NodeField::Label(
        "Settings panel for advanced user configuration and preferences UI".to_string(),
    ));

    let p_text_hit = Pattern::text("settings");
    let p_text_miss = Pattern::text("NotInTree");
    let p_text_ident = Pattern::text("btn-settings");
    let p_regex_default = Pattern::regex_with_flags("^Set", "");
    let p_regex_explicit = Pattern::regex_with_flags("^set", "i");
    let p_regex_long = Pattern::regex_with_flags("preferences", "i");

    // Pre-compile (hot path: caller of match_text_compiled would call this
    // once per selector and reuse). Compilation cost itself is captured in
    // the separate "Pattern::compile()" bench below.
    let cp_text_hit = p_text_hit.compile().unwrap();
    let cp_text_miss = p_text_miss.compile().unwrap();
    let cp_text_ident = p_text_ident.compile().unwrap();
    let cp_regex_default = p_regex_default.compile().unwrap();
    let cp_regex_explicit = p_regex_explicit.compile().unwrap();
    let cp_regex_long = p_regex_long.compile().unwrap();

    let mut g = c.benchmark_group("match_text_compiled (hot path, regex cache)");
    g.bench_function(
        "string strict equal — label field 1 hit (early DFS)",
        |b| b.iter(|| match_text_compiled(black_box(&node_label), black_box(&cp_text_hit))),
    );
    g.bench_function(
        "string strict equal — miss (full 6-field scan + 6× case-fold)",
        |b| b.iter(|| match_text_compiled(black_box(&node_miss), black_box(&cp_text_miss))),
    );
    g.bench_function(
        "string strict equal — identifier field 5 hit (late DFS)",
        |b| b.iter(|| match_text_compiled(black_box(&node_ident), black_box(&cp_text_ident))),
    );
    g.bench_function("RegExp default — pre-compiled match — label hit", |b| {
        b.iter(|| match_text_compiled(black_box(&node_label), black_box(&cp_regex_default)))
    });
    g.bench_function(
        "RegExp with /i explicit — pre-compiled match — label hit",
        |b| b.iter(|| match_text_compiled(black_box(&node_label), black_box(&cp_regex_explicit))),
    );
    g.bench_function("RegExp on 100-char label — pre-compiled match", |b| {
        b.iter(|| match_text_compiled(black_box(&node_long), black_box(&cp_regex_long)))
    });
    g.finish();

    // Compile cost (one-time, amortized across resolver loop iters).
    let mut g2 = c.benchmark_group("Pattern::compile (one-time)");
    g2.bench_function("text — trivial clone", |b| {
        b.iter(|| black_box(&p_text_hit).compile().unwrap())
    });
    g2.bench_function("regex — short pattern compile", |b| {
        b.iter(|| black_box(&p_regex_explicit).compile().unwrap())
    });
    g2.finish();
}

criterion_group!(benches, bench_match_text);
criterion_main!(benches);
