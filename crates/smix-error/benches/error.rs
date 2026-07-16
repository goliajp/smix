//! Criterion bench for smix-error cold-path primitives.
//!
//! These are cold-path (only run on assertion failure) but suggestions+
//! to_prompt go to AI-readable output which the user reads — needs to be
//! fast enough that adding it to a failure path doesn't dominate.
//!
//! Run: `cargo bench --bench error -p smix-error`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_error::{
    ExpectationFailure, FailureCode, FailureInit, build_suggestions, edit_distance, similarity,
};
use smix_screen::{ElementSummary, Rect, Role};
use std::hint::black_box;

fn mk_summary(name: &str, role: Option<Role>) -> ElementSummary {
    ElementSummary {
        role,
        name: Some(name.to_string()),
        id: None,
        text: None,
        bounds: Rect {
            x: 50.0,
            y: 100.0,
            w: 200.0,
            h: 40.0,
        },
        enabled: true,
    }
}

fn ten_summaries() -> Vec<ElementSummary> {
    vec![
        mk_summary("Login", Some(Role::Button)),
        mk_summary("Logout", Some(Role::Button)),
        mk_summary("Settings", Some(Role::Button)),
        mk_summary("Profile", Some(Role::Button)),
        mk_summary("Username", Some(Role::TextField)),
        mk_summary("Password", Some(Role::SecureTextField)),
        mk_summary("Search", Some(Role::SearchField)),
        mk_summary("Cancel", Some(Role::Button)),
        mk_summary("Submit", Some(Role::Button)),
        mk_summary("Help", Some(Role::Link)),
    ]
}

fn bench_edit_distance(c: &mut Criterion) {
    c.bench_function("edit_distance short equal", |b| {
        b.iter(|| edit_distance(black_box("Login"), black_box("Login")))
    });
    c.bench_function("edit_distance short typo", |b| {
        b.iter(|| edit_distance(black_box("Login"), black_box("Logon")))
    });
    c.bench_function("edit_distance medium", |b| {
        b.iter(|| edit_distance(black_box("Settings"), black_box("Setings")))
    });
}

fn bench_similarity(c: &mut Criterion) {
    c.bench_function("similarity short typo", |b| {
        b.iter(|| similarity(black_box("Login"), black_box("Logon")))
    });
}

fn bench_build_suggestions(c: &mut Criterion) {
    let visible = ten_summaries();
    c.bench_function("build_suggestions 10-element list", |b| {
        b.iter(|| build_suggestions(Some(black_box("Logn")), black_box(&visible)))
    });
}

fn bench_to_prompt(c: &mut Criterion) {
    let visible = ten_summaries();
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "Element not found: text=\"Logn\"".to_string(),
        selector: None,
        suggestions: vec!["Did you mean \"Login\"? (similarity 0.80)".into()],
        visible_elements: visible,
        hint: Some("Try `app.wait_for(selector)` first".into()),
        screenshot: None,
        device_log: vec!["[2026-05-26 10:00:00] some log line".into()],
    });
    c.bench_function("to_prompt full failure", |b| {
        b.iter(|| black_box(&f).to_prompt())
    });
}

criterion_group!(
    benches,
    bench_edit_distance,
    bench_similarity,
    bench_build_suggestions,
    bench_to_prompt
);
criterion_main!(benches);
