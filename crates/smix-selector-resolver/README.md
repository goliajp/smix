# smix-selector-resolver

[![Crates.io](https://img.shields.io/crates/v/smix-selector-resolver?style=flat-square&logo=rust)](https://crates.io/crates/smix-selector-resolver)
[![docs.rs](https://img.shields.io/docsrs/smix-selector-resolver?style=flat-square&logo=docs.rs)](https://docs.rs/smix-selector-resolver)
[![License](https://img.shields.io/crates/l/smix-selector-resolver?style=flat-square)](#license)

End-to-end resolver for [`smix-selector`](https://crates.io/crates/smix-selector)
queries against an [`smix-screen::A11yNode`](https://crates.io/crates/smix-screen)
tree. Pipeline: DFS pre-order collect → visibility filter → spatial
filter (six recursive anchor keys, AND semantics) → index pick
(`first` / `last` / `nth`). One per-call compile cache (`ResolverContext`)
amortizes regex compilation across the entire candidate walk.

## Quickstart

```rust
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::{resolve_selector, resolve_selector_all};

let tree = A11yNode {
    raw_type: "application".into(), element_type_raw: 1,
    bounds: Rect { x: 0.0, y: 0.0, w: 390.0, h: 844.0 },
    children: vec![
        A11yNode { /* "Login" button */
#            raw_type: "button".into(), element_type_raw: 1, role: None, identifier: None,
#            label: Some("Login".into()),
#            title: None, placeholder_value: None, value: None, text: None,
            bounds: Rect { x: 50.0, y: 100.0, w: 200.0, h: 40.0 },
            enabled: true, selected: false, has_focus: false, visible: true, hittable: None,
            children: vec![],
        },
    ],
#    role: None, identifier: None, label: None, title: None,
#    placeholder_value: None, value: None, text: None,
#    enabled: true, selected: false, has_focus: false, visible: true, hittable: None,
};

let selector = Selector::Text {
    text: Pattern::text("Login"),
    modifiers: Modifiers::default(),
};

let one = resolve_selector(&tree, &selector);
assert!(one.is_some());

let all = resolve_selector_all(&tree, &selector);
assert_eq!(all.len(), 1);
```

## Performance

End-to-end `resolve_selector` measurements on M-series macOS:

| Operation | TS V8 baseline | Rust release | Speedup |
|---|---|---|---|
| text base, 100-node tree, label hit (DFS depth 2) | 3,652 ns | **1,186 ns** | **3.1×** |
| text base, 100-node tree, full DFS miss | 3,693 ns | **1,088 ns** | **3.4×** |
| text base + spatial below modifier, 4-node tree | 607 ns | **295 ns** | **2.1×** |
| id base, 100-node tree, miss | 3,234 ns | **198 ns** | **16×** |
| regex text base, 100-node tree, label hit | 11,114 ns | **10,957 ns** | **parity** (regex compile-dominant) |
| anchor-only + below, 4-node tree, nth=1 | 532 ns | **277 ns** | **1.9×** |
| text + nth=2, multi-match 5-X tree | 468 ns | **179 ns** | **2.6×** |

Numbers reproducible via
`cargo bench --bench resolver -p smix-selector-resolver`.

The regex case sits at parity (≈11 µs) because regex compile dominates;
consumer-level caches (e.g. caching the compiled selector across
multiple `wait_for` poll iterations) push that case to ~11× as well.

## Pipeline

1. **Collect**: DFS pre-order over the tree, gather every node where
   the base form matches (text / id / label / role+name / focused /
   anchor accept-all).
2. **Visibility filter** (from
   [`smix-screen`](https://crates.io/crates/smix-screen)): drop nodes
   with zero/negative bounds or fully-outside-viewport — unless no
   candidate is visible, in which case drop nothing (preserves miss
   reporting paths).
3. **Spatial filter**: for each of `near` / `below` / `above` /
   `leftOf` / `rightOf` / `inside` (top-level or inside `anchor.*`),
   recursively resolve the anchor sub-selector; filter candidates by
   centroid-axis or geometric containment. AND semantics. Anchor null →
   overall `None`.
4. **Index pick**: apply `first` / `last` / `nth`; later modifier
   overrides earlier (`nth` wins both).

## When to reach for this

| Use case | Pick |
|---|---|
| Have an `A11yNode` tree + structured `Selector` → need the matched node | **smix-selector-resolver** |
| Just need string match against a single node | [`smix-selector::match_text`](https://crates.io/crates/smix-selector) directly |
| Need xpath / CSS / jQuery-style selectors | use a generic query crate |

## Scope

- ✅ Pure function, no async, no I/O
- ✅ `ResolverContext` regex compile cache (1 compile per `resolve_selector` call, amortized over all candidates)
- ✅ Anchor null short-circuits to `None`
- ✅ 30 unit tests + 2 perf-gate budgets enforced in CI
- ❌ No tree fetching (use [`smix-runner-client`](https://crates.io/crates/smix-runner-client))
- ❌ No tap/coord injection (use [`smix-host-coord-resolver`](https://crates.io/crates/smix-host-coord-resolver) + an injector)

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.
