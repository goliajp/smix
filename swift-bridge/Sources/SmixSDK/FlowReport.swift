import Foundation

/// What a smix run tells a host test framework.
///
/// Native teams live in Xcode. smix walks in rather than asking them out: a
/// flow runs through the same CLI that CI runs, and its JUnit report becomes
/// whatever XCTest calls a failure.
///
/// The reader is small on purpose — the Rust side has the same one beside
/// the writer, and a Kotlin one exists for Gradle. Three readers of one
/// document is a thing to keep honest, so all three run against the same
/// recorded payloads and are asserted to agree.
public struct FlowReport: Equatable, Sendable {
  /// The flow's name, as the report names it.
  public let flow: String
  /// Whether it passed.
  public let passed: Bool
  /// Why not. `nil` exactly when `passed`.
  public let failure: String?
}

/// Why a report could not be read.
///
/// Distinct from "it failed". A run that never happened and a run that
/// failed want different things from a caller, and one value for both is how
/// an empty string becomes a green test.
public enum FlowReportError: Error, Equatable {
  /// Not a smix report at all — usually the CLI never ran.
  case notAReport
  /// A report, with no flow in it.
  case noFlowInIt
}

public enum SmixFlow {
  /// Parse the JUnit XML `smix run --format junit` writes.
  public static func parse(junitXML xml: String) throws -> FlowReport {
    guard xml.contains("<testsuite") else { throw FlowReportError.notAReport }
    guard let flow = attribute(xml, tag: "<testcase", name: "name") else {
      throw FlowReportError.noFlowInIt
    }
    let raw = between(xml, "<![CDATA[", "]]>")
      ?? attribute(xml, tag: "<failure", name: "message")
    let failure = raw.map(unescape)
    return FlowReport(flow: flow, passed: failure == nil, failure: failure)
  }

  private static func attribute(_ xml: String, tag: String, name: String) -> String? {
    guard let start = xml.range(of: tag) else { return nil }
    let rest = xml[start.lowerBound...]
    guard let end = rest.firstIndex(of: ">") else { return nil }
    let head = rest[..<end]
    guard let keyRange = head.range(of: "\(name)=\"") else { return nil }
    let after = head[keyRange.upperBound...]
    guard let close = after.firstIndex(of: "\"") else { return nil }
    return String(after[..<close])
  }

  private static func between(_ xml: String, _ open: String, _ close: String) -> String? {
    guard let a = xml.range(of: open) else { return nil }
    let rest = xml[a.upperBound...]
    guard let b = rest.range(of: close) else { return nil }
    return String(rest[..<b.lowerBound])
  }

  /// Undo the escaping the writer applies.
  ///
  /// A reader that hands `&quot;` to a developer has made the failure harder
  /// to read than the stdout it replaced.
  private static func unescape(_ s: String) -> String {
    s.replacingOccurrences(of: "&quot;", with: "\"")
      .replacingOccurrences(of: "&apos;", with: "'")
      .replacingOccurrences(of: "&lt;", with: "<")
      .replacingOccurrences(of: "&gt;", with: ">")
      .replacingOccurrences(of: "&amp;", with: "&")
  }
}
