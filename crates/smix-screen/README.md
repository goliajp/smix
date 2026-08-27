# smix-screen

[![Crates.io](https://img.shields.io/crates/v/smix-screen?style=flat-square&logo=rust)](https://crates.io/crates/smix-screen)
[![docs.rs](https://img.shields.io/docsrs/smix-screen?style=flat-square&logo=docs.rs)](https://docs.rs/smix-screen)
[![License](https://img.shields.io/crates/l/smix-screen?style=flat-square)](#license)

Accessibility tree types for iOS-Simulator automation — `A11yNode`,
`Rect`, `Bounds`, `Role`, `ElementSummary`, `ScreenDescription` — with
inlined visibility primitives that approach single-cycle latency on
M-series machines.

Zero-allocation arithmetic, `#[inline]` on the hot path, criterion bench
medians available below for reference. The visibility math is the same
semantics maestro and Apple's XCUITest enforce (any non-empty frame
intersection with the viewport, zero-width / zero-height bounds
short-circuit to false).

## Quickstart

```rust
use smix_screen::{
    A11yNode, Rect, Role, collect_visible_summaries, is_visible_enough,
    summarize_node, visible_area,
};

let node = A11yNode {
    raw_type: "button".into(),
    element_type_raw: 9,
    role: Some(Role::Button),
    identifier: Some("btn-login".into()),
    label: Some("Log in".into()),
    title: None, placeholder_value: None, value: None, text: None,
    bounds: Rect { x: 50.0, y: 100.0, w: 200.0, h: 40.0 },
    enabled: true, selected: false, has_focus: false, visible: true, hittable: None,
    children: vec![],
};
let viewport = Rect { x: 0.0, y: 0.0, w: 390.0, h: 844.0 };
let tree = A11yNode {
    raw_type: "application".into(),
    bounds: viewport,
    children: vec![node.clone()],
    ..node.clone()
};

assert!(is_visible_enough(&node, &tree));
assert_eq!(visible_area(&node, &tree), 200.0 * 40.0);

let summary = summarize_node(&node);
assert_eq!(summary.name.as_deref(), Some("Log in"));

let summaries = collect_visible_summaries(&tree, 100);
assert_eq!(summaries.len(), 2); // root + button
```

## Performance (criterion bench medians on M-series macOS)

| Operation | TS V8 baseline | `smix-screen` Rust | Speedup |
|---|---|---|---|
| `is_visible_enough` (in-view) | 21.2 ns | **0.996 ns** | **21×** |
| `is_visible_enough` (zero-bounds early reject) | 20.5 ns | **0.449 ns** | **46×** |
| `is_visible_enough` (unknown-root conservative pass) | 20.6 ns | **0.584 ns** | **35×** |
| `is_visible_enough` (offscreen) | 20.7 ns | **0.805 ns** | **26×** |
| `visible_area` (intersection arithmetic) | 20.9 ns | **1.112 ns** | **19×** |
| `dfs_collect` 100-node composite | 788 ns | **233 ns** | **3.4×** |

Numbers reproducible via `cargo bench --bench visibility -p smix-screen`.

## When to reach for this

| Use case | Pick |
|---|---|
| Need typed `A11yNode` matching Apple's XCUIElement attribute set | **smix-screen** |
| Need to filter "visible enough" candidates on a hot path | **smix-screen** |
| Want the same visibility semantics as maestro / XCUITest `.isHittable` | **smix-screen** |
| Need a full DOM/HTML accessibility tree (browser context) | use a browser-focused crate; this is iOS-shaped |
| Need physical device a11y tree | this is sim-shaped (camelCase wire fields match `xcrun simctl io` outputs) |

## Wire compatibility

JSON serialization uses `serde(rename_all = "camelCase")` matching the
iOS-Simulator runner conventions: `rawType`, `placeholderValue`,
`hasFocus`. `children: Vec<A11yNode>` is recursive; terminal nodes
may omit the field (serde `default = "..."`).

The `Role` enum mirrors `XCUIElement.ElementType` literals (29 variants)
with a stable camelCase wire name accessor (`Role::Button.as_str()` →
`"button"`).

## Scope

- ✅ Pure types + visibility primitives
- ✅ `#[inline]` + zero-allocation hot path
- ✅ camelCase JSON wire matches the iOS sim runner format
- ✅ `ScreenDescription` + `collect_visible_summaries` for DFS-pre-order
  visible-element projection
- ❌ No tree fetch (use [`smix-runner-client`](https://crates.io/crates/smix-runner-client) for the HTTP IPC)
- ❌ No selector parsing (use [`smix-selector`](https://crates.io/crates/smix-selector))
- ❌ No resolver / spatial filter logic (use [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver))

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.
