//! Criterion bench for smix-runner-wire serde encode/decode.
//!
//! Every runner HTTP roundtrip pays an encode + decode. Large payloads
//! (100-node A11yNode tree) dominate; small ones (TapRequest) are noise
//! but kept as a sanity ceiling.
//!
//! Run: `cargo bench --bench wire -p smix-runner-wire`

use criterion::{Criterion, criterion_group, criterion_main};
use smix_runner_wire::{
    SystemPopup, SystemPopupButton, TapAtNormCoordRequest, TapMode, TapRequest, TapResult,
    TapStages,
};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::hint::black_box;

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

fn bench_tap_request(c: &mut Criterion) {
    let req = TapRequest {
        selector: Selector::Text {
            text: Pattern::text("Login"),
            modifiers: Modifiers::default(),
        },
        mode: TapMode::Resolve,
    };
    let json = serde_json::to_string(&req).unwrap();
    c.bench_function("TapRequest encode", |b| {
        b.iter(|| serde_json::to_string(black_box(&req)).unwrap())
    });
    c.bench_function("TapRequest decode", |b| {
        b.iter(|| {
            let r: TapRequest = serde_json::from_str(black_box(&json)).unwrap();
            black_box(r);
        })
    });
}

fn bench_tap_result(c: &mut Criterion) {
    let r = TapResult {
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
    c.bench_function("TapResult encode", |b| {
        b.iter(|| serde_json::to_string(black_box(&r)).unwrap())
    });
    c.bench_function("TapResult decode", |b| {
        b.iter(|| {
            let v: TapResult = serde_json::from_str(black_box(&json)).unwrap();
            black_box(v);
        })
    });
}

fn bench_tap_at_norm_coord(c: &mut Criterion) {
    let req = TapAtNormCoordRequest { nx: 0.5, ny: 0.25 };
    let json = serde_json::to_string(&req).unwrap();
    c.bench_function("TapAtNormCoordRequest encode", |b| {
        b.iter(|| serde_json::to_string(black_box(&req)).unwrap())
    });
    c.bench_function("TapAtNormCoordRequest decode", |b| {
        b.iter(|| {
            let v: TapAtNormCoordRequest = serde_json::from_str(black_box(&json)).unwrap();
            black_box(v);
        })
    });
}

fn bench_system_popup(c: &mut Criterion) {
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
    c.bench_function("SystemPopup encode", |b| {
        b.iter(|| serde_json::to_string(black_box(&p)).unwrap())
    });
    c.bench_function("SystemPopup decode", |b| {
        b.iter(|| {
            let v: SystemPopup = serde_json::from_str(black_box(&json)).unwrap();
            black_box(v);
        })
    });
}

fn bench_a11y_tree_100(c: &mut Criterion) {
    let tree = build_tree_100();
    let json = serde_json::to_string(&tree).unwrap();
    c.bench_function("A11yNode 100-node encode", |b| {
        b.iter(|| serde_json::to_string(black_box(&tree)).unwrap())
    });
    c.bench_function("A11yNode 100-node decode", |b| {
        b.iter(|| {
            let v: A11yNode = serde_json::from_str(black_box(&json)).unwrap();
            black_box(v);
        })
    });
}

criterion_group!(
    benches,
    bench_tap_request,
    bench_tap_result,
    bench_tap_at_norm_coord,
    bench_system_popup,
    bench_a11y_tree_100
);
criterion_main!(benches);
