// v7.1 c2 — A11yNode + Rect mirror smix-screen wire form, with
// DFS lookup + flatten helpers for SDK callers.

import Foundation

/// A single a11y tree node. Mirror Rust smix-screen `A11yNode`
/// (wire form via `#[serde(rename_all = "camelCase")]`).
public struct A11yNode: Codable, Sendable, Equatable {
    /// Raw Apple `XCUIElement.ElementType` name (e.g. `"any"`,
    /// `"other"`, `"button"`).
    public let rawType: String
    /// Curated semantic role; `nil` when the raw type doesn't map.
    public let role: Role?
    /// Accessibility identifier (Apple `accessibilityIdentifier`).
    public let identifier: String?
    /// Accessibility label (Apple `accessibilityLabel`).
    public let label: String?
    /// Element title (Apple `title`).
    public let title: String?
    /// Placeholder text shown when the field is empty.
    public let placeholderValue: String?
    /// Element value (Apple `value`).
    public let value: String?
    /// Visible text content (Apple `text`).
    public let text: String?
    /// Geometric bounds in logical points.
    public let bounds: Rect
    /// Whether the element is currently interactable.
    public let enabled: Bool
    /// Whether the element is currently selected.
    public let selected: Bool
    /// Whether the element currently has keyboard focus.
    public let hasFocus: Bool
    /// Whether Apple's accessibility runtime reports this element as visible.
    public let visible: Bool
    /// Child nodes in DFS pre-order.
    public let children: [A11yNode]

    public init(
        rawType: String,
        role: Role? = nil,
        identifier: String? = nil,
        label: String? = nil,
        title: String? = nil,
        placeholderValue: String? = nil,
        value: String? = nil,
        text: String? = nil,
        bounds: Rect,
        enabled: Bool = false,
        selected: Bool = false,
        hasFocus: Bool = false,
        visible: Bool = false,
        children: [A11yNode] = []
    ) {
        self.rawType = rawType
        self.role = role
        self.identifier = identifier
        self.label = label
        self.title = title
        self.placeholderValue = placeholderValue
        self.value = value
        self.text = text
        self.bounds = bounds
        self.enabled = enabled
        self.selected = selected
        self.hasFocus = hasFocus
        self.visible = visible
        self.children = children
    }

    /// Recursively find the first node whose `identifier` matches `id`.
    /// Returns nil when no match anywhere in the subtree (including
    /// self).
    public func findById(_ id: String) -> A11yNode? {
        if identifier == id { return self }
        for child in children {
            if let hit = child.findById(id) { return hit }
        }
        return nil
    }

    /// DFS pre-order flatten — self + all descendants in a single list.
    /// Used by [`ExpectationFailure.visibleElements`] to populate the
    /// "what was on screen when this failed" payload.
    public func flatten() -> [A11yNode] {
        var out: [A11yNode] = [self]
        for child in children {
            out.append(contentsOf: child.flatten())
        }
        return out
    }
}

/// Logical-points rectangle, mirror Rust smix-screen `Rect`.
public struct Rect: Codable, Sendable, Equatable {
    public let x: Double
    public let y: Double
    public let w: Double
    public let h: Double

    public init(x: Double, y: Double, w: Double, h: Double) {
        self.x = x
        self.y = y
        self.w = w
        self.h = h
    }

    /// X coordinate of the bounds center (logical points).
    public var centerX: Double { x + w / 2.0 }
    /// Y coordinate of the bounds center.
    public var centerY: Double { y + h / 2.0 }
}
