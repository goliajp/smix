// The selector forms the tap-family routes accept.
//
// They used to accept one: `text`. `dispatch: daemonProxy` exists for
// React Native views whose Pressable swallows the ordinary gesture
// path, and those views are addressed by testID — which is exactly what
// a text selector cannot reach. The actions guide has documented that
// pairing with an id since it was written, and the Rust-side guard has
// refused it every time.
//
// The predicate downstream already matched `label == %@ OR identifier
// == %@`, so a text selector could land on a testID by accident. That
// is a confused hit rather than support: the caller asked about a label
// and got a match on something else, with no way to tell which. Putting
// the form on the wire is what lets the predicate stop guessing.
//
// Three forms, and the boundary is deliberate: text, id and label are
// what that predicate can express directly. Regex needs the host's
// pattern semantics, roles need its rawType→Role table, and spatial or
// index modifiers need the whole tree walk. Accepting those here would
// put a second implementation of the resolver inside XCUITest — one
// contract with two implementations is the drift this is undoing, not a
// feature to add. They keep resolving host-side, which is what the
// default tap path already does.

import Foundation

/// A selector a runner-side route can resolve without re-implementing
/// the host resolver.
public enum RouteSelector: Equatable, Sendable {
    case text(String)
    case id(String)
    case label(String)

    public enum Failure: Error, Equatable {
        /// No recognised key, or a key this route deliberately does not
        /// take (role / regex / modifiers).
        case unsupportedSelectorForm
        /// A recognised key carrying the wrong type — a regex arrives
        /// as an object, not a string.
        case wrongType(String)
    }

    /// The literal being matched, whichever form this is.
    public var raw: String {
        switch self {
        case .text(let v), .id(let v), .label(let v): return v
        }
    }

    /// The wire key this form came in on. Used when reporting a miss so
    /// the response names the key the caller actually sent.
    public var wireKey: String {
        switch self {
        case .text: return "text"
        case .id: return "id"
        case .label: return "label"
        }
    }

    /// Decode from a `selector` object.
    ///
    /// Key presence is the discriminant, matching how the Rust
    /// `Selector` enum deserializes (untagged, first matching shape
    /// wins). Order is fixed so a body carrying two keys resolves the
    /// same way on both sides rather than by dictionary order.
    public static func decode(from obj: [String: Any]) throws -> RouteSelector {
        for key in ["text", "id", "label"] {
            guard let raw = obj[key] else { continue }
            guard let value = raw as? String else {
                throw Failure.wrongType("selector.\(key) not string")
            }
            switch key {
            case "text": return .text(value)
            case "id": return .id(value)
            default: return .label(value)
            }
        }
        throw Failure.unsupportedSelectorForm
    }
}
