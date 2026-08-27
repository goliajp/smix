//! A control the accessibility tree cannot name, resolved from the tree it
//! is projected from.
//!
//! With a Compose dialog open, its content IS in the accessibility tree —
//! eleven nodes under the activity's content frame — and every one of them
//! is an anonymous `View`. `testTagsAsResourceId` is a property of the
//! subtree it is written on, and a dialog composes into its own, so the
//! opt-in never reaches it. `smix find id:compose_dialog_confirm` answers
//! `exists=false` about a button plainly on screen.
//!
//! The probe reads the semantics tree, where the tag is simply there. What
//! the resolver needs is not a second kind of node — it is the same facts
//! from a second source, so the probe's answer is converted into the shape
//! everything downstream already speaks.
//!
//! Payloads are recorded off a device. Both are asserted to still be the
//! shape the wire emits, because a recorded fixture goes stale in silence
//! and this release has already had a gate blind for a whole checkpoint
//! that way.

use smix_selector::Selector;
use smix_selector_resolver::{probe_tree_to_a11y, resolve_selector};

fn id_selector(id: &str) -> Selector {
    Selector::Id { id: id.to_string(), modifiers: Default::default() }
}

const SEMANTICS: &str = include_str!("fixtures/dialog-semantics.json");
const A11Y: &str = include_str!("fixtures/dialog-a11y.json");

fn a11y_tree() -> smix_screen::A11yNode {
    let v: serde_json::Value = serde_json::from_str(A11Y).expect("the a11y fixture parses");
    assert!(
        v.get("source").is_some(),
        "the recorded a11y payload has no `source` — it is older than the \
         envelope `smix tree --json` emits, and re-recording is the fix"
    );
    serde_json::from_value(v["root"].clone()).expect("the root is an A11yNode")
}

#[test]
fn the_accessibility_tree_cannot_name_the_dialog_button() {
    // The starting condition, asserted rather than assumed: without this
    // the test below could pass on a payload where nothing was ever wrong.
    let tree = a11y_tree();
    let sel = id_selector("compose_dialog_confirm");
    assert!(
        resolve_selector(&tree, &sel).is_none(),
        "the accessibility payload already carries the dialog's tag, so this \
         fixture is not exhibiting the defect it was recorded for"
    );
}

#[test]
fn the_semantics_tree_can() {
    let tree = probe_tree_to_a11y(SEMANTICS).expect("the probe payload converts");
    let sel = id_selector("compose_dialog_confirm");
    let hit = resolve_selector(&tree, &sel);
    assert!(
        hit.is_some(),
        "`compose_dialog_confirm` is in the semantics payload and did not \
         resolve after conversion"
    );
}

#[test]
fn the_converted_tree_keeps_screen_coordinates() {
    // Bounds are what a tap is placed from. The probe reports screen
    // coordinates precisely because a dialog composes into its own window
    // and `boundsInWindow` put it at y=0; losing that in conversion would
    // put every dialog tap at the top of the screen.
    let tree = probe_tree_to_a11y(SEMANTICS).expect("converts");
    let sel = id_selector("compose_dialog_confirm");
    let n = resolve_selector(&tree, &sel).expect("resolves");
    assert!(
        n.bounds.y > 100.0,
        "the dialog button came back at y={}, which is where a window-relative \
         coordinate would put it",
        n.bounds.y
    );
}

#[test]
fn every_tag_the_probe_saw_survives_the_conversion() {
    // A precise count, not "more than none": a conversion that dropped half
    // a screen would still resolve something.
    let tree = probe_tree_to_a11y(SEMANTICS).expect("converts");
    let mut ids = vec![];
    fn walk(n: &smix_screen::A11yNode, out: &mut Vec<String>) {
        if let Some(i) = &n.identifier {
            out.push(i.clone());
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    walk(&tree, &mut ids);
    assert_eq!(
        ids.len(),
        17,
        "the recorded payload has 17 tags (1 in the dialog's root, 16 in the \
         screen's); the conversion produced {ids:?}"
    );
}

#[test]
fn a_masked_field_converts_to_what_was_typed_not_what_is_shown() {
    // The recorded dialog payload has empty fields, so it cannot pin this —
    // a mutation swapping the two priorities passed against it. Compose
    // applies a password field's visual transformation BEFORE semantics
    // sees it, so `editableText` reads back as bullets and `inputText` is
    // what the flow typed. A predicate comparing a fill with `editableText`
    // asks a question the field cannot answer, and that verdict took a
    // consumer's whole Android suite red at 6.4.0.
    let payload = r#"[{"id":1,"bounds":[0,0,100,40],"focused":false,
        "enabled":true,"actions":[],"children":[
          {"id":2,"testTag":"secret","editableText":"•••••••••",
           "inputText":"s3cret-99","bounds":[0,0,100,40],"focused":false,
           "enabled":true,"actions":[],"children":[]}]}]"#;
    let tree = probe_tree_to_a11y(payload).expect("converts");
    let n = resolve_selector(&tree, &id_selector("secret")).expect("resolves");
    assert_eq!(
        n.value.as_deref(),
        Some("s3cret-99"),
        "the converted node carries what is shown rather than what was typed"
    );
}

#[test]
fn a_payload_that_is_not_the_probes_shape_is_refused_rather_than_emptied() {
    // "Nothing on screen" and "I could not read this" want different
    // answers. One value for both is how a caller learns to distrust the
    // field — and an empty tree resolves nothing, which reads exactly like
    // a screen that has nothing on it.
    assert!(probe_tree_to_a11y("not json at all").is_none());
    assert!(probe_tree_to_a11y(r#"{"source":"a11y","root":{}}"#).is_none(),
        "the a11y envelope was accepted as a probe payload");
}
