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

/// Machine-readable failure category. The case names are Swift-idiomatic
/// but the raw values are Rust `smix_error::FailureCode`'s wire strings
/// verbatim — `crates/smix-error/tests/sdk_failure_code_parity.rs` reads
/// this declaration and fails if the two sets ever diverge.
public enum FailureCode: String, Sendable, Codable, Equatable, CaseIterable {
    /// Selector matched zero elements in the visible tree.
    case elementNotFound = "ELEMENT_NOT_FOUND"
    /// Element matched but failed the visibility filter.
    case notVisible = "NOT_VISIBLE"
    /// Element matched but `enabled = false`.
    case notEnabled = "NOT_ENABLED"
    /// Selector matched multiple elements (when uniqueness was required).
    case ambiguous = "AMBIGUOUS"
    /// Operation exceeded the implicit-wait budget.
    case timeout = "TIMEOUT"
    /// `expect` assertion (e.g. `toHaveLabel`) did not hold.
    case assertionFailed = "ASSERTION_FAILED"
    /// Target app exited or never launched.
    case appNotRunning = "APP_NOT_RUNNING"
    /// Simulator device is not booted.
    case simulatorNotBooted = "SIMULATOR_NOT_BOOTED"
    /// The touch was synthesised, and it did not land inside the element the selector matched.
    /// Distinct from element-not-found: not-found means fix the selector, missed means the element was there and the touch went elsewhere.
    case tapMissed = "TAP_MISSED"
    /// The screen is described in one coordinate space and the touch would be delivered in another, so no aim can land where the tree says the element is. Distinct from tap-missed: a miss invites another attempt with a better point, and there is no better point here — whatever is passed gets recomputed against the app's frame and then read against the device's.
    case coordinateSpaceMismatch = "COORDINATE_SPACE_MISMATCH"
    /// Catch-all for runner / driver / IO failures.
    case driverError = "DRIVER_ERROR"
    /// The device's capture path is under load and refusing frames for a stated window. Not a defect and not a driver error: it means "not now, try again shortly", so a caller with time left can keep waiting rather than fail.
    case captureBackpressure = "CAPTURE_BACKPRESSURE"
}
