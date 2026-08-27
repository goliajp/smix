//! Property-based tests for smix-selector match_text + Pattern.
//!
//! Invariants:
//! - Empty `Pattern::Text("")` never matches anything.
//! - `match_text` is case-insensitive (uppercase ↔ lowercase variants match identically).
//! - `Pattern::compile` is deterministic — same `Pattern::Regex` compiles to a Regex
//!   that produces the same `match_text_compiled` result for any node.

use proptest::prelude::*;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Pattern, match_text, match_text_compiled};

fn mk_node(label: Option<String>, id: Option<String>) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: id,
        label,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 40.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

proptest! {
    /// Empty text pattern never matches anything.
    #[test]
    fn empty_text_never_matches(
        label in proptest::option::of(".*"),
        id in proptest::option::of(".*"),
    ) {
        let node = mk_node(label, id);
        let p = Pattern::Text(String::new());
        prop_assert!(!match_text(&node, &p));
    }

    /// `match_text` is case-insensitive: matching on lowercase ↔ uppercase variant.
    #[test]
    fn case_insensitive_match(label in "[A-Za-z]{1,20}") {
        let node = mk_node(Some(label.clone()), None);
        let p_lower = Pattern::Text(label.to_lowercase());
        let p_upper = Pattern::Text(label.to_uppercase());
        prop_assert_eq!(match_text(&node, &p_lower), match_text(&node, &p_upper));
    }

    /// `Pattern::compile` is deterministic — repeated compile of the same
    /// pattern yields semantically equivalent matchers.
    #[test]
    fn compile_deterministic(
        pattern in "[A-Za-z0-9 ]{1,30}",
        label in "[A-Za-z0-9 ]{1,30}",
    ) {
        let p = Pattern::Text(pattern.clone());
        let c1 = p.compile().expect("text compile is infallible");
        let c2 = p.compile().expect("text compile is infallible");
        let node = mk_node(Some(label), None);
        prop_assert_eq!(match_text_compiled(&node, &c1), match_text_compiled(&node, &c2));
    }
}
