//! `KeyName::from_name`: the one reader of a key's name.

use smix_input::{KeyName, KeyNameError};

/// maestro's `KeyCode` descriptions (maestro-client `KeyCode.kt`, main,
/// read 2026-09-24): 30 keys. The nine smix presses, then the 21 it
/// refuses by name.
const MAESTRO_PRESSED: [(&str, KeyName); 9] = [
    ("Enter", KeyName::Return),
    ("Backspace", KeyName::Delete),
    ("Back", KeyName::Back),
    ("Home", KeyName::Home),
    ("Lock", KeyName::Lock),
    ("Volume Up", KeyName::VolumeUp),
    ("Volume Down", KeyName::VolumeDown),
    ("Escape", KeyName::Escape),
    ("Tab", KeyName::Tab),
];

const MAESTRO_REFUSED: [&str; 21] = [
    "Remote Dpad Up",
    "Remote Dpad Down",
    "Remote Dpad Left",
    "Remote Dpad Right",
    "Remote Dpad Center",
    "Remote Media Play Pause",
    "Remote Media Stop",
    "Remote Media Next",
    "Remote Media Previous",
    "Remote Media Rewind",
    "Remote Media Fast Forward",
    "Power",
    "Remote System Navigation Up",
    "Remote System Navigation Down",
    "Remote Button A",
    "Remote Button B",
    "Remote Menu",
    "TV Input",
    "TV Input HDMI 1",
    "TV Input HDMI 2",
    "TV Input HDMI 3",
];

#[test]
fn every_maestro_key_smix_presses_reads_by_its_maestro_spelling() {
    for (name, key) in MAESTRO_PRESSED {
        assert_eq!(KeyName::from_name(name), Ok(key), "{name}");
        assert_eq!(
            KeyName::from_name(&name.to_uppercase()),
            Ok(key),
            "{name} upper-cased"
        );
    }
}

#[test]
fn every_other_maestro_key_is_refused_by_name_with_a_reason() {
    for name in MAESTRO_REFUSED {
        match KeyName::from_name(name) {
            Err(KeyNameError::NotPressedHere { name: n, why }) => {
                assert_eq!(n, name);
                assert!(!why.is_empty());
            }
            other => panic!("{name}: expected a refusal by name, got {other:?}"),
        }
    }
}

#[test]
fn power_points_to_lock() {
    match KeyName::from_name("power") {
        Err(KeyNameError::NotPressedHere { why, .. }) => {
            assert!(why.contains("`lock`"), "{why}")
        }
        other => panic!("expected a refusal pointing to lock, got {other:?}"),
    }
}

#[test]
fn a_tv_key_says_smix_drives_no_tv() {
    let err = KeyName::from_name("Remote Dpad Center").unwrap_err();
    assert!(err.to_string().contains("TV"), "{err}");
}

#[test]
fn every_key_reads_back_from_its_own_wire_name() {
    for key in KeyName::ALL {
        assert_eq!(KeyName::from_name(key.as_str()), Ok(key));
    }
}

/// `ALL` is every variant: a new one without an entry here is a compile
/// error in the match, and one left out of `ALL` fails the count.
#[test]
fn all_is_every_key_once() {
    fn index(k: KeyName) -> usize {
        match k {
            KeyName::Return => 0,
            KeyName::Delete => 1,
            KeyName::Tab => 2,
            KeyName::Space => 3,
            KeyName::Escape => 4,
            KeyName::ArrowUp => 5,
            KeyName::ArrowDown => 6,
            KeyName::ArrowLeft => 7,
            KeyName::ArrowRight => 8,
            KeyName::Home => 9,
            KeyName::Lock => 10,
            KeyName::VolumeUp => 11,
            KeyName::VolumeDown => 12,
            KeyName::Back => 13,
        }
    }
    let mut seen = [false; 14];
    for k in KeyName::ALL {
        assert!(!seen[index(k)], "{k} twice");
        seen[index(k)] = true;
    }
    assert!(seen.iter().all(|s| *s));
}

#[test]
fn spaces_underscores_hyphens_and_case_are_not_part_of_a_name() {
    for name in [
        "volume up",
        "VOLUME_UP",
        "volume-up",
        "volumeUp",
        "VolumeUp",
    ] {
        assert_eq!(KeyName::from_name(name), Ok(KeyName::VolumeUp), "{name}");
    }
    for name in ["arrowUp", "arrow_up", "ARROW UP", "up"] {
        assert_eq!(KeyName::from_name(name), Ok(KeyName::ArrowUp), "{name}");
    }
}

#[test]
fn the_shell_shorthands_still_read() {
    for (name, key) in [
        ("return", KeyName::Return),
        ("enter", KeyName::Return),
        ("delete", KeyName::Delete),
        ("backspace", KeyName::Delete),
        ("esc", KeyName::Escape),
        ("down", KeyName::ArrowDown),
        ("left", KeyName::ArrowLeft),
        ("right", KeyName::ArrowRight),
        ("space", KeyName::Space),
    ] {
        assert_eq!(KeyName::from_name(name), Ok(key), "{name}");
    }
}

#[test]
fn an_unknown_name_lists_the_keys() {
    let err = KeyName::from_name("banana").unwrap_err();
    assert_eq!(
        err,
        KeyNameError::Unknown {
            name: "banana".into()
        }
    );
    let msg = err.to_string();
    assert!(
        msg.contains("banana") && msg.contains("back") && msg.contains("volumeDown"),
        "{msg}"
    );
    assert!(KeyName::from_name("").is_err());
}
