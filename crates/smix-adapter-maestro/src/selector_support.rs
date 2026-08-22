//! Which verb reads which selector form, decided cell by cell.
//!
//! Three selector spellings describe something the accessibility tree
//! cannot show: `ocrText:` is pixels, `localizedText:` is a locale map
//! that has to be picked from first, and `anchorRelative:` resolves to a
//! shifted coordinate rather than to a node. Each is read above the
//! resolver by the verbs that read it at all — and matched by nothing by
//! the verbs that do not.
//!
//! v6.7 spent six checkpoints on the consequences of that being written
//! in a comment: `assertVisible` with a `fallback:` chain matched
//! nothing on both platforms, `assertNotVisible` with `ocrText:` passed
//! against a screen showing the words, and a locale map was rewritten by
//! three verbs out of twelve. Every one of them was a rule that only
//! some call sites followed, and nothing could see the gap because a
//! verb that does not read a form fails exactly like a verb whose target
//! is absent.
//!
//! So the cells are here, and both matches are exhaustive. A new `Step`
//! does not compile until it says which selector slots it has; a new
//! slot does not compile until it says what it does with each form.
//! There is no third answer: a slot either reads the form or refuses it
//! by name.

use crate::Step;
use crate::{AnnotationPos, AnnotationSpec};
use smix_selector::Selector;

/// A selector spelling the tree resolver cannot evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableForm {
    /// `ocrText:` — pixels, read by Vision (iOS) / ML Kit (Android).
    OcrText,
    /// `localizedText:` — a locale map, rewritten to `text:` first.
    LocalizedText,
    /// `anchorRelative:` — an anchor plus a shift, resolving to a point.
    AnchorRelative,
}

impl UnreadableForm {
    /// Every form, for the tests and the reconciliation scan.
    pub const ALL: [UnreadableForm; 3] = [
        UnreadableForm::OcrText,
        UnreadableForm::LocalizedText,
        UnreadableForm::AnchorRelative,
    ];

    /// The spelling as an author writes it.
    #[must_use]
    pub fn as_written(self) -> &'static str {
        match self {
            UnreadableForm::OcrText => "ocrText",
            UnreadableForm::LocalizedText => "localizedText",
            UnreadableForm::AnchorRelative => "anchorRelative",
        }
    }
}

/// One place a verb takes a selector.
///
/// A verb can have more than one, and they need not agree:
/// `extendedWaitUntil` reads OCR on its `visible:` slot and refuses it
/// on `notVisible:`, because "OCR did not find it" is evidence of
/// presence in one direction and not of absence in the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// `tapOn:` — the element to tap.
    TapOnTarget,
    /// `repeatTap:` — the element to tap repeatedly.
    RepeatTapTarget,
    /// `doubleTapOn:` — the element to tap twice.
    DoubleTapTarget,
    /// `longPressOn:` — the element to hold.
    LongPressTarget,
    /// `inputText: { id, text }` — the field to type into.
    FillTarget,
    /// `copyTextFrom:` — the element to read.
    CopyTextSource,
    /// `assertVisible:` — the element that must be there.
    AssertVisibleTarget,
    /// `assertNotVisible:` — the element that must not be.
    AssertNotVisibleTarget,
    /// `extendedWaitUntil: { visible: }`.
    WaitVisibleTarget,
    /// `extendedWaitUntil: { notVisible: }`.
    WaitNotVisibleTarget,
    /// `scrollUntilVisible:` — the element to scroll to.
    ScrollTarget,
    /// `runFlow: { when: { visible / notVisible } }` — the gate.
    RunFlowGate,
    /// `takeScreenshot: { annotations: [{ at: <selector> }] }`.
    AnnotationAnchor,
}

/// What a slot does with a form it cannot read from the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// The verb reads it, above the resolver.
    Dispatched,
    /// The verb cannot, and says so. The reason is shown to the author,
    /// so it says what this verb does about the form — never that some
    /// other layer handles it. "The adapter dispatches it" is the
    /// sentence that let `Fallback` match nothing for its whole
    /// existence, and it points away from the code that has to change.
    Refused(&'static str),
}

/// The selector slots this step carries. Exhaustive over `Step`: a new
/// variant does not compile until it answers.
#[must_use]
pub fn slots(step: &Step) -> &'static [Slot] {
    match step {
        Step::TapOn { .. } => &[Slot::TapOnTarget],
        Step::RepeatTap { .. } => &[Slot::RepeatTapTarget],
        Step::DoubleTapOn { .. } => &[Slot::DoubleTapTarget],
        Step::LongPressOn { .. } => &[Slot::LongPressTarget],
        Step::InputTextInto { .. } => &[Slot::FillTarget],
        Step::CopyTextFrom { .. } => &[Slot::CopyTextSource],
        Step::AssertVisible { .. } => &[Slot::AssertVisibleTarget],
        Step::AssertNotVisible { .. } => &[Slot::AssertNotVisibleTarget],
        Step::ExtendedWaitUntil { .. } => &[Slot::WaitVisibleTarget, Slot::WaitNotVisibleTarget],
        Step::ScrollUntilVisible { .. } => &[Slot::ScrollTarget],
        Step::RunFlowInline { .. } | Step::RunFlowConditional { .. } => &[Slot::RunFlowGate],
        Step::TakeScreenshot { .. } => &[Slot::AnnotationAnchor],
        Step::TapAtPoint { .. }
        | Step::WebViewEval { .. }
        | Step::WaitForAnimationToEnd { .. }
        | Step::InputText { .. }
        | Step::PressKey { .. }
        | Step::Back
        | Step::RunFlow { .. }
        | Step::EraseText { .. }
        | Step::Swipe { .. }
        | Step::LaunchApp { .. }
        | Step::ClearAppData { .. }
        | Step::ResetAppData { .. }
        | Step::ClearUserDefaults { .. }
        | Step::OpenLink { .. }
        | Step::StopApp
        | Step::Scroll
        | Step::HideKeyboard
        | Step::KillApp { .. }
        | Step::ClearState { .. }
        | Step::ClearKeychain
        | Step::SetClipboard { .. }
        | Step::PasteText { .. }
        | Step::AssertTrue { .. }
        | Step::Repeat { .. }
        | Step::Retry { .. }
        | Step::RunScript { .. }
        | Step::EvalScript { .. }
        | Step::SetLocation { .. }
        | Step::Travel { .. }
        | Step::SetPermissions { .. }
        | Step::AddMedia { .. }
        | Step::SetOrientation { .. }
        | Step::StartRecording { .. }
        | Step::StopRecording
        | Step::AssertScreenshot { .. }
        | Step::AssertCondition { .. }
        | Step::ExtractWithAI { .. }
        | Step::ExpectSignal { .. }
        | Step::ExpectSignals { .. }
        | Step::ExpectLogClean
        | Step::Fixture { .. } => &[],
    }
}

impl Slot {
    /// Every slot, for walking. The compiler holds the other side —
    /// `support` matches exhaustively, so a slot cannot exist without an
    /// answer for each form — and `every-cell-is-declared.py` compares
    /// this list against the `Slot::` names `slots()` actually hands
    /// out, because a list written by hand beside a list derived from
    /// the code is two truths waiting to disagree.
    pub const ALL: [Slot; 13] = [
        Slot::TapOnTarget,
        Slot::RepeatTapTarget,
        Slot::DoubleTapTarget,
        Slot::LongPressTarget,
        Slot::FillTarget,
        Slot::CopyTextSource,
        Slot::AssertVisibleTarget,
        Slot::AssertNotVisibleTarget,
        Slot::WaitVisibleTarget,
        Slot::WaitNotVisibleTarget,
        Slot::ScrollTarget,
        Slot::RunFlowGate,
        Slot::AnnotationAnchor,
    ];
}

/// What this slot does with this form. Exhaustive over `Slot`: a new
/// slot does not compile until every form has an answer.
///
/// The `Dispatched` cells are checked against the code by
/// `every_cell_is_a_decision` — a cell that claims a dispatch the
/// runtime does not have would be a table agreeing with itself.
#[must_use]
pub fn support(slot: Slot, form: UnreadableForm) -> Support {
    use Slot as S;
    use UnreadableForm as F;
    match (slot, form) {
        // The yaml verb does not resolve a selector for an annotation
        // position at all — every form becomes (0, 0) with a warning —
        // so none of the three is read here, a locale map included.
        // Listed before the general locale rule because that rule is
        // true of every slot that resolves a selector, and this one
        // does not.
        (S::AnnotationAnchor, _) => Support::Refused(
            "an annotation position is not resolved from a selector: the yaml verb \
             draws at (0, 0) whatever you name. Write `at: { x, y }` or \
             `at: { nx, ny }`",
        ),

        // A locale map is a rewrite, not a capability: every slot that
        // resolves a selector does it, since 6.7.1.
        (_, F::LocalizedText) => Support::Dispatched,

        // Tapping is where a coordinate is enough. OCR gives a box and
        // an anchor plus a shift gives a point, and a tap is the one
        // thing you can do with a point.
        (S::TapOnTarget, F::OcrText | F::AnchorRelative) => Support::Dispatched,

        (S::RepeatTapTarget, F::OcrText) => Support::Refused(
            "repeatTap resolves once and taps the same element N times, and a \
             text found by OCR has a box rather than an element to hold on to. \
             Use `tapOn` with `ocrText:` inside a `repeat:`, which re-reads the \
             screen each time",
        ),
        (S::RepeatTapTarget, F::AnchorRelative) => Support::Refused(
            "repeatTap holds one resolved element across its taps, and an anchor \
             plus a shift is a point recomputed from the screen. Use `tapOn` with \
             `anchorRelative:` inside a `repeat:`",
        ),

        (S::DoubleTapTarget | S::LongPressTarget, F::OcrText | F::AnchorRelative) => {
            Support::Dispatched
        }

        (S::FillTarget, F::OcrText | F::AnchorRelative) => Support::Dispatched,

        (S::CopyTextSource, F::OcrText) => Support::Refused(
            "copyTextFrom reads the element's own text, and OCR returns what the \
             pixels look like rather than what the element holds — a masked field \
             would yield bullets and a truncated label its ellipsis. Name the \
             element by id, text, label or role",
        ),
        (S::CopyTextSource, F::AnchorRelative) => Support::Refused(
            "copyTextFrom reads an element, and an anchor plus a shift is a point; \
             there is no text at a coordinate. Name the element itself",
        ),

        (S::AssertVisibleTarget | S::WaitVisibleTarget | S::ScrollTarget, F::OcrText) => {
            Support::Dispatched
        }
        (S::AssertVisibleTarget | S::WaitVisibleTarget | S::ScrollTarget, F::AnchorRelative) => {
            Support::Refused(
                "an anchor plus a shift names a place, not an element, so there is \
                 nothing there to be visible. Assert on the anchor itself, or tap \
                 the point and check what the tap reached",
            )
        }

        (S::AssertNotVisibleTarget | S::WaitNotVisibleTarget, F::OcrText) => Support::Refused(
            "OCR not finding text is not evidence that it is absent: recognition \
             misses low contrast, small type and partial occlusion, and reporting \
             those as absence would be a pass with nothing behind it. Name the \
             element by id, text, label or role",
        ),
        (S::AssertNotVisibleTarget | S::WaitNotVisibleTarget, F::AnchorRelative) => {
            Support::Refused(
                "an anchor plus a shift names a place, not an element, so there is \
                 nothing there to be absent",
            )
        }

        (S::RunFlowGate, F::OcrText) => Support::Dispatched,
        (S::RunFlowGate, F::AnchorRelative) => Support::Refused(
            "a gate asks whether an element is on screen, and an anchor plus a \
             shift is a place. Gate on the anchor itself",
        ),
    }
}

/// Each selector this step carries, with the slot it sits in.
///
/// Exhaustive over `Step` for the same reason `slots` is: a variant
/// that carries a selector and says nothing here would be checked by
/// nothing.
#[must_use]
pub fn slot_selectors(step: &Step) -> Vec<(Slot, &Selector)> {
    match step {
        Step::TapOn { selector, .. } => vec![(Slot::TapOnTarget, selector)],
        Step::RepeatTap { selector, .. } => vec![(Slot::RepeatTapTarget, selector)],
        Step::DoubleTapOn { selector, .. } => vec![(Slot::DoubleTapTarget, selector)],
        Step::LongPressOn { selector, .. } => vec![(Slot::LongPressTarget, selector)],
        Step::InputTextInto { selector, .. } => vec![(Slot::FillTarget, selector)],
        Step::CopyTextFrom { selector, .. } => vec![(Slot::CopyTextSource, selector)],
        Step::AssertVisible { selector, .. } => vec![(Slot::AssertVisibleTarget, selector)],
        Step::AssertNotVisible { selector, .. } => {
            vec![(Slot::AssertNotVisibleTarget, selector)]
        }
        Step::ExtendedWaitUntil {
            selector,
            expect_visible,
            ..
        } => vec![(
            if *expect_visible {
                Slot::WaitVisibleTarget
            } else {
                Slot::WaitNotVisibleTarget
            },
            selector,
        )],
        Step::ScrollUntilVisible { selector, .. } => vec![(Slot::ScrollTarget, selector)],
        Step::RunFlowInline {
            when_visible,
            when_not_visible,
            ..
        }
        | Step::RunFlowConditional {
            when_visible,
            when_not_visible,
            ..
        } => when_visible
            .iter()
            .chain(when_not_visible.iter())
            .map(|s| (Slot::RunFlowGate, s))
            .collect(),
        Step::TakeScreenshot { annotations, .. } => annotations
            .iter()
            .flat_map(annotation_positions)
            .filter_map(|pos| match pos {
                AnnotationPos::Selector(sel) => Some((Slot::AnnotationAnchor, sel)),
                AnnotationPos::Pixel { .. } | AnnotationPos::Normalized { .. } => None,
            })
            .collect(),
        Step::TapAtPoint { .. }
        | Step::WebViewEval { .. }
        | Step::WaitForAnimationToEnd { .. }
        | Step::InputText { .. }
        | Step::PressKey { .. }
        | Step::Back
        | Step::RunFlow { .. }
        | Step::EraseText { .. }
        | Step::Swipe { .. }
        | Step::LaunchApp { .. }
        | Step::ClearAppData { .. }
        | Step::ResetAppData { .. }
        | Step::ClearUserDefaults { .. }
        | Step::OpenLink { .. }
        | Step::StopApp
        | Step::Scroll
        | Step::HideKeyboard
        | Step::KillApp { .. }
        | Step::ClearState { .. }
        | Step::ClearKeychain
        | Step::SetClipboard { .. }
        | Step::PasteText { .. }
        | Step::AssertTrue { .. }
        | Step::Repeat { .. }
        | Step::Retry { .. }
        | Step::RunScript { .. }
        | Step::EvalScript { .. }
        | Step::SetLocation { .. }
        | Step::Travel { .. }
        | Step::SetPermissions { .. }
        | Step::AddMedia { .. }
        | Step::SetOrientation { .. }
        | Step::StartRecording { .. }
        | Step::StopRecording
        | Step::AssertScreenshot { .. }
        | Step::AssertCondition { .. }
        | Step::ExtractWithAI { .. }
        | Step::ExpectSignal { .. }
        | Step::ExpectSignals { .. }
        | Step::ExpectLogClean
        | Step::Fixture { .. } => vec![],
    }
}

/// The form this selector is, when it is one the tree cannot read and
/// it stands alone.
///
/// Deliberately not walking a `fallback:` chain. A chain with a
/// readable layer that matches is a working flow, and refusing it
/// because a later layer names something this verb cannot read would
/// break flows that never needed that layer. When the whole chain
/// misses, the failure names the layer that went unread instead.
#[must_use]
pub fn standalone_unreadable(selector: &Selector) -> Option<UnreadableForm> {
    match selector {
        Selector::OcrText { .. } => Some(UnreadableForm::OcrText),
        Selector::LocalizedText { .. } => Some(UnreadableForm::LocalizedText),
        Selector::AnchorRelative { .. } => Some(UnreadableForm::AnchorRelative),
        _ => None,
    }
}

/// Every position an annotation names. Exhaustive over
/// `AnnotationSpec`, so a new annotation shape has to say where it
/// points before it compiles.
fn annotation_positions(spec: &AnnotationSpec) -> Vec<&AnnotationPos> {
    match spec {
        AnnotationSpec::Circle { at, .. }
        | AnnotationSpec::Text { at, .. }
        | AnnotationSpec::Box { at, .. } => vec![at],
        AnnotationSpec::Line { from, to, .. } | AnnotationSpec::Arrow { from, to, .. } => {
            vec![from, to]
        }
    }
}
