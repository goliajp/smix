// v7.1 c1 — Role enum mirror Rust smix-screen Role.
//
// Wire JSON form: lowercase camelCase ("button" / "textField" / etc.) —
// matches Rust `#[derive(Serialize)] #[serde(rename_all = "camelCase")]`
// pattern (verified via crates/smix-screen/src/lib.rs:163-).

import Foundation

/// Semantic UI role — wire form is lowercase camelCase per smix-screen
/// JSON contract.
public enum Role: String, Sendable, Codable, Equatable, CaseIterable {
    case button
    case link
    case textField
    case secureTextField
    case searchField
    case `switch`
    case toggle
    case checkBox
    case radio
    case image
    case staticText
    case tab
    case tabBar
    case navigationBar
    case cell
    case alert
    case dialog
    case slider
    case progressBar
    case picker
    case menu
    case menuItem
    case scrollView
    case segmentedControl
    case table
    case collectionView
    case webView
    case keyboard
}
