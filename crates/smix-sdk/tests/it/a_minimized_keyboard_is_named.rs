//! A wait for the keyboard that times out on a simulator with
//! `AutomaticMinimizationEnabled` on carries that fact and the way to turn
//! it off — as a fact about the device, not as the cause of the timeout:
//! on iOS 27 a keyboard still appears with it on. It says nothing when the
//! setting is off, unread, or the wait was for something else.

use smix_sdk::{Role, Selector, keyboard_minimized_note};

fn keyboard() -> Selector {
    Selector::Role {
        role: Role::Keyboard,
        name: None,
        modifiers: Default::default(),
    }
}

#[test]
fn a_keyboard_wait_on_a_minimizing_simulator_names_the_setting() {
    let note = keyboard_minimized_note(&keyboard(), Some(true), "UDID-1")
        .expect("the setting is on and the wait was for the keyboard");
    assert!(note.contains("AutomaticMinimizationEnabled"), "{note}");
    assert!(
        note.contains(
            "defaults delete com.apple.keyboard.preferences AutomaticMinimizationEnabled"
        ),
        "names the way back: {note}"
    );
    assert!(
        note.contains("UDID-1"),
        "addressed to this simulator: {note}"
    );
}

#[test]
fn the_setting_is_reported_as_a_fact_not_as_the_cause() {
    let note = keyboard_minimized_note(&keyboard(), Some(true), "UDID-1").expect("setting on");
    assert!(
        note.contains("not as the cause"),
        "says what it is not: {note}"
    );
    for claim in [
        "cannot succeed",
        "shows no keyboard",
        "shows no software keyboard",
        "keeps its software keyboard minimized",
    ] {
        assert!(
            !note.contains(claim),
            "asserts a cause measurement does not support ({claim:?}): {note}"
        );
    }
}

#[test]
fn off_or_unread_says_nothing() {
    assert_eq!(keyboard_minimized_note(&keyboard(), Some(false), "U"), None);
    assert_eq!(keyboard_minimized_note(&keyboard(), None, "U"), None);
}

#[test]
fn another_selector_says_nothing() {
    let button = Selector::Role {
        role: Role::Button,
        name: None,
        modifiers: Default::default(),
    };
    assert_eq!(keyboard_minimized_note(&button, Some(true), "U"), None);
    let id = Selector::Id {
        id: "keyboard".into(),
        modifiers: Default::default(),
    };
    assert_eq!(keyboard_minimized_note(&id, Some(true), "U"), None);
}
