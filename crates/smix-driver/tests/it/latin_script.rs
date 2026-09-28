//! Which scripts the Android recogniser can read, and what it says when
//! it cannot.
//!
//! The Kotlin route reads `locales` and ignores it. That was harmless
//! while no surface let a caller send any; the moment MCP and the CLI
//! could, a Chinese needle would go to a Latin recogniser and come back
//! "no matching text" — a sentence about the screen when the truth is
//! about the recogniser. Invariant 9 #1 ③: name what this device cannot
//! do rather than degrading into silence.

use smix_driver::latin_script_only;

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

/// The scripts ML Kit's Latin package cannot read, named.
#[test]
fn a_non_latin_script_is_named() {
    for tag in [
        "zh-Hans",
        "zh",
        "zh-Hant-HK",
        "ja",
        "ko",
        "ru",
        "el",
        "ar",
        "th",
    ] {
        assert_eq!(
            latin_script_only(&v(&[tag])),
            Some(tag),
            "{tag} needs a script the Latin package does not have"
        );
    }
}

/// And the ones it can, so the refusal cannot be had by refusing
/// everything — which would take OCR off Android entirely.
#[test]
fn latin_scripts_pass() {
    for tag in ["en", "en-GB", "fr", "de", "es", "pt-BR", "vi", "tr", "pl"] {
        assert_eq!(latin_script_only(&v(&[tag])), None, "{tag} is Latin script");
    }
}

/// Naming none is what every caller did until now, and it still means
/// "let the recogniser decide".
#[test]
fn naming_no_locale_is_not_a_refusal() {
    assert_eq!(latin_script_only(&[]), None);
}

/// One unreadable script in a list is enough — the caller asked for a
/// reading that cannot happen, whatever else is beside it.
#[test]
fn one_unreadable_script_in_a_list_is_named() {
    assert_eq!(latin_script_only(&v(&["en", "ja"])), Some("ja"));
}

/// Case is the caller's, not the answer's.
#[test]
fn case_does_not_decide() {
    assert_eq!(latin_script_only(&v(&["ZH-HANS"])), Some("ZH-HANS"));
}
