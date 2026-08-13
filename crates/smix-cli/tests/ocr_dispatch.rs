//! Every CLI command that accepts an OCR selector dispatches past the tree.
//!
//! `OcrText` never matches in the tree: the resolver returns false for it
//! by design, because OCR is a live look at the screen and not a
//! predicate over a dump. A command that takes one and calls `find` or
//! `tap` with it does not fail — it reports that text plainly on screen
//! is not there. Silence is the failure mode, which is why this is a
//! source-level assertion rather than a behavioural one: there is nothing
//! to observe when it goes wrong.

use std::fs;

const ACT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/act.rs");

/// Every command that reaches `parse_selector` either dispatches an OCR
/// needle or refuses one by name. Neither is optional; passing it to the
/// resolver is the third option and the wrong one.
#[test]
fn every_selector_command_answers_for_ocr() {
    let src = fs::read_to_string(ACT).expect("act.rs");
    let bodies: Vec<&str> = src.split("pub async fn cmd_").skip(1).collect();
    let takers: Vec<&&str> = bodies
        .iter()
        .filter(|b| b.contains("parse_selector(&selector_str)"))
        .collect();

    // A split that finds nothing agrees with any file at all.
    assert!(
        takers.len() >= 5,
        "only {} command(s) parse a selector — this test is reading air",
        takers.len()
    );

    for body in takers {
        let name = body.split('(').next().unwrap_or("?");
        assert!(
            body.contains("ocr_needle"),
            "cmd_{name} takes a selector and never asks whether it is an OCR \
             needle. It will hand one to the resolver, which answers 'not \
             found' for text that is on the screen"
        );
    }
}

/// And the shared reading is shared: the recognition level and the
/// default locale live in one place, so the CLI cannot drift from the SDK
/// by a string literal.
#[test]
fn the_ocr_constants_are_not_copied() {
    let src = fs::read_to_string(ACT).expect("act.rs");
    assert!(
        src.contains("OCR_RECOGNITION_LEVEL"),
        "the CLI names the shared constant"
    );
    assert!(
        !src.contains("\"accurate\""),
        "a second copy of the recognition level is a literal that will differ"
    );
}
