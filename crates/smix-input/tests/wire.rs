//! Wire round-trip for KeyName + SwipeDirection.

use smix_input::{KeyName, SwipeDirection};

#[test]
fn swipe_direction_camel_case_wire() {
    for (d, expected) in [
        (SwipeDirection::Up, "\"up\""),
        (SwipeDirection::Down, "\"down\""),
        (SwipeDirection::Left, "\"left\""),
        (SwipeDirection::Right, "\"right\""),
    ] {
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, expected);
        let parsed: SwipeDirection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, d);
    }
}

#[test]
fn key_name_camel_case_wire() {
    let cases = [
        (KeyName::Return, "\"return\""),
        (KeyName::Delete, "\"delete\""),
        (KeyName::Tab, "\"tab\""),
        (KeyName::Space, "\"space\""),
        (KeyName::Escape, "\"escape\""),
        (KeyName::ArrowUp, "\"arrowUp\""),
        (KeyName::ArrowDown, "\"arrowDown\""),
        (KeyName::ArrowLeft, "\"arrowLeft\""),
        (KeyName::ArrowRight, "\"arrowRight\""),
    ];
    for (k, expected) in cases {
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, expected);
        let parsed: KeyName = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, k);
    }
}

#[test]
fn display_impl_matches_as_str() {
    assert_eq!(format!("{}", KeyName::ArrowUp), "arrowUp");
    assert_eq!(format!("{}", SwipeDirection::Left), "left");
}
