#![cfg(not(debug_assertions))] // perf budgets are release-only + load-sensitive (test-optimize.md §2.4)
//! Perf gate for smix-runner-wire serde encode/decode.

use smix_runner_wire::{
    SystemPopup, SystemPopupButton, TapAtNormCoordRequest, TapMode, TapRequest, TapResult,
    TapStages,
};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u32 = 50_000;
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

fn mk_node(label: &str, bounds: Rect) -> A11yNode {
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

fn build_tree_100() -> A11yNode {
    let mut top = Vec::with_capacity(10);
    for i in 0..10 {
        let mut kids = Vec::with_capacity(9);
        for j in 0..9 {
            kids.push(mk_node(
                &format!("c{}-{}", i, j),
                Rect {
                    x: 20.0 + (j as f64) * 5.0,
                    y: 100.0 + (i as f64) * 30.0,
                    w: 50.0,
                    h: 25.0,
                },
            ));
        }
        let mut t = mk_node(
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
    let mut root = mk_node(
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

#[test]
fn perf_gate_tap_request_encode_under_2us() {
    let req = TapRequest {
        selector: Selector::Text {
            text: Pattern::text("Login"),
            modifiers: Modifiers::default(),
        },
        mode: TapMode::Resolve,
    };
    let ns = measure_ns(
        || {
            black_box(serde_json::to_string(black_box(&req)).unwrap());
        },
        ITERATIONS,
    );
    assert!(
        ns < 2_000.0,
        "TapRequest encode exceeded 2 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_tap_result_decode_under_5us() {
    let r = TapResult {
        matched_label: Some("Sign In".to_string()),
        stages: Some(TapStages {
            resolve_ms: 12.3,
            tap_call_ms: 4.5,
            total_ms: 17.1,
            wait_existence_ms: 0.0,
            frame_read_ms: 0.7,
        }),
        frame: Some(Rect {
            x: 50.0,
            y: 100.0,
            w: 200.0,
            h: 40.0,
        }),
        app_frame: Some(Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        }),
    };
    let json = serde_json::to_string(&r).unwrap();
    let ns = measure_ns(
        || {
            let _: TapResult = serde_json::from_str(black_box(&json)).unwrap();
        },
        ITERATIONS,
    );
    assert!(
        ns < 5_000.0,
        "TapResult decode exceeded 5 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_tap_at_norm_coord_encode_under_500ns() {
    let req = TapAtNormCoordRequest { nx: 0.5, ny: 0.25 };
    let ns = measure_ns(
        || {
            black_box(serde_json::to_string(black_box(&req)).unwrap());
        },
        ITERATIONS,
    );
    assert!(
        ns < 500.0,
        "TapAtNormCoordRequest encode exceeded 500 ns: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_system_popup_decode_under_10us() {
    let p = SystemPopup {
        id: "popup-1".into(),
        kind: "alert".into(),
        source: "com.apple.springboard".into(),
        title: "Allow access to your location?".into(),
        body: "MyApp wants to use your location to show nearby places.".into(),
        buttons: vec![
            SystemPopupButton {
                id: "cancel".into(),
                label: "Don't Allow".into(),
                role: "cancel".into(),
                dangerous: false,
                outcome_hint: None,
            },
            SystemPopupButton {
                id: "ok".into(),
                label: "Allow".into(),
                role: "default".into(),
                dangerous: false,
                outcome_hint: Some("grants location permission".into()),
            },
        ],
    };
    let json = serde_json::to_string(&p).unwrap();
    let ns = measure_ns(
        || {
            let _: SystemPopup = serde_json::from_str(black_box(&json)).unwrap();
        },
        ITERATIONS / 5,
    );
    assert!(
        ns < 10_000.0,
        "SystemPopup decode exceeded 10 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_a11y_tree_100_encode_under_100us() {
    let tree = build_tree_100();
    let ns = measure_ns(
        || {
            black_box(serde_json::to_string(black_box(&tree)).unwrap());
        },
        ITERATIONS / 20,
    );
    assert!(
        ns < 100_000.0,
        "A11yNode 100-node encode exceeded 100 μs: {:.0} ns/iter",
        ns
    );
}

#[test]
fn perf_gate_a11y_tree_100_decode_under_400us() {
    let tree = build_tree_100();
    let json = serde_json::to_string(&tree).unwrap();
    let ns = measure_ns(
        || {
            let _: A11yNode = serde_json::from_str(black_box(&json)).unwrap();
        },
        ITERATIONS / 20,
    );
    assert!(
        ns < 400_000.0,
        "A11yNode 100-node decode exceeded 400 μs: {:.0} ns/iter",
        ns
    );
}
