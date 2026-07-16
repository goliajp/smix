// Modifiers struct — mirrors Rust smix-selector `Modifiers` +
// `IndexModifiers`, flattened into the Selector body via custom Codable.
//
// Wire JSON shape: fields are FLATTENED into the Selector base body.
// E.g. Rust:
//   {"id": "btn-foo", "below": {"id": "anchor"}, "nth": 2}
// not:
//   {"id": "btn-foo", "modifiers": {"below": {...}, "nth": 2}}
//
// Each field is Optional and `nil` when omitted (skipped on encode).
//
// Spatial: near / below / above / leftOf / rightOf / inside / ancestor
// Index:   first / last / nth
//
// Stackable: e.g. `{"id":"btn", "below":{"text":"hi"}, "nth": 0, "near":{"role":"button"}}`.

import Foundation

/// All-optional modifier set stacked onto a base [`Selector`].
///
/// Constructed empty via `.empty` then mutated by fluent chaining
/// methods on Selector. Spatial fields hold sub-selectors (anchor
/// to spatially compare against); index fields are scalar picks.
public struct Modifiers: Sendable, Equatable, Codable {
    // ---- spatial -----------------------------------------------------

    /// Candidate must be geometrically near this anchor (centroid-distance
    /// ≤ threshold pts). Threshold defaults to 100pt per Rust resolver.
    public var near: Selector?
    /// Candidate must be geometrically below this anchor (larger y center).
    public var below: Selector?
    /// Candidate must be geometrically above this anchor (smaller y center).
    public var above: Selector?
    /// Candidate must be to the left of this anchor (smaller x center).
    public var leftOf: Selector?
    /// Candidate must be to the right of this anchor (larger x center).
    public var rightOf: Selector?
    /// Candidate must be geometrically inside this anchor's bounds.
    public var inside: Selector?
    /// Ancestor sub-selector — candidate's a11y tree parent chain (recursive
    /// ancestors, excluding self) must contain at least one node matching
    /// this sub-selector. Structural filter, not spatial.
    public var ancestor: Selector?

    // ---- index -------------------------------------------------------

    /// Pick the nth match (0-indexed) from surviving candidates.
    public var nth: Int?
    /// Pick the first match (`true`). Overridden by `nth` if both set.
    public var first: Bool?
    /// Pick the last match (`true`). Overridden by `nth` if both set.
    public var last: Bool?

    public init(
        near: Selector? = nil,
        below: Selector? = nil,
        above: Selector? = nil,
        leftOf: Selector? = nil,
        rightOf: Selector? = nil,
        inside: Selector? = nil,
        ancestor: Selector? = nil,
        nth: Int? = nil,
        first: Bool? = nil,
        last: Bool? = nil
    ) {
        self.near = near
        self.below = below
        self.above = above
        self.leftOf = leftOf
        self.rightOf = rightOf
        self.inside = inside
        self.ancestor = ancestor
        self.nth = nth
        self.first = first
        self.last = last
    }

    /// Empty modifiers — no filtering or index pick applied.
    public static let empty = Modifiers()

    /// True when no modifier field is set — used by encoder to skip the
    /// shared modifier block entirely.
    public var isEmpty: Bool {
        near == nil && below == nil && above == nil
            && leftOf == nil && rightOf == nil && inside == nil
            && ancestor == nil && nth == nil && first == nil && last == nil
    }
}
