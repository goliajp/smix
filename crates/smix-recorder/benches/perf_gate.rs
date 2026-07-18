//! Hot path: `generate_rust` is the only pure-synchronous transform on
//! the recorder surface — `IRAction[]` → idiomatic `#[tokio::test]`
//! Rust source string. Everything else in `smix-recorder` is async +
//! io-bound (session capture, claude CLI cleanup). Bench input is a
//! realistic 6-action slice covering the most common variants (tap /
//! fill / press_key) so the numbers track the AI-recorder fast-path
//! cost, not a degenerate one-action stub.
//!
//! Run: `cargo bench --bench perf_gate -p smix-recorder`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_authoring_ir::IRAction;
use smix_input::KeyName;
use smix_recorder::generator_rust::generate_rust;
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;

fn text_selector(s: &str) -> Selector {
    Selector::Text {
        text: Pattern::Text(s.to_string()),
        modifiers: Modifiers::default(),
    }
}

fn id_selector(s: &str) -> Selector {
    Selector::Id {
        id: s.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn sample_actions() -> Vec<IRAction> {
    vec![
        IRAction::Tap {
            selector: text_selector("Login"),
            timestamp_ms: 0.0,
        },
        IRAction::Fill {
            selector: id_selector("email-input"),
            text: "user@example.com".to_string(),
            timestamp_ms: 10.0,
        },
        IRAction::Fill {
            selector: id_selector("password-input"),
            text: "hunter2".to_string(),
            timestamp_ms: 20.0,
        },
        IRAction::PressKey {
            key: KeyName::Return,
            timestamp_ms: 30.0,
        },
        IRAction::WaitFor {
            selector: text_selector("Welcome"),
            timestamp_ms: 40.0,
        },
        IRAction::Tap {
            selector: text_selector("Continue"),
            timestamp_ms: 50.0,
        },
    ]
}

fn perf_gate(c: &mut Criterion) {
    let actions = sample_actions();

    c.bench_function("generate_rust 6-action slice", |b| {
        b.iter(|| {
            generate_rust(
                black_box(&actions),
                black_box("recorded_login"),
                black_box("com.example.app"),
            )
        });
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
