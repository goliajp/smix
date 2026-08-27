import XCTest

@testable import SmixSDK

/// The same three facts the Rust reader takes out of the same bytes.
///
/// Three readers of one document — Rust beside the writer, this one for
/// XCTest, a Kotlin one for Gradle. What keeps them honest is that all three
/// run against the same recorded payloads, so a shape that drifts breaks all
/// of them at once rather than one of them quietly.
final class AFlowFromATestTests: XCTestCase {

  private func payload(_ name: String) throws -> String {
    let url = try XCTUnwrap(
      Bundle.module.url(forResource: "fixtures/\(name)", withExtension: "xml"),
      "the recorded \(name) payload is missing from the test bundle")
    return try String(contentsOf: url, encoding: .utf8)
  }

  func testAPassingRunNamesTheFlow() throws {
    let r = try SmixFlow.parse(junitXML: payload("passing"))
    XCTAssertEqual(r.flow, "dialog-confirm")
    XCTAssertTrue(r.passed)
    XCTAssertNil(r.failure)
  }

  func testAFailingRunCarriesTheStepTheVerbAndTheReason() throws {
    let r = try SmixFlow.parse(junitXML: payload("failing"))
    XCTAssertFalse(r.passed)
    let f = try XCTUnwrap(r.failure)
    XCTAssertTrue(f.contains("step 2"), "the reason does not say which step: \(f)")
    XCTAssertTrue(f.contains("tapOn"), "the reason does not name the verb: \(f)")
    XCTAssertTrue(
      f.contains("no-such-control"),
      "the reason does not carry the selector that failed: \(f)")
  }

  func testNothingIsNotAPass() {
    // "Could not read this" and "it passed" are different answers, and one
    // value for both is how an empty string becomes a green test. An empty
    // report usually means the CLI never ran.
    XCTAssertThrowsError(try SmixFlow.parse(junitXML: "")) { e in
      XCTAssertEqual(e as? FlowReportError, .notAReport)
    }
    XCTAssertThrowsError(try SmixFlow.parse(junitXML: "total nonsense")) { e in
      XCTAssertEqual(e as? FlowReportError, .notAReport)
    }
  }

  func testASuiteWithNoCaseIsNotAPassEither() {
    let empty = """
      <?xml version="1.0" encoding="UTF-8"?>
      <testsuite name="smix" tests="0" failures="0" errors="0" skipped="0">
      </testsuite>
      """
    XCTAssertThrowsError(try SmixFlow.parse(junitXML: empty)) { e in
      XCTAssertEqual(e as? FlowReportError, .noFlowInIt)
    }
  }

  func testTheAttributePathIsReadAndUnescaped() throws {
    // Both recorded payloads carry CDATA, where nothing is escaped, so
    // neither the attribute fallback nor the unescaping runs against them —
    // a mutation sweep on the Rust side found both surviving. A writer that
    // drops CDATA leaves only the attribute, and there everything IS
    // escaped.
    let attributeOnly = """
      <?xml version="1.0" encoding="UTF-8"?>
      <testsuite name="smix" tests="1" failures="1" errors="0" skipped="0">
        <testcase name="attr-only" classname="smix.flow" time="0">
            <failure type="smix.sdk" message="step 2 (tapOn): not found: { id=&quot;x&quot; }"/>
        </testcase>
      </testsuite>
      """
    let r = try SmixFlow.parse(junitXML: attributeOnly)
    XCTAssertFalse(r.passed)
    let f = try XCTUnwrap(r.failure)
    XCTAssertTrue(f.contains("step 2"), "the attribute path lost the step: \(f)")
    XCTAssertTrue(f.contains("id=\"x\""), "the attribute path did not unescape: \(f)")
  }
}
