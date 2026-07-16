import XCTest
import CoreGraphics
@testable import SmixIndigoHID
import SmixRunnerCore

final class AxpTreeBridgeTests: XCTestCase {
  // case 1 — sidecar parses axTree op
  func test_parseAxTreeOpFromSidecar() throws {
    XCTAssertEqual(
      try SidecarDaemon.parseLine(#"{"op":"axTree","udid":"ABCDEF"}"#),
      .axTree(udid: "ABCDEF")
    )
    XCTAssertThrowsError(try SidecarDaemon.parseLine(#"{"op":"axTree"}"#)) { err in
      XCTAssertEqual(err as? SidecarParseError, .missingField("udid"))
    }
  }

  // case 2 — sidecar formats axTree response (wraps a pre-serialized tree body)
  func test_formatAxTreeResponse() {
    XCTAssertEqual(
      SidecarDaemon.formatAxTree(ok: true, jsonBody: #"{"rawType":"application"}"#),
      #"{"ok":true,"op":"axTree","tree":{"rawType":"application"}}"#
    )
    XCTAssertEqual(
      SidecarDaemon.formatAxTree(ok: false, jsonBody: "{}"),
      #"{"ok":false,"op":"axTree","tree":{}}"#
    )
  }

  // case 3 — AxpElementLike → A11ySnapshotData mapping
  func test_axpElementToSnapshotDataPureMapping() {
    let leaf = AxpTreeBridge.AxpElementLike(
      role: "AXButton",
      frame: CGRect(x: 10, y: 20, width: 80, height: 40),
      label: "OK",
      value: nil,
      identifier: "ok-btn",
      enabled: true,
      focused: false,
      selected: false,
      children: []
    )
    let snap = AxpTreeBridge.elementToSnapshot(leaf)
    XCTAssertEqual(snap.elementTypeRawValue, 9)
    XCTAssertEqual(snap.identifier, "ok-btn")
    XCTAssertEqual(snap.label, "OK")
    XCTAssertNil(snap.value)
    XCTAssertEqual(snap.frame, CGRect(x: 10, y: 20, width: 80, height: 40))
    XCTAssertTrue(snap.isEnabled)
    XCTAssertFalse(snap.isSelected)
    XCTAssertEqual(snap.children.count, 0)

    // value: empty string → nil
    let withEmptyValue = AxpTreeBridge.AxpElementLike(
      role: "AXTextField", frame: .zero, label: "",
      value: "", identifier: "", enabled: true, focused: false,
      selected: false, children: []
    )
    XCTAssertNil(AxpTreeBridge.elementToSnapshot(withEmptyValue).value)
  }

  // case 4 — AX role string → XCUIElementType raw lookup
  func test_axRoleToElementTypeRaw() {
    XCTAssertEqual(AxpTreeBridge.elementTypeRaw(forAXRole: "AXButton"), 9)
    XCTAssertEqual(AxpTreeBridge.elementTypeRaw(forAXRole: "AXTextField"), 49)
    XCTAssertEqual(AxpTreeBridge.elementTypeRaw(forAXRole: "AXStaticText"), 48)
    XCTAssertEqual(AxpTreeBridge.elementTypeRaw(forAXRole: "AXCell"), 75)
    XCTAssertEqual(AxpTreeBridge.elementTypeRaw(forAXRole: "AXImage"), 43)
    XCTAssertEqual(AxpTreeBridge.elementTypeRaw(forAXRole: "AXUnknownRoleXyz"), 1)
  }

  // case 5 — live AXP invocation gate (spike). Opt-in via SMIX_AXP_SPIKE_OK=run
  // because it needs a booted simulator and the bridgeTokenDelegate / XPC
  // routing behaviour is not yet fully characterised. Default skip.
  func test_axpTreeBridgeSpikeRealLiveSim() throws {
    let env = ProcessInfo.processInfo.environment["SMIX_AXP_SPIKE_OK"]
    if env != "run" {
      throw XCTSkip("spike requires SMIX_AXP_SPIKE_OK=run (live AXP invocation against a booted simulator)")
    }
    let udid = try Self.requireDevSimUdid()
    let bridge = try AxpTreeBridge.acquire(udid: udid)
    defer { bridge.release() }
    let snap = try bridge.captureSnapshot()
    XCTAssertGreaterThan(snap.children.count, 0, "AXP returned empty top-level tree")
    XCTAssertGreaterThan(snap.frame.size.width, 0)
    XCTAssertGreaterThan(snap.frame.size.height, 0)
  }

  // Helper — reads .smix/dev-sim.txt (host-side path; the test runs on the
  // host process, not inside the sim). Throws XCTSkip if file missing.
  private static func requireDevSimUdid() throws -> String {
    let cwd = FileManager.default.currentDirectoryPath
    // tests run from swift-bridge/; walk up until .smix/dev-sim.txt is found
    var dir = cwd
    for _ in 0..<5 {
      let candidate = (dir as NSString).appendingPathComponent(".smix/dev-sim.txt")
      if FileManager.default.fileExists(atPath: candidate) {
        let raw = (try? String(contentsOfFile: candidate, encoding: .utf8)) ?? ""
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { return trimmed }
      }
      dir = (dir as NSString).deletingLastPathComponent
    }
    throw XCTSkip("dev-sim.txt not found from cwd=\(cwd)")
  }
}
