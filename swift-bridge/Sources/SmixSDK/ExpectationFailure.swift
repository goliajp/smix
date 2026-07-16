// ExpectationFailure / FailureCode — the AI-readable failure contract.
//
// Mirrors the Rust smix-error `ExpectationFailure` JSON schema; the
// wire form is byte-identical across all three SDK languages.

import Foundation

/// Structured failure thrown by [`Locator`] expect methods + [`App.tap`]
/// when resolver returns no match. Codable so the same JSON shape
/// crosses Rust → FFI → Swift.
public struct ExpectationFailure: Error, LocalizedError, Codable, Sendable, Equatable {
    /// Machine-readable failure category.
    public let code: FailureCode
    /// One-line human-readable summary.
    public let message: String
    /// Optional: the selector being resolved when failure occurred.
    public let selector: Selector?
    /// Up to ~20 a11y nodes from the current tree — populated when
    /// resolver returns 0 matches, helps AI agent diagnose.
    public let visibleElements: [A11yNode]
    /// Heuristic suggestions ("did you mean `.label(...)` instead of
    /// `.id(...)`?").
    public let suggestions: [String]
    /// When the failure was constructed (host wall clock).
    public let timestamp: Date

    public init(
        code: FailureCode,
        message: String,
        selector: Selector? = nil,
        visibleElements: [A11yNode] = [],
        suggestions: [String] = [],
        timestamp: Date = Date()
    ) {
        self.code = code
        self.message = message
        self.selector = selector
        self.visibleElements = visibleElements
        self.suggestions = suggestions
        self.timestamp = timestamp
    }

    /// AI-readable JSON dump — what Claude Code sees when the test
    /// fails. Single-line, sorted-keys, ISO-8601 timestamp.
    public var errorDescription: String? {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        encoder.dateEncodingStrategy = .iso8601
        guard let data = try? encoder.encode(self),
              let str = String(data: data, encoding: .utf8) else {
            return "ExpectationFailure(\(code): \(message))"
        }
        return str
    }
}

/// Machine-readable failure category. Mirror Rust smix-error FailureCode.
public enum FailureCode: String, Sendable, Codable, Equatable, CaseIterable {
    /// Resolver returned 0 candidates after spatial + index filters.
    case notFound
    /// Resolver returned >1 candidate after filters; explicit `.first`
    /// / `.last` / `.nth(...)` modifier required to disambiguate.
    case ambiguous
    /// Element exists but is not enabled / hittable / interactable.
    case notInteractable
    /// `timeout` parameter expired before condition was met.
    case timeout
    /// Element exists but its state contradicts the assertion (e.g.
    /// `toHaveLabel` mismatch).
    case wrongState
    /// Catch-all for runner / wire errors.
    case unknown
}
