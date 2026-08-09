import FlyingFox
import XCTest

@testable import SmixRunnerCore

/// A `found:false` that says nothing is a dead end.
///
/// A flow whose `appId` differs from the runner's `--bundle` gets
/// `found:false` from `/find` for every selector, while `/tree` — same
/// request, same `App-Bundle-Id` header, same `resolveApp()` — returns
/// that app's nodes in full. Both sides of that could see the symptom
/// and neither could see why: the two routes reach elements differently
/// (`snapshot()` against a live `descendants` query) and only the first
/// honours the rebind.
///
/// So the refusal carries what it saw. Not a fix — the mechanism is
/// still unknown, and this is what makes it knowable.
final class FindDiagnosticsTests: XCTestCase {
  func testFoundTrueStaysTheOldTwoFieldShape() async throws {
    // Nothing to explain when it worked, and a client parsing the old
    // shape must not meet a new field on the happy path.
    let body = try await body(of: FindRoute.success(found: true))
    XCTAssertTrue(body.contains("\"found\":true"))
    XCTAssertFalse(body.contains("\"diagnostics\""))
  }

  func testFoundFalseCarriesWhatItSaw() async throws {
    let body = try await body(of: FindRoute.success(
      found: false,
      diagnostics: FindRoute.Diagnostics(
        appState: 4,
        candidates: 0,
        rebound: true
      )
    ))
    XCTAssertTrue(body.contains("\"found\":false"))
    XCTAssertTrue(body.contains("\"appState\":4"), body)
    XCTAssertTrue(body.contains("\"candidates\":0"), body)
    XCTAssertTrue(body.contains("\"rebound\":true"), body)
  }

  func testDiagnosticsAreOptional() async throws {
    // A runner that cannot tell says nothing rather than inventing a
    // zero, which would read as "the app is not running".
    let body = try await body(of: FindRoute.success(found: false))
    XCTAssertTrue(body.contains("\"found\":false"))
    XCTAssertFalse(body.contains("\"diagnostics\""))
  }

  private func body(of response: HTTPResponse) async throws -> String {
    String(decoding: try await response.bodyData, as: UTF8.self)
  }
}
