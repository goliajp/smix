//! Two base selector keys mean one thing, and the same thing everywhere.
//!
//! `{ id: X, text: Y }` reads as "the element with this id whose text
//! is that" — a conjunction, which is what maestro's selectors are and
//! what six blocks in smix's own guides are written as.
//!
//! It was not. Both parse sites test `text` before `id` and return the
//! first match, so `{ id: X, text: Y }` becomes `Text { Y }` and the id
//! is dropped entirely — the selector matches any element reading Y.
//! `assertVisible: { id: "counter", text: "1" }` in the cookbook has
//! been asserting "some element reads 1" since it was written.
//!
//! The verb-agreement test below passes today and stays as a
//! regression guard: the sites agree now, and a fix that changed one
//! without the other would be a new way to get here.
//!
//! Found the first time the corpus was pointed at an app that was not
//! Settings. The twenty flows before it each named exactly one key.

use smix_adapter_maestro::{Step, parse_flow_yaml};
use smix_selector::Selector;

fn selector_of(yaml: &str) -> Selector {
    let flow = parse_flow_yaml(yaml).expect("the flow parses");
    match flow.steps.first().expect("one step") {
        Step::TapOn { selector, .. } | Step::AssertVisible { selector, .. } => selector.clone(),
        other => panic!("unexpected step: {other:?}"),
    }
}

#[test]
fn the_same_two_keys_mean_the_same_thing_at_every_verb() {
    let tap = selector_of("appId: com.example\n---\n- tapOn: { id: \"a\", text: \"b\" }\n");
    let assert =
        selector_of("appId: com.example\n---\n- assertVisible: { id: \"a\", text: \"b\" }\n");
    assert_eq!(
        tap, assert,
        "one selector map, two verbs, two meanings — which is the defect"
    );
}

#[test]
fn the_conjunction_keeps_both_halves() {
    // Whichever base form wins, the other key has to survive as a
    // constraint. A selector that quietly drops half of what was
    // written matches things the author excluded.
    let s = selector_of("appId: com.example\n---\n- tapOn: { id: \"a\", text: \"b\" }\n");
    let Selector::Id { id, modifiers } = &s else {
        panic!("id is the more specific half and should be the base: {s:?}");
    };
    assert_eq!(id, "a");
    assert_eq!(
        modifiers.and.len(),
        1,
        "the text half was dropped rather than kept as a constraint: {s:?}"
    );
}

#[test]
fn one_base_key_is_untouched() {
    let s = selector_of("appId: com.example\n---\n- tapOn: { id: \"a\" }\n");
    let Selector::Id { modifiers, .. } = &s else {
        panic!("expected Id: {s:?}");
    };
    assert!(
        modifiers.and.is_empty(),
        "a single-key selector grew a constraint out of nowhere: {s:?}"
    );
}

#[test]
fn role_with_a_name_is_one_form_spelled_with_two_keys() {
    // `role` + `name` is a documented single form, not a conjunction;
    // folding `name` into the constraint list would change what every
    // existing role selector matches.
    parse_flow_yaml("appId: com.example\n---\n- tapOn: { role: button, name: \"OK\" }\n")
        .expect("role + name is one form");
}
