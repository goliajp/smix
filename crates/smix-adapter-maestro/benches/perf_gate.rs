//! Hot path: `parse_flow_yaml` is the single per-flow entry point for
//! every `smix-maestro` invocation; the per-step helpers
//! `text_to_pattern` + `visible_to_selector` run once per step inside
//! the parse loop. Together they bound the host-side cost before the
//! adapter starts dispatching actions to the runner.
//!
//! Three `parse_flow_yaml` workloads bracket real usage:
//!
//! - SMALL_FLOW: 4 steps, no subflows — the floor case.
//! - TYPICAL_FLOW: ~13 steps, full-form tapOn / extendedWaitUntil /
//!   pressKey — mirrors a login flow.
//! - LARGE_FLOW: ~28 steps with 4 `runFlow` subflow includes (one
//!   conditional `when.visible`) + scrollUntilVisible.
//!   `parse_flow_yaml` parses the `runFlow` step structurally; it does
//!   NOT load the referenced files, so the bench has no filesystem
//!   dependency.
//!
//! The shorthand-helper benches keep their input small so the numbers
//! stay stable across runs.
//!
//! Run: `cargo bench --bench perf_gate -p smix-adapter-maestro`

use criterion::{Criterion, criterion_group, criterion_main};
use serde_norway::Value;
use smix_adapter_maestro::{parse_flow_yaml, text_to_pattern, visible_to_selector};
use std::hint::black_box;

const SMALL_FLOW: &str = "appId: com.example.app\n\
---\n\
- launchApp\n\
- tapOn: \"Login\"\n\
- inputText: \"user@example.com\"\n\
- assertVisible: \"Welcome\"\n";

const TYPICAL_FLOW: &str = r#"appId: com.example.app
---
- launchApp:
    appId: com.example.app
- tapOn: "Log in"
- extendedWaitUntil:
    visible:
      id: input-email
    timeout: 10000
- tapOn:
    id: input-email
- inputText: "user@example.com"
- pressKey: enter
- tapOn: "Continue"
- tapOn:
    id: input-password
- inputText: "secret123"
- waitForAnimationToEnd
- tapOn:
    id: btn-login
- extendedWaitUntil:
    visible:
      id: btn-open-menu
    timeout: 30000
- assertVisible: "Dashboard"
"#;

const LARGE_FLOW: &str = r#"appId: com.example.app
---
- runFlow: ../../subflows/launch-warm.yaml
- runFlow: ../../subflows/ensure-login.yaml
- runFlow:
    when:
      visible: "Open menu"
    file: ../../subflows/open-menu.yaml
- tapOn: "Add Device"
- extendedWaitUntil:
    visible:
      id: screen-add-device
    timeout: 30000
- assertVisible: "Add Device"
- tapOn: "Enter code"
- waitForAnimationToEnd
- tapOn:
    id: input-sn
- inputText: "ABCD"
- eraseText: 10
- inputText: "ABCD1234EFGH"
- waitForAnimationToEnd
- assertVisible: "Add Device"
- scrollUntilVisible:
    element:
      text: "Network"
- tapOn: "Network"
- runFlow: ../../subflows/network-select.yaml
- tapOn:
    id: btn-wifi-next
- inputText: "wifipassword"
- pressKey: enter
- waitForAnimationToEnd
- assertVisible: "Connecting"
- extendedWaitUntil:
    visible:
      text: "Success"
    timeout: 60000
- assertVisible: "Device added"
- tapOn: "Done"
- runFlow: ../../subflows/device-detail.yaml
"#;

fn perf_gate(c: &mut Criterion) {
    // Setup-time guard: the typical/large workloads must parse (Ok) so the
    // bench measures the success path, not an early error return. A syntax
    // drift surfaces here as a bench-launch panic, not silently-skewed nums.
    assert!(
        parse_flow_yaml(TYPICAL_FLOW).is_ok(),
        "TYPICAL_FLOW must parse"
    );
    assert!(parse_flow_yaml(LARGE_FLOW).is_ok(), "LARGE_FLOW must parse");

    c.bench_function("parse_flow_yaml 4-step", |b| {
        b.iter(|| parse_flow_yaml(black_box(SMALL_FLOW)));
    });
    c.bench_function("parse_flow_yaml typical 13-step", |b| {
        b.iter(|| parse_flow_yaml(black_box(TYPICAL_FLOW)));
    });
    c.bench_function("parse_flow_yaml large 28-step 4-subflow", |b| {
        b.iter(|| parse_flow_yaml(black_box(LARGE_FLOW)));
    });

    c.bench_function("text_to_pattern plain literal", |b| {
        b.iter(|| text_to_pattern(black_box("Login")));
    });

    let visible_map: Value = serde_norway::from_str("text: Welcome\n")
        .expect("static yaml literal must parse for visible_to_selector bench input");
    c.bench_function("visible_to_selector text-shorthand", |b| {
        b.iter(|| visible_to_selector(black_box(&visible_map)));
    });
}

criterion_group!(benches, perf_gate);
criterion_main!(benches);
