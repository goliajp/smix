import XCTest
import FlyingFox
@testable import SmixRunnerCore

/// Which hardware buttons an iOS runner cannot press, and the answer it
/// gives instead of pressing nothing.
///
/// These keys used to be skipped on the host for every device with the
/// simulator's reason, so the runner never had to answer. Now the host
/// sends every key and the device says what it has.
final class MissingButtonTest: XCTestCase {

  private func parseBody(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  func test_lockHasNoButtonAnywhere() {
    for onSimulator in [true, false] {
      let why = KeyboardRoute.missingButton(key: "lock", onSimulator: onSimulator)
      XCTAssertNotNil(why, "onSimulator=\(onSimulator)")
      XCTAssertTrue(why?.contains("XCUIDevice") == true, why ?? "")
    }
  }

  func test_volumeHasNoButtonOnTheSimulatorAndIsNotDrivenOnAPhone() {
    for key in ["volumeUp", "volumeDown"] {
      let sim = KeyboardRoute.missingButton(key: key, onSimulator: true)
      XCTAssertTrue(sim?.contains("Simulator") == true, sim ?? "nil for \(key)")
      let phone = KeyboardRoute.missingButton(key: key, onSimulator: false)
      XCTAssertNotNil(phone, key)
      XCTAssertFalse(phone?.contains("Simulator has no") == true,
        "a phone's refusal must not borrow the simulator's reason: \(phone ?? "")")
    }
  }

  func test_keysThatArePressedAreNotRefused() {
    for key in ["home", "return", "delete", "tab", "space", "escape", "arrowUp"] {
      XCTAssertNil(KeyboardRoute.missingButton(key: key, onSimulator: true), key)
    }
  }

  func test_theRefusalNamesItselfAndCarriesTheReason() async throws {
    let resp = KeyboardRoute.noSuchButton(saw: "the reason")
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parseBody(resp)
    XCTAssertEqual(json["ok"] as? Bool, false)
    XCTAssertEqual(json["error"] as? String, "no_such_button")
    XCTAssertEqual(json["saw"] as? String, "the reason")
  }
}
