// SystemPopup + SystemPopupButton — mirror the smix-runner-wire
// `/system-popups` shape returned by `SmixSession.systemPopups()`
// (`#[serde(rename_all = "camelCase")]`).

import Foundation

/// One system popup on screen (alert / sheet / banner) — the shape the
/// FFI `SmixSession.systemPopups()` returns.
public struct SystemPopup: Codable, Sendable, Equatable {
    /// Stable identifier for matching across polls.
    public let id: String
    /// Discriminator (e.g. `"alert"` / `"sheet"` / `"banner"`).
    public let type: String
    /// Originator (e.g. bundle id, system framework name).
    public let source: String
    /// Popup title text.
    public let title: String
    /// Popup body text.
    public let body: String
    /// Buttons available on the popup.
    public let buttons: [SystemPopupButton]

    public init(
        id: String,
        type: String,
        source: String,
        title: String = "",
        body: String = "",
        buttons: [SystemPopupButton] = []
    ) {
        self.id = id
        self.type = type
        self.source = source
        self.title = title
        self.body = body
        self.buttons = buttons
    }
}

/// One button on a [`SystemPopup`].
public struct SystemPopupButton: Codable, Sendable, Equatable {
    /// Stable identifier for the button.
    public let id: String
    /// Visible button label.
    public let label: String
    /// Semantic role (`"cancel"` / `"destructive"` / `"default"`).
    public let role: String
    /// Whether tapping this button performs a destructive action.
    public let dangerous: Bool
    /// Optional outcome hint (e.g. `"grants location permission"`).
    public let outcomeHint: String?

    public init(
        id: String,
        label: String,
        role: String,
        dangerous: Bool = false,
        outcomeHint: String? = nil
    ) {
        self.id = id
        self.label = label
        self.role = role
        self.dangerous = dangerous
        self.outcomeHint = outcomeHint
    }
}
