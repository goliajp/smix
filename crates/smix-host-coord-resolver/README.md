# smix-host-coord-resolver

[![Crates.io](https://img.shields.io/crates/v/smix-host-coord-resolver?style=flat-square&logo=rust)](https://crates.io/crates/smix-host-coord-resolver)
[![docs.rs](https://img.shields.io/docsrs/smix-host-coord-resolver?style=flat-square&logo=docs.rs)](https://docs.rs/smix-host-coord-resolver)
[![License](https://img.shields.io/crates/l/smix-host-coord-resolver?style=flat-square)](#license)

Pure pipeline from `(A11yNode tree, Selector)` to a normalized `(nx, ny)`
tap coordinate. No I/O, no async, no injector awareness — the
host-side resolve step decoupled from whatever HID / Apple-native /
runner-side mechanism the caller wants to drive afterward.

This crate exists because the "fetch tree → resolve selector → compute
centroid → normalize into app frame" half of a typical iOS automation
`tap` call is generic across smix, maestro, Appium, and anyone building
their own automation framework. Extracting it gives you a stone-sized
dependency without dragging in a specific injector or runner-client.

## Quickstart

```rust
use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_coord};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};

# fn mk(label: &str, bounds: Rect) -> A11yNode {
#     A11yNode {
#         raw_type: "other".into(), element_type_raw: 1, role: None, identifier: None,
#         label: Some(label.into()), title: None, placeholder_value: None,
#         value: None, text: None, bounds, enabled: true, selected: false,
#         has_focus: false, visible: true, children: vec![],
#     }
# }
let mut root = mk("root", Rect { x: 0.0, y: 0.0, w: 390.0, h: 844.0 });
root.children.push(mk("Login", Rect { x: 50.0, y: 100.0, w: 200.0, h: 40.0 }));

let sel = Selector::Text {
    text: Pattern::text("Login"),
    modifiers: Modifiers::default(),
};

let (nx, ny) = resolve_to_norm_coord(&root, &sel).unwrap();
// Hand (nx, ny) to whatever injector you have:
// my_runner.tap_at_norm_coord(nx, ny).await?;
# assert!((nx - 150.0 / 390.0).abs() < 1e-9);
# assert!((ny - 120.0 / 844.0).abs() < 1e-9);
```

## Error variants

`HostResolveError` is fine-grained so callers can map each variant to
their own failure type:

| Variant | Meaning |
|---|---|
| `NotFound` | selector matched nothing in the tree (after visibility filter) |
| `EmptyMatchedFrame` | matched node has zero-or-negative `w` or `h` |
| `UnknownAppFrame` | `tree.bounds` has zero-or-negative `w` or `h` |
| `CentroidOutOfFrame { nx, ny }` | computed normalized coord is outside `(0, 1)` |

## When to reach for this

| Use case | Pick |
|---|---|
| Building an iOS automation library and want host-side resolve as a stone | **smix-host-coord-resolver** |
| Already use [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver) and want the centroid step too | **smix-host-coord-resolver** |
| Need a full driver with `tap` / `fill` / `scroll` semantics + retry | use [`smix-driver`](https://crates.io/crates/smix-driver) (consumes this stone) |
| Doing physical-coord tap (not normalized) | normalize by your viewport first, then use this |

## Scope

- ✅ Pure function `fn resolve_to_norm_coord(tree, selector) -> Result<(f64, f64), _>`
- ✅ Reuses [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver) for the tree walk
- ✅ Centroid + normalize math in `(0, 1)` app-frame coordinates
- ❌ No async, no I/O, no injector
- ❌ No retry / implicit wait — that's [`smix-driver`](https://crates.io/crates/smix-driver)'s job

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.
