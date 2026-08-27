//! The tree says which of the two readers answered.
//!
//! smix has one perception primitive and, since v10, two things that can
//! satisfy it: the accessibility tree it has always read, and — when the app
//! opted in — the UI toolkit's own semantics tree, read in the app's process.
//!
//! The two do not see the same screen. With a Compose dialog open the
//! accessibility path sees none of the app at all while the probe sees all of
//! it, so an answer that does not say where it came from is an answer a
//! caller cannot weigh. Silently giving the worse one is the failure mode
//! §9 #1 exists to forbid: a capability that is not obtainable is a loud
//! error, never a quiet downgrade.

use smix_runner_client::TreeSource;

#[test]
fn a_source_names_the_reader_not_the_platform() {
    // Not `Android` / `IOS`: the question is which tree answered, and both
    // platforms can answer from either. Naming the platform here would make
    // the field describe the device instead of the evidence.
    assert_eq!(TreeSource::Accessibility.as_str(), "a11y");
    assert_eq!(TreeSource::Semantics.as_str(), "semantics");
}

#[test]
fn the_accessibility_source_says_what_it_cannot_see() {
    // A caller holding an `a11y` answer needs to know what that costs them,
    // at the point they hold it — not in a guide they would have to already
    // suspect they needed.
    let note = TreeSource::Accessibility.limitation();
    let note = note.expect("the accessibility reader has known blind spots and must say so");
    assert!(
        note.contains("dialog"),
        "the note does not mention dialogs, which is the blind spot that \
         costs a whole screen: {note}"
    );
}

#[test]
fn the_semantics_source_claims_no_blind_spot() {
    assert!(
        TreeSource::Semantics.limitation().is_none(),
        "the semantics reader reads the tree the other is projected from; \
         if it grows a known blind spot, this test is the place to say so"
    );
}

// --- what a caller is handed ------------------------------------------

use smix_runner_client::PerceivedTree;
use smix_screen::A11yNode;

fn empty_root() -> A11yNode {
    serde_json::from_str(r#"{"rawType":"Window","bounds":{"x":0.0,"y":0.0,"w":1.0,"h":1.0},
           "enabled":true,"selected":false,"hasFocus":false,"visible":true,
           "children":[]}"#).expect("a root parses")
}

#[test]
fn an_accessibility_answer_arrives_with_its_caveat() {
    let t = PerceivedTree { source: TreeSource::Accessibility, root: empty_root() };
    let c = t.caveat().expect("an accessibility answer must carry its caveat");
    assert!(c.contains("a11y"), "the caveat does not say which reader: {c}");
    assert!(
        c.contains("smix-probe"),
        "the caveat says what is wrong and not what to do about it: {c}"
    );
}

#[test]
fn a_semantics_answer_has_nothing_to_apologise_for() {
    let t = PerceivedTree { source: TreeSource::Semantics, root: empty_root() };
    assert!(t.caveat().is_none());
}

#[test]
fn the_source_survives_a_round_trip_on_the_wire() {
    // The runner sends it and the client reads it back. A spelling that
    // only matched by accident would make every answer look like a11y,
    // which is the direction that fails quietly.
    let json = serde_json::to_string(&TreeSource::Semantics).expect("serialises");
    assert_eq!(json, "\"semantics\"");
    let back: TreeSource = serde_json::from_str("\"a11y\"").expect("deserialises");
    assert_eq!(back, TreeSource::Accessibility);
}
