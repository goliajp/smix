//! `ResolverContext` + `resolve_selector_compiled` /
//! `resolve_selector_all_compiled` API surface unit tests.
//!
//! Validates the four entry-point cases callers of the pub API must
//! handle:
//! 1. plain hit — non-regex `Pattern`, single tree match
//! 2. regex hit — regex `Pattern`, single tree match (cache prepass
//!    builds a `CompiledPattern` once, looked up via `ctx.pattern`)
//! 3. miss — selector compiles but matches no node in the tree
//! 4. invalid regex — `ResolverContext::new` returns `None` at compile
//!    time (matches `resolve_selector` silent-`None` semantic; caller
//!    can branch on `?` short-circuit)

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::{
    ResolverContext, resolve_selector_all_compiled, resolve_selector_compiled,
};

fn mk_leaf(label: &str) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 20.0,
            y: 50.0,
            w: 100.0,
            h: 25.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn mk_app(children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "application".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

#[test]
fn compiled_plain_hit() {
    let tree = mk_app(vec![mk_leaf("Target")]);
    let sel = Selector::Text {
        text: Pattern::text("Target"),
        modifiers: Modifiers::default(),
    };
    let ctx = ResolverContext::new(&sel).expect("plain pattern always compiles");
    let hit = resolve_selector_compiled(&tree, &sel, &ctx);
    assert!(hit.is_some(), "plain hit must resolve");
    assert_eq!(hit.unwrap().label.as_deref(), Some("Target"));
}

#[test]
fn compiled_regex_hit() {
    let tree = mk_app(vec![mk_leaf("Login")]);
    let sel = Selector::Text {
        text: Pattern::regex("^Log.*"),
        modifiers: Modifiers::default(),
    };
    let ctx = ResolverContext::new(&sel).expect("valid regex must compile");
    let hit = resolve_selector_compiled(&tree, &sel, &ctx);
    assert!(hit.is_some(), "regex hit must resolve");
    assert_eq!(hit.unwrap().label.as_deref(), Some("Login"));
}

#[test]
fn compiled_miss() {
    let tree = mk_app(vec![mk_leaf("Login")]);
    let sel = Selector::Text {
        text: Pattern::text("NotInTree"),
        modifiers: Modifiers::default(),
    };
    let ctx = ResolverContext::new(&sel).expect("plain pattern always compiles");
    let hit = resolve_selector_compiled(&tree, &sel, &ctx);
    assert!(hit.is_none(), "miss must return None");
    let all = resolve_selector_all_compiled(&tree, &sel, &ctx);
    assert!(all.is_empty(), "_all miss must return empty vec");
}

#[test]
fn compiled_invalid_regex_returns_none_from_new() {
    let sel = Selector::Text {
        text: Pattern::regex("[unbalanced"),
        modifiers: Modifiers::default(),
    };
    let ctx = ResolverContext::new(&sel);
    assert!(
        ctx.is_none(),
        "invalid regex must short-circuit at ResolverContext::new"
    );
}
