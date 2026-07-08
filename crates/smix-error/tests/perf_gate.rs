#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! v3.3 c2 — perf gate for smix-error cold-path primitives.
//!
//! Cold path (only fires on assertion failure) but must stay tight so
//! AI-readable failure rendering doesn't dominate the failure window.
//! Numbers come from `cargo bench --bench error` + 3-5× headroom.

use smix_error::{
    ExpectationFailure, FailureCode, FailureInit, build_suggestions, edit_distance, similarity,
};
use smix_screen::{ElementSummary, Rect, Role};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 100_000;
const WARMUP_FRAC: u32 = 10;

fn measure_ns<F: FnMut()>(mut body: F, iterations: u32) -> f64 {
    let warmup = iterations / WARMUP_FRAC;
    for _ in 0..warmup {
        body();
    }
    let measured = iterations - warmup;
    let start = Instant::now();
    for _ in 0..measured {
        body();
    }
    let elapsed = start.elapsed();
    elapsed.as_nanos() as f64 / measured as f64
}

fn mk_summary(name: &str, role: Option<Role>) -> ElementSummary {
    ElementSummary {
        role,
        name: Some(name.to_string()),
        id: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
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

#[test]
fn perf_gate_edit_distance_short_under_300ns() {
    let ns = measure_ns(
        || {
            black_box(edit_distance(black_box("Login"), black_box("Logon")));
        },
        ITERATIONS,
    );
    assert!(
        ns < 300.0,
        "edit_distance short typo exceeded 300 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_similarity_short_under_400ns() {
    let ns = measure_ns(
        || {
            black_box(similarity(black_box("Login"), black_box("Logon")));
        },
        ITERATIONS,
    );
    assert!(
        ns < 400.0,
        "similarity short typo exceeded 400 ns budget: {:.2} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_build_suggestions_10_under_100us() {
    let visible = ten_summaries();
    let ns = measure_ns(
        || {
            black_box(build_suggestions(
                Some(black_box("Logn")),
                black_box(&visible),
            ));
        },
        ITERATIONS / 10,
    );
    assert!(
        ns < 100_000.0,
        "build_suggestions 10-element exceeded 100 μs budget: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_to_prompt_full_under_150us() {
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
    let ns = measure_ns(
        || {
            black_box(f.to_prompt());
        },
        ITERATIONS / 10,
    );
    assert!(
        ns < 150_000.0,
        "to_prompt full failure exceeded 150 μs budget: {:.0} ns/iter",
        ns
    );
}
