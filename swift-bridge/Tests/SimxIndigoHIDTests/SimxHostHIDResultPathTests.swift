import XCTest
@testable import SimxIndigoHID

/// C4: `SimxHostHIDResult.success` must carry the `path` field set to whichever
/// dispatch implementation actually ran (`"digitizer"` vs `"indigo9"`).
/// The TS-side smoke script greps this value via `jq -e '.path == "digitizer"'`.
final class SimxHostHIDResultPathTests: XCTestCase {
  func test_success_pathDigitizer_jsonContainsPath() throws {
    let s = SimxHostHIDResult.success(
      path: "digitizer",
      resolved: [
        "IOHIDEventCreateDigitizerEvent",
        "IOHIDEventCreateDigitizerFingerEvent",
        "IOHIDEventAppendEvent",
        "IndigoHIDMessageForTrackpadEventFromHIDEventRef",
      ]
    )
    XCTAssertFalse(s.contains("\n"), "result must be single line: \(s)")
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: Data(s.utf8), options: []) as? [String: Any]
    )
    XCTAssertEqual(dict["ok"] as? Bool, true)
    XCTAssertEqual(dict["path"] as? String, "digitizer")
    let resolved = try XCTUnwrap(dict["resolved"] as? [String])
    XCTAssertTrue(
      resolved.contains("IndigoHIDMessageForTrackpadEventFromHIDEventRef"),
      "resolved must contain SimulatorKit trackpad wrap symbol: \(resolved)"
    )
    XCTAssertTrue(
      resolved.contains("IOHIDEventCreateDigitizerEvent"),
      "resolved must contain IOKit digitizer event symbol: \(resolved)"
    )
  }

  func test_success_pathIndigo9_unchanged_from_c3() throws {
    // Behaviour identical to C3 `SimxHostHIDResultTests` lockdown — only assert
    // the path passthrough piece (the broader JSON shape is C3's contract).
    let s = SimxHostHIDResult.success(
      path: "indigo9",
      resolved: ["IndigoHIDMessageForMouseNSEvent"]
    )
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: Data(s.utf8), options: []) as? [String: Any]
    )
    XCTAssertEqual(dict["ok"] as? Bool, true)
    XCTAssertEqual(dict["path"] as? String, "indigo9")
  }
}
