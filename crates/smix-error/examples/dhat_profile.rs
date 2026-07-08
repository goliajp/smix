//! dhat-heap mem profile for `smix_error::build_suggestions` +
//! `ExpectationFailure::to_prompt`.
//!
//! Exercises the cold-path failure render × 10,000 iterations. Per-call
//! allocation count + bytes show how much the AI-readable failure prompt
//! costs the failure window.
//!
//! Run: `cargo run --example dhat_profile -p smix-error --release`

use smix_error::{ExpectationFailure, FailureCode, FailureInit, build_suggestions};
use smix_screen::{ElementSummary, Rect, Role};
use std::hint::black_box;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

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

fn main() {
    let visible = ten_summaries();
    let _profiler = dhat::Profiler::new_heap();
    for _ in 0..10_000 {
        let suggestions = build_suggestions(Some(black_box("Logn")), black_box(&visible));
        let f = ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::ElementNotFound),
            message: "Element not found: text=\"Logn\"".to_string(),
            selector: None,
            suggestions,
            visible_elements: visible.clone(),
            hint: Some("Try `app.wait_for(selector)` first".into()),
            screenshot: None,
            device_log: vec!["[2026-05-26 10:00:00] some log line".into()],
        });
        let p = f.to_prompt();
        black_box(p.len());
    }
}
