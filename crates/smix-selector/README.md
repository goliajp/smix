# smix-selector

[![Crates.io](https://img.shields.io/crates/v/smix-selector?style=flat-square&logo=rust)](https://crates.io/crates/smix-selector)
[![docs.rs](https://img.shields.io/docsrs/smix-selector?style=flat-square&logo=docs.rs)](https://docs.rs/smix-selector)
[![License](https://img.shields.io/crates/l/smix-selector?style=flat-square)](#license)

Structured `Selector` enum + `match_text` for iOS-Simulator a11y trees.
Six mutually-exclusive base forms (text / id / label / role+name /
focused / anchor-only), recursive spatial modifiers
(near/below/above/leftOf/rightOf/inside), and a `Pattern` / `CompiledPattern`
split that gives JS-RegExp-like ergonomics with zero per-call compile
cost on the resolver hot path.

## Quickstart

```rust
use smix_screen::{A11yNode, Rect, Role};
use smix_selector::{
    AnchorBox, IndexModifiers, Modifiers, Pattern, Selector,
    describe_selector, match_text, match_text_compiled,
};

// Plain text base
let s1: Selector = Selector::Text {
    text: Pattern::text("Login"),
    modifiers: Modifiers::default(),
};

// Spatial modifier — "Submit" below "Email"
let s2: Selector = Selector::Text {
    text: Pattern::text("Submit"),
    modifiers: Modifiers {
        below: Some(Box::new(Selector::Text {
            text: Pattern::text("Email"),
            modifiers: Modifiers::default(),
        })),
        ..Default::default()
    },
};

// Regex (auto case-insensitive via /(?i)/ prefix — maestro parity)
let s3 = Selector::Text {
    text: Pattern::regex("^log"),  // matches "Login", "Log out", etc.
    modifiers: Modifiers::default(),
};

// Anchor-only — pick the n-th element below a reference
let s4 = Selector::Anchor {
    anchor: AnchorBox {
        below: Some(Box::new(Selector::Text {
            text: Pattern::text("Header"),
            modifiers: Modifiers::default(),
        })),
        ..Default::default()
    },
    index: IndexModifiers { nth: Some(2), ..Default::default() },
};

// AI-readable rendering for failure prompts and logs
let s = describe_selector(&s2);
assert!(s.contains("below="));

// Hot-path matching: compile once, match many times
let pattern = Pattern::text("Login");
let compiled = pattern.compile().expect("text compile is infallible");
let node = A11yNode { /* ... */
#    raw_type: "button".into(), element_type_raw: 9, role: Some(Role::Button),
#    identifier: None, label: Some("Login".into()),
#    title: None, placeholder_value: None, value: None, text: None,
#    bounds: Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 },
#    enabled: true, selected: false, has_focus: false, visible: true, hittable: None,
#    children: vec![],
};
assert!(match_text_compiled(&node, &compiled));
```

## Pattern / CompiledPattern split

The wire form of a text or regex selector pattern is `Pattern`
(serde-friendly: a plain string for text, `{regex, flags}` object for
regex). Matching uses `CompiledPattern` (caches the compiled
`regex::Regex` DFA). Callers transition via `Pattern::compile()`.

This split mirrors how V8's `RegExp` lazily compiles + reuses the DFA
once per object. In Rust we make the compile step explicit so the
resolver layer can amortize compile cost across hundreds of candidate
nodes in a tree walk:

| Operation | TS V8 baseline | Rust hot path | Speedup |
|---|---|---|---|
| `match_text` string field-1 hit | 36.8 ns | **4.89 ns** (`match_text_compiled`) | **7.5×** |
| `match_text` string 6-field scan miss | 41.4 ns | **3.53 ns** | **12×** |
| `match_text` regex hit (default `/i`) | 105.5 ns | **9.39 ns** | **11×** |
| `match_text` regex on 100-char label | 52.2 ns | **31.5 ns** | **1.7×** |
| `Pattern::compile` regex (one-time, amortized) | n/a | **15.65 µs** | — |

For one-shot SDK calls there is also `match_text(&node, &Pattern)` which
compiles on every call (15 µs); resolver-driven hot loops should always
prefer `match_text_compiled`.

## SDK convenience vs resolver hot path

`smix-selector` exposes two matcher entry points, and the choice between
them matters by ~10000× for regex patterns:

| API | When | Cost (regex) | Cost (plain text) |
|---|---|---|---|
| `match_text(node, &Pattern)` | One-shot ad-hoc calls; ergonomic for tests, REPLs, and tooling that matches a handful of nodes | ~15 µs / call (`regex::Regex::new` every time) | ~5 ns / call |
| `match_text_compiled(node, &CompiledPattern)` | Hot loops; the real path inside `smix-selector-resolver` (`HashMap<*const Pattern, CompiledPattern>` cache + DFS) and `smix-driver` | ~5–50 ns / call (compile cost paid once) | ~5 ns / call |

If you write your own resolver, walker, or any code that calls a matcher
in a loop, reach for `Pattern::compile()` once at the call site and feed
the resulting `CompiledPattern` to `match_text_compiled` per node — the
pattern documented in [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver)
`ResolverContext`. A regression guard
(`crates/smix-selector/tests/perf_contract.rs`) blocks bare hot-loop
`smix_selector::match_text` calls from landing in workspace `src/`.

## When to reach for this

| Use case | Pick |
|---|---|
| Need a structured query type for a11y trees / accessibility selectors | **smix-selector** |
| Want maestro-equivalent text matching (6-field OR + case-insensitive) | **smix-selector** |
| Want recursive spatial modifiers (e.g. "Submit below Email") | **smix-selector** |
| Need xpath / CSS selectors | use a generic query crate; this is iOS-shaped |
| Just need string matching, no tree | use `regex` directly |

## Scope

- ✅ 6 base forms × recursive spatial modifiers × index modifiers
- ✅ camelCase JSON wire (no discriminator field; presence of `text` /
  `id` / `label` / `role` / `focused` / `anchor` is the discriminator)
- ✅ `Pattern::compile()` → `CompiledPattern` (regex cache)
- ✅ `match_text` 6-field OR scan matching maestro parity
- ✅ `describe_selector` AI-readable rendering
- ❌ No tree walking (use [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver))
- ❌ No xpath / CSS

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.
