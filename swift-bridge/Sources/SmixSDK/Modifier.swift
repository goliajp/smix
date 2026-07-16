// Modifier — a single selector modifier expressed as a value.
//
// Rust smix-selector `Modifiers` is a struct with optional fields
// (near / below / above / leftOf / rightOf / inside / ancestor +
// nth / first / last) that gets `#[serde(flatten)]`-ed into the
// Selector base JSON. The Swift mirror of that struct is `Modifiers`
// (Modifiers.swift), reached through Selector's fluent chaining
// (`.id("btn").below(.text("hi")).nth(0)`).

import Foundation

/// A single selector modifier. `Selector` itself carries the
/// all-optional [`Modifiers`] struct rather than this enum.
///
/// Wire JSON form: flattened into the Selector body so
/// `{"id":"btn","nth":0,"below":{"text":"hi"}}` round-trips.
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
