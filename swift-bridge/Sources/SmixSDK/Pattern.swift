// Pattern (literal vs regex) — mirrors Rust smix-selector
// `#[serde(untagged)] enum Pattern`.
//
// Wire JSON forms (untagged — discriminator = which shape matches):
//   Literal: bare string             e.g. "hello"
//   Regex:   {"regex":"...", "flags":"i"}  (flags default "i")

import Foundation

/// A text-match pattern — literal (strict equal) or regex.
public enum Pattern: Sendable, Equatable {
    /// Strict literal match (case-sensitive).
    case literal(String)
    /// Regex match. Flags default to "i" (case-insensitive), matching
    /// Rust smix-selector and Maestro.
    case regex(String, flags: String = "i")
}

extension Pattern: Codable {
    public init(from decoder: Decoder) throws {
        // Try bare string first (literal); fall back to {regex,flags} object.
        if let single = try? decoder.singleValueContainer().decode(String.self) {
            self = .literal(single)
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let r = try container.decode(String.self, forKey: .regex)
        let f = (try? container.decode(String.self, forKey: .flags)) ?? "i"
        self = .regex(r, flags: f)
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .literal(s):
            var c = encoder.singleValueContainer()
            try c.encode(s)
        case let .regex(r, f):
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(r, forKey: .regex)
            try c.encode(f, forKey: .flags)
        }
    }

    private enum CodingKeys: String, CodingKey {
        case regex, flags
    }
}

// Ergonomic constructor — `.text("hello")` style: implicit literal.
extension Pattern: ExpressibleByStringLiteral {
    public init(stringLiteral value: String) {
        self = .literal(value)
    }
}
