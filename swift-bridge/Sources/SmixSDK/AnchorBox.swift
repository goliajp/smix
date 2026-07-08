// v7.2 c3 — AnchorBox + IndexModifiers mirror Rust smix-selector.
//
// `AnchorBox` carries only spatial anchor sub-selectors (no index, no
// base form). Used by `Selector.anchor(AnchorBox, ...)` where the
// candidate set is defined entirely by spatial composition.
//
// `IndexModifiers` is the index-only subset (nth/first/last) used by
// Anchor (which doesn't have a Modifiers struct).

import Foundation

/// Spatial-only anchor box (no index, no base). All fields optional;
/// at least one is required for the resolver to produce candidates.
public struct AnchorBox: Sendable, Equatable, Codable {
    public var near: Selector?
    public var below: Selector?
    public var above: Selector?
    public var leftOf: Selector?
    public var rightOf: Selector?
    public var inside: Selector?
    public var ancestor: Selector?

    public init(
        near: Selector? = nil,
        below: Selector? = nil,
        above: Selector? = nil,
        leftOf: Selector? = nil,
        rightOf: Selector? = nil,
        inside: Selector? = nil,
        ancestor: Selector? = nil
    ) {
        self.near = near
        self.below = below
        self.above = above
        self.leftOf = leftOf
        self.rightOf = rightOf
        self.inside = inside
        self.ancestor = ancestor
    }
}

/// Index-only modifier subset used by `Selector.anchor(...)`. Same
/// semantics as the `nth/first/last` fields on [`Modifiers`].
public struct IndexModifiers: Sendable, Equatable {
    public var nth: Int?
    public var first: Bool?
    public var last: Bool?

    public init(nth: Int? = nil, first: Bool? = nil, last: Bool? = nil) {
        self.nth = nth
        self.first = first
        self.last = last
    }

    public static let empty = IndexModifiers()
}
