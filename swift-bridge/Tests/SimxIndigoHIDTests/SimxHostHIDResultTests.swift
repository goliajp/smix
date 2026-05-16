import XCTest
@testable import SimxIndigoHID

final class SimxHostHIDResultTests: XCTestCase {
  func test_success_singleLineRawJSON_okTrue() throws {
    let s = SimxHostHIDResult.success(
      path: "indigo9",
      resolved: [
        "IndigoHIDMessageForMouseNSEvent",
        "IndigoHIDMessageForButton",
        "IndigoHIDMessageToCreatePointerService",
        "IndigoHIDMessageToCreateMouseService",
      ]
    )
    XCTAssertFalse(s.contains("\n"), "result must be single line: \(s)")
    XCTAssertTrue(s.contains("\"ok\":true"), s)
    XCTAssertTrue(s.contains("\"path\":\"indigo9\""), s)
    XCTAssertTrue(s.contains("\"resolved\":["), s)
    XCTAssertTrue(s.contains("\"IndigoHIDMessageForMouseNSEvent\""), s)

    // must be valid JSON
    let data = Data(s.utf8)
    let any = try JSONSerialization.jsonObject(with: data, options: [])
    let dict = try XCTUnwrap(any as? [String: Any])
    XCTAssertEqual(dict["ok"] as? Bool, true)
    XCTAssertEqual(dict["path"] as? String, "indigo9")
    let resolved = try XCTUnwrap(dict["resolved"] as? [String])
    XCTAssertEqual(resolved.first, "IndigoHIDMessageForMouseNSEvent")
  }

  func test_failure_dlopenFailed_codeAndDetailEchoed() throws {
    let s = SimxHostHIDResult.failure(
      error: .dlopenFailed(path: "/x/SimulatorKit", detail: "image not found")
    )
    XCTAssertFalse(s.contains("\n"), "result must be single line: \(s)")
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: Data(s.utf8), options: []) as? [String: Any]
    )
    XCTAssertEqual(dict["ok"] as? Bool, false)
    XCTAssertEqual(dict["error"] as? String, HostHIDErrorCode.dlopenFailed)
    let detail = try XCTUnwrap(dict["detail"] as? String)
    XCTAssertTrue(detail.contains("/x/SimulatorKit"), detail)
    XCTAssertTrue(detail.contains("image not found"), detail)
  }

  func test_failure_deviceNotBooted_codeIs_udid_not_booted() throws {
    let s = SimxHostHIDResult.failure(error: .deviceNotBooted(udid: "ZZZ-NOT-A-UDID"))
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: Data(s.utf8), options: []) as? [String: Any]
    )
    XCTAssertEqual(dict["ok"] as? Bool, false)
    XCTAssertEqual(dict["error"] as? String, "udid_not_booted")
    XCTAssertEqual(dict["error"] as? String, HostHIDErrorCode.deviceNotBooted)
  }
}
