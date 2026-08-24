import Foundation

/// May a fill that names no field go ahead, or is there nothing here to
/// type into?
///
/// `inputText: "..."` — the scalar form — targets `_focused_`: type into
/// whatever holds focus. The runner skipped focus resolution for that
/// selector *whatever the dispatch mode said*:
///
/// ```swift
/// let resolveFocus = dispatch != .keyEvents
/// if resolveFocus && !selectorText.isEmpty && selectorText != "_focused_"
/// ```
///
/// so the documented default (`a11y` — "resolve focus via the a11y
/// tree") never applied to the one selector that is entirely about
/// focus, and `--force-key-events` was a no-op for it. The daemon send
/// then went ahead unconditionally and answered ok.
///
/// Measured 2026-08-25 on the iOS fixture with nothing focused: `ok`,
/// 18 characters, no warnings — while the tree read `value=None` and the
/// screenshot still showed the placeholder. Android answers
/// `ELEMENT_NOT_FOUND` to the same flow. One verb, one input, two
/// opposite answers, and the wrong one is the silent one.
///
/// What counts as evidence that the characters have somewhere to go:
///
/// - **something is focused** — the ordinary case; or
/// - **a keyboard is up** — the RN hidden-input case, where the a11y
///   tree cannot address the field so nothing reads as focused, but the
///   keyboard being on screen means something took it.
///
/// Neither, and there is nowhere for the text to land. Saying so is not
/// a new restriction: `--force-key-events` is documented as "skip a11y
/// focus resolution … for fields the a11y tree cannot address", and it
/// still does exactly that. This makes it the only way to get that,
/// rather than the silent default — the same move as replacing an
/// environment variable that recorded nothing with a claim that does.
public enum FillFocusPolicy {
  /// Which way the caller asked the text to be dispatched.
  public enum Dispatch: Equatable, Sendable {
    /// Resolve focus through the a11y tree first. The documented default.
    case a11y
    /// Skip focus resolution and send raw key events. `--force-key-events`.
    case keyEvents
    /// Try a11y, fall back to key events.
    case auto
  }

  public enum Decision: Equatable, Sendable {
    case proceed
    /// Nothing here can receive the text, and reporting success would be
    /// a claim with no evidence behind it.
    case refuse
  }

  /// The refusal, in the caller's terms. Names the two ways out rather
  /// than describing the runner's internals: a caller cannot act on
  /// "focus resolution was skipped", and can act on both of these.
  public static let refusalReason = """
    nothing has keyboard focus and no keyboard is up, so there is nothing \
    here to type into. Name the field — `inputText: { id: ..., text: ... }` \
    — or tap it first. If the field is one the accessibility tree cannot \
    address (a React Native hidden input is the usual case), pass \
    --force-key-events, which types into whatever holds focus without \
    asking.
    """

  /// Pure. Takes what the runner observed rather than observing anything,
  /// so the decision can be driven from a test with no device — which is
  /// the half of this that a device gate cannot check cheaply.
  public static func decide(
    dispatch: Dispatch,
    isFocusedSelector: Bool,
    somethingFocused: Bool,
    keyboardUp: Bool
  ) -> Decision {
    // A named field resolves and is tapped by the existing path; this
    // policy governs only the form that names nothing.
    guard isFocusedSelector else { return .proceed }
    // The mode that exists in order to skip this question.
    if dispatch == .keyEvents { return .proceed }
    return (somethingFocused || keyboardUp) ? .proceed : .refuse
  }
}
