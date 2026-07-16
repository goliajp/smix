// Selector — 7 base cases + Modifiers flatten + Pattern + fluent chaining.
//
// Mirror Rust smix-selector `#[serde(untagged)] enum Selector` with full
// `Modifiers` flatten on Text/Id/Label/Role/LocalizedText bases. Focused
// has no modifiers (focus is a singleton runtime property); Anchor uses
// AnchorBox (spatial only, no flat) + IndexModifiers.
//
// Fluent chaining API:
//   Selector.id("btn-login")
//     .below(.text("Sign In"))
//     .nth(0)
//
// Each fluent method returns a new Selector value with Modifiers
// updated (immutable copy semantics).

import Foundation

/// A11y selector — identifies a node in the tree. Wire JSON form
/// matches Rust smix-selector `#[serde(untagged)]`: discriminator is
/// which key is present (`text` / `id` / `label` / `role` / `focused`
/// / `anchor` / `localizedText`), NOT a `{"kind":"..."}` tag.
///
/// Each base case (except `.focused`) carries a [`Modifiers`] struct
/// flattened into its JSON body via custom Codable.
public indirect enum Selector: Sendable, Equatable {
    /// `{"text": "hello", ...modifiers}` — text content pattern match.
    case text(Pattern, modifiers: Modifiers = .empty)
    /// `{"id": "btn-xyz", ...modifiers}` — strict equal a11y identifier.
    case id(String, modifiers: Modifiers = .empty)
    /// `{"label": "Settings", ...modifiers}` — strict equal a11y label.
    case label(String, modifiers: Modifiers = .empty)
    /// `{"role": "button", "name"?: pattern, ...modifiers}` — semantic
    /// role + optional accessible-name pattern.
    case role(Role, name: Pattern? = nil, modifiers: Modifiers = .empty)
    /// `{"focused": true}` — pick the currently focused element. No
    /// modifiers (focus is a singleton runtime property).
    case focused
    /// `{"anchor": {...spatial}, ...indexModifiers}` — anchor-only base
    /// form (no base shape; pure spatial composition). Uses AnchorBox
    /// (spatial fields only, no index) + IndexModifiers (nth/first/last).
    case anchor(AnchorBox, index: IndexModifiers = .empty)
    /// `{"localizedText": {"en":"Submit","ja":"送信"}, ...modifiers}` —
    /// per-locale text table.
    case localizedText([String: String], modifiers: Modifiers = .empty)
}

// MARK: - Fluent chaining

extension Selector {
    /// Set the `below` spatial modifier — candidate must be below `anchor`.
    public func below(_ anchor: Selector) -> Selector {
        withModifiers { $0.below = anchor }
    }
    /// Set the `above` spatial modifier.
    public func above(_ anchor: Selector) -> Selector {
        withModifiers { $0.above = anchor }
    }
    /// Set the `leftOf` spatial modifier.
    public func leftOf(_ anchor: Selector) -> Selector {
        withModifiers { $0.leftOf = anchor }
    }
    /// Set the `rightOf` spatial modifier.
    public func rightOf(_ anchor: Selector) -> Selector {
        withModifiers { $0.rightOf = anchor }
    }
    /// Set the `near` spatial modifier (default threshold = 100pt).
    public func near(_ anchor: Selector) -> Selector {
        withModifiers { $0.near = anchor }
    }
    /// Set the `inside` spatial modifier — candidate must be geometrically
    /// inside `anchor`'s bounds.
    public func inside(_ anchor: Selector) -> Selector {
        withModifiers { $0.inside = anchor }
    }
    /// Set the `ancestor` structural modifier — candidate must descend
    /// from a node matching `anchor`.
    public func ancestor(_ anchor: Selector) -> Selector {
        withModifiers { $0.ancestor = anchor }
    }
    /// Pick the nth match from surviving candidates (0-indexed).
    public func nth(_ index: Int) -> Selector {
        withModifiers { $0.nth = index }
    }
    /// Pick the first match.
    public func first() -> Selector {
        withModifiers { $0.first = true }
    }
    /// Pick the last match.
    public func last() -> Selector {
        withModifiers { $0.last = true }
    }

    /// Helper — produce a new selector value with `Modifiers` mutated
    /// via the given closure. Anchor / focused cases throw at runtime
    /// — they don't carry a Modifiers field.
    private func withModifiers(_ mutate: (inout Modifiers) -> Void) -> Selector {
        switch self {
        case let .text(p, mods):
            var m = mods; mutate(&m); return .text(p, modifiers: m)
        case let .id(s, mods):
            var m = mods; mutate(&m); return .id(s, modifiers: m)
        case let .label(s, mods):
            var m = mods; mutate(&m); return .label(s, modifiers: m)
        case let .role(r, n, mods):
            var m = mods; mutate(&m); return .role(r, name: n, modifiers: m)
        case .focused:
            // Focused has no modifiers — chain is a no-op. We return self
            // rather than throw to preserve fluent ergonomics; the JSON
            // encoder will simply emit `{"focused": true}`.
            return self
        case let .anchor(box, idx):
            // Anchor uses IndexModifiers, not Modifiers. Reuse closure on
            // a temporary Modifiers; copy the relevant nth/first/last back.
            var tmp = Modifiers.empty
            mutate(&tmp)
            var newIdx = idx
            if let n = tmp.nth { newIdx.nth = n }
            if let f = tmp.first { newIdx.first = f }
            if let l = tmp.last { newIdx.last = l }
            // Spatial fields ignored on anchor (already inside AnchorBox).
            return .anchor(box, index: newIdx)
        case let .localizedText(map, mods):
            var m = mods; mutate(&m); return .localizedText(map, modifiers: m)
        }
    }
}

// MARK: - Codable (custom, mirrors Rust untagged + flatten)

extension Selector: Codable {
    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: AnyKey.self)

        switch self {
        case let .text(pattern, modifiers):
            try container.encode(pattern, forKey: .text)
            try encodeModifiers(modifiers, into: &container)
        case let .id(value, modifiers):
            try container.encode(value, forKey: .id)
            try encodeModifiers(modifiers, into: &container)
        case let .label(value, modifiers):
            try container.encode(value, forKey: .label)
            try encodeModifiers(modifiers, into: &container)
        case let .role(role, name, modifiers):
            try container.encode(role, forKey: .role)
            if let name {
                try container.encode(name, forKey: .name)
            }
            try encodeModifiers(modifiers, into: &container)
        case .focused:
            try container.encode(true, forKey: .focused)
        case let .anchor(box, index):
            try container.encode(box, forKey: .anchor)
            try encodeIndex(index, into: &container)
        case let .localizedText(map, modifiers):
            try container.encode(map, forKey: .localizedText)
            try encodeModifiers(modifiers, into: &container)
        }
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: AnyKey.self)

        // Focused: explicit boolean field. Highest-priority discriminator.
        if let focused = try? container.decode(Bool.self, forKey: .focused), focused {
            self = .focused
            return
        }

        // Anchor base: nested AnchorBox object.
        if let box = try? container.decode(AnchorBox.self, forKey: .anchor) {
            let idx = try decodeIndex(from: container)
            self = .anchor(box, index: idx)
            return
        }

        // Text / id / label / role / localizedText discriminate by key presence.
        if let id = try? container.decode(String.self, forKey: .id) {
            let mods = try decodeModifiers(from: container)
            self = .id(id, modifiers: mods)
            return
        }
        if let pat = try? container.decode(Pattern.self, forKey: .text) {
            let mods = try decodeModifiers(from: container)
            self = .text(pat, modifiers: mods)
            return
        }
        if let lab = try? container.decode(String.self, forKey: .label) {
            let mods = try decodeModifiers(from: container)
            self = .label(lab, modifiers: mods)
            return
        }
        if let role = try? container.decode(Role.self, forKey: .role) {
            let name = try? container.decode(Pattern.self, forKey: .name)
            let mods = try decodeModifiers(from: container)
            self = .role(role, name: name, modifiers: mods)
            return
        }
        if let lt = try? container.decode([String: String].self, forKey: .localizedText) {
            let mods = try decodeModifiers(from: container)
            self = .localizedText(lt, modifiers: mods)
            return
        }
        throw DecodingError.dataCorrupted(
            .init(
                codingPath: decoder.codingPath,
                debugDescription: "Selector JSON missing recognized discriminator key (id / text / label / role / focused / anchor / localizedText)"
            )
        )
    }
}

// MARK: - Modifier flatten helpers

private func encodeModifiers(
    _ m: Modifiers,
    into container: inout KeyedEncodingContainer<AnyKey>
) throws {
    if let v = m.near { try container.encode(v, forKey: .near) }
    if let v = m.below { try container.encode(v, forKey: .below) }
    if let v = m.above { try container.encode(v, forKey: .above) }
    if let v = m.leftOf { try container.encode(v, forKey: .leftOf) }
    if let v = m.rightOf { try container.encode(v, forKey: .rightOf) }
    if let v = m.inside { try container.encode(v, forKey: .inside) }
    if let v = m.ancestor { try container.encode(v, forKey: .ancestor) }
    if let v = m.nth { try container.encode(v, forKey: .nth) }
    if let v = m.first { try container.encode(v, forKey: .first) }
    if let v = m.last { try container.encode(v, forKey: .last) }
}

private func decodeModifiers(
    from container: KeyedDecodingContainer<AnyKey>
) throws -> Modifiers {
    Modifiers(
        near: try? container.decode(Selector.self, forKey: .near),
        below: try? container.decode(Selector.self, forKey: .below),
        above: try? container.decode(Selector.self, forKey: .above),
        leftOf: try? container.decode(Selector.self, forKey: .leftOf),
        rightOf: try? container.decode(Selector.self, forKey: .rightOf),
        inside: try? container.decode(Selector.self, forKey: .inside),
        ancestor: try? container.decode(Selector.self, forKey: .ancestor),
        nth: try? container.decode(Int.self, forKey: .nth),
        first: try? container.decode(Bool.self, forKey: .first),
        last: try? container.decode(Bool.self, forKey: .last)
    )
}

private func encodeIndex(
    _ idx: IndexModifiers,
    into container: inout KeyedEncodingContainer<AnyKey>
) throws {
    if let v = idx.nth { try container.encode(v, forKey: .nth) }
    if let v = idx.first { try container.encode(v, forKey: .first) }
    if let v = idx.last { try container.encode(v, forKey: .last) }
}

private func decodeIndex(
    from container: KeyedDecodingContainer<AnyKey>
) throws -> IndexModifiers {
    IndexModifiers(
        nth: try? container.decode(Int.self, forKey: .nth),
        first: try? container.decode(Bool.self, forKey: .first),
        last: try? container.decode(Bool.self, forKey: .last)
    )
}

// MARK: - AnyKey + named CodingKey enum

/// CodingKey usable as both an enum discriminator (`.id`, `.text` etc.)
/// and a string fallback. Lets us reuse a single container type for the
/// flat-untagged + flatten pattern across all 7 Selector variants.
private struct AnyKey: CodingKey {
    var stringValue: String
    var intValue: Int?

    init(stringValue: String) {
        self.stringValue = stringValue
        self.intValue = nil
    }
    init?(intValue: Int) { return nil }

    static let id = AnyKey(stringValue: "id")
    static let text = AnyKey(stringValue: "text")
    static let label = AnyKey(stringValue: "label")
    static let role = AnyKey(stringValue: "role")
    static let name = AnyKey(stringValue: "name")
    static let focused = AnyKey(stringValue: "focused")
    static let anchor = AnyKey(stringValue: "anchor")
    static let localizedText = AnyKey(stringValue: "localizedText")
    // modifiers
    static let near = AnyKey(stringValue: "near")
    static let below = AnyKey(stringValue: "below")
    static let above = AnyKey(stringValue: "above")
    static let leftOf = AnyKey(stringValue: "leftOf")
    static let rightOf = AnyKey(stringValue: "rightOf")
    static let inside = AnyKey(stringValue: "inside")
    static let ancestor = AnyKey(stringValue: "ancestor")
    static let nth = AnyKey(stringValue: "nth")
    static let first = AnyKey(stringValue: "first")
    static let last = AnyKey(stringValue: "last")
}
