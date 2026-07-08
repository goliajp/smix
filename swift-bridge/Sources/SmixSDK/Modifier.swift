// v7.1 c1 — Modifier API placeholder.
//
// Rust smix-selector `Modifiers` is a struct with optional fields
// (near / below / above / leftOf / rightOf / inside / ancestor +
// nth / first / last) that gets `#[serde(flatten)]`-ed into the
// Selector base JSON. v7.2 c3 will land the full Modifiers struct
// + Selector fluent chaining (`.id("btn").below(.text("hi")).nth(0)`).
//
// v7.1 c1 MVP keeps Modifier as a placeholder for shape tests but
// does not wire it into Selector yet. Test author writes plain base
// selectors only this cycle.

import Foundation

/// Modifier — placeholder enum for v7.1 c1 shape tests. v7.2 c3 will
/// replace with the full Modifiers struct + fluent chaining methods.
///
/// Wire JSON form (when wired in v7.2): flattened into Selector body
/// so `{"id":"btn","nth":0,"below":{"text":"hi"}}` round-trips.
public enum Modifier: Sendable, Equatable {
    /// Pick first match from surviving candidates.
    case first
    /// Pick last match.
    case last
    /// Pick nth (0-indexed).
    case nth(Int)
    /// Anchor sub-selector — candidate must be geometrically above it.
    case above(Selector)
    /// Anchor sub-selector — candidate must be geometrically below it.
    case below(Selector)
    /// Anchor sub-selector — candidate must be to its left.
    case leftOf(Selector)
    /// Anchor sub-selector — candidate must be to its right.
    case rightOf(Selector)
    /// Anchor sub-selector — candidate must be geometrically near it
    /// (centroid-distance ≤ threshold pts).
    case near(Selector, thresholdPts: Double = 100.0)
    /// Anchor sub-selector — candidate must be inside its bounds.
    case inside(Selector)
}
