//! Every verb-by-form cell says dispatched or refused, and neither
//! answer is allowed to be prose that points somewhere else.
//!
//! The table is only worth having if it agrees with the code. A cell
//! claiming a dispatch the runtime does not perform is a table agreeing
//! with itself — which is what the comment in `matches_base` was for
//! four releases: it asserted that the adapter dispatched a form, three
//! verbs did, and nothing compared the sentence with the call sites.

use smix_adapter_maestro::selector_support::{Slot, Support, UnreadableForm, support};

#[test]
fn every_refusal_says_what_this_verb_does() {
    for slot in Slot::ALL {
        for form in UnreadableForm::ALL {
            if let Support::Refused(why) = support(slot, form) {
                assert!(
                    why.len() > 20,
                    "{slot:?} × {form:?} refuses with {why:?}, which is too short to \
                     tell an author what to write instead"
                );
                // The sentence that cost four releases. A refusal that
                // points at another layer tells the reader the problem
                // is elsewhere, and the call site that has to change
                // never hears about it.
                for evasion in ["adapter dispatches", "handled elsewhere", "caller forgot"] {
                    assert!(
                        !why.contains(evasion),
                        "{slot:?} × {form:?} refuses by pointing away: {why:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_locale_map_is_read_wherever_a_selector_is_resolved() {
    // It is a rewrite, not a capability. 6.7.1 closed the last nine
    // call sites; a cell going back to Refused here would mean one of
    // them was dropped.
    //
    // `AnnotationAnchor` is the one slot that resolves no selector at
    // all — the yaml verb draws at (0, 0) whatever you name — so it
    // reads no form, a locale map included. Written as an exception
    // with its reason rather than by loosening the rule to "most
    // slots", which would stop noticing the next one to drop out.
    for slot in Slot::ALL {
        if slot == Slot::AnnotationAnchor {
            assert!(
                matches!(
                    support(slot, UnreadableForm::LocalizedText),
                    Support::Refused(_)
                ),
                "AnnotationAnchor resolves no selector, so it cannot read a locale map",
            );
            continue;
        }
        assert_eq!(
            support(slot, UnreadableForm::LocalizedText),
            Support::Dispatched,
            "{slot:?} stopped reading a locale map",
        );
    }
}

#[test]
fn an_absence_check_refuses_ocr_in_both_slots() {
    // Not a style choice: OCR missing text is not evidence the text is
    // absent, and `assertNotVisible: { ocrText: 'smix fixture' }` passed
    // against a screen showing those words.
    for slot in [Slot::AssertNotVisibleTarget, Slot::WaitNotVisibleTarget] {
        assert!(
            matches!(support(slot, UnreadableForm::OcrText), Support::Refused(_)),
            "{slot:?} must not report absence from an OCR miss",
        );
    }
}

#[test]
fn the_two_halves_of_extended_wait_can_disagree() {
    // One verb, two slots, opposite answers — which is why the table is
    // keyed by slot and not by verb.
    assert_eq!(
        support(Slot::WaitVisibleTarget, UnreadableForm::OcrText),
        Support::Dispatched
    );
    assert!(matches!(
        support(Slot::WaitNotVisibleTarget, UnreadableForm::OcrText),
        Support::Refused(_)
    ));
}

/// Print the table so a scanner can compare it with the code.
///
/// The cells live in `match` arms that group slots and forms with `|`,
/// so a scanner parsing the source would answer about pairs nobody
/// wrote down. This asks the compiled table instead.
#[test]
fn print_the_table() {
    for slot in Slot::ALL {
        for form in UnreadableForm::ALL {
            let verdict = match support(slot, form) {
                Support::Dispatched => "DISPATCHED",
                Support::Refused(_) => "REFUSED",
            };
            println!("CELL {slot:?}:{form:?} {verdict}");
        }
    }
}
