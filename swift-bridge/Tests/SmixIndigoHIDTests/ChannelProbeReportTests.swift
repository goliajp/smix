import XCTest
@testable import SmixIndigoHID

/// Wire-stable JSON schema for `ChannelProbeReport`. The byte sequence
/// is the contract with `src/cli/commands/doctor.ts` `JSON.parse`, so any
/// drift here breaks `smix doctor`.
final class ChannelProbeReportTests: XCTestCase {

  // MARK: - happy-case schema bytes locked

  func test_probeReport_jsonSchema_allAvailable_serializes_to_locked_bytes() {
    let report = ChannelProbeReport(channels: [
      .init(
        name: "digitizer",
        available: true,
        resolved: [
          "IOHIDEventCreateDigitizerEvent",
          "IOHIDEventCreateDigitizerFingerEvent",
          "IOHIDEventAppendEvent",
          "IndigoHIDMessageForTrackpadEventFromHIDEventRef",
        ],
        missing: []
      ),
      .init(
        name: "indigo9",
        available: true,
        resolved: [
          "IndigoHIDMessageForMouseNSEvent",
          "IndigoHIDMessageForButton",
          "IndigoHIDMessageToCreatePointerService",
          "IndigoHIDMessageToCreateMouseService",
        ],
        missing: []
      ),
    ])

    let expected =
      "{\"ok\":true,\"channels\":[" +
      "{\"name\":\"digitizer\",\"available\":true,\"resolved\":[" +
      "\"IOHIDEventCreateDigitizerEvent\"," +
      "\"IOHIDEventCreateDigitizerFingerEvent\"," +
      "\"IOHIDEventAppendEvent\"," +
      "\"IndigoHIDMessageForTrackpadEventFromHIDEventRef\"" +
      "],\"missing\":[]}," +
      "{\"name\":\"indigo9\",\"available\":true,\"resolved\":[" +
      "\"IndigoHIDMessageForMouseNSEvent\"," +
      "\"IndigoHIDMessageForButton\"," +
      "\"IndigoHIDMessageToCreatePointerService\"," +
      "\"IndigoHIDMessageToCreateMouseService\"" +
      "],\"missing\":[]}" +
      "]}"

    XCTAssertEqual(report.json(), expected)

    // And it must be parseable JSON for downstream consumers.
    let data = Data(report.json().utf8)
    let any = try? JSONSerialization.jsonObject(with: data, options: [])
    XCTAssertNotNil(any as? [String: Any], "json() must produce valid JSON")
  }

  // MARK: - missing-channel case

  func test_probeReport_jsonSchema_oneMissing_serializes_correctly() throws {
    let report = ChannelProbeReport(channels: [
      .init(
        name: "digitizer",
        available: false,
        resolved: [
          "IOHIDEventCreateDigitizerEvent",
          "IOHIDEventCreateDigitizerFingerEvent",
          "IOHIDEventAppendEvent",
        ],
        missing: ["IndigoHIDMessageForTrackpadEventFromHIDEventRef"]
      ),
      .init(
        name: "indigo9",
        available: true,
        resolved: [
          "IndigoHIDMessageForMouseNSEvent",
          "IndigoHIDMessageForButton",
          "IndigoHIDMessageToCreatePointerService",
          "IndigoHIDMessageToCreateMouseService",
        ],
        missing: []
      ),
    ])
    let s = report.json()
    XCTAssertTrue(s.contains("\"available\":false"), s)
    XCTAssertTrue(
      s.contains("\"missing\":[\"IndigoHIDMessageForTrackpadEventFromHIDEventRef\"]"),
      s
    )
    XCTAssertTrue(s.contains("\"ok\":false"), s)

    // Parse roundtrip
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: Data(s.utf8), options: []) as? [String: Any]
    )
    XCTAssertEqual(dict["ok"] as? Bool, false)
    let channels = try XCTUnwrap(dict["channels"] as? [[String: Any]])
    XCTAssertEqual(channels[0]["available"] as? Bool, false)
    XCTAssertEqual(
      channels[0]["missing"] as? [String],
      ["IndigoHIDMessageForTrackpadEventFromHIDEventRef"]
    )
  }

  // MARK: - JSON escape path

  func test_probeReport_jsonSchema_jsonEscape_handles_special_chars() throws {
    let report = ChannelProbeReport(channels: [
      .init(
        name: "digitizer",
        available: false,
        resolved: [],
        // Hypothetical poisoned symbol name — locks the escape contract
        // through `SmixHostJsonEscape.escape(...)`.
        missing: ["weird\"name\\with\nnewline"]
      ),
    ])
    let s = report.json()
    // Escaped quote, backslash, newline.
    XCTAssertTrue(s.contains("\"weird\\\"name\\\\with\\nnewline\""), s)

    // Result must still be valid JSON.
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: Data(s.utf8), options: []) as? [String: Any]
    )
    let channels = try XCTUnwrap(dict["channels"] as? [[String: Any]])
    XCTAssertEqual(
      channels[0]["missing"] as? [String],
      ["weird\"name\\with\nnewline"]
    )
  }

  // MARK: - ok-field invariant

  func test_probeReport_ok_field_is_false_if_any_channel_unavailable() {
    let report = ChannelProbeReport(channels: [
      .init(name: "digitizer", available: true, resolved: ["a", "b"], missing: []),
      .init(name: "indigo9",   available: false, resolved: [], missing: ["x"]),
    ])
    XCTAssertFalse(report.ok)
    XCTAssertTrue(report.json().contains("\"ok\":false"))
  }

  // MARK: - probe(...) factory uses DlsymResolver

  func test_probe_resolves_via_resolver_when_all_symbols_present() {
    let ioKit = UnsafeMutableRawPointer(bitPattern: 0x10_0000)!
    let sk    = UnsafeMutableRawPointer(bitPattern: 0x20_0000)!
    let resolver = FakeChannelProbeResolver()
    // IOKit (3 symbols)
    resolver.set(handle: ioKit, name: "IOHIDEventCreateDigitizerEvent",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x1)!)
    resolver.set(handle: ioKit, name: "IOHIDEventCreateDigitizerFingerEvent",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x2)!)
    resolver.set(handle: ioKit, name: "IOHIDEventAppendEvent",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x3)!)
    // SimulatorKit (trackpadWrap + 4 indigo)
    resolver.set(handle: sk, name: "IndigoHIDMessageForTrackpadEventFromHIDEventRef",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x4)!)
    resolver.set(handle: sk, name: "IndigoHIDMessageForMouseNSEvent",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x5)!)
    resolver.set(handle: sk, name: "IndigoHIDMessageForButton",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x6)!)
    resolver.set(handle: sk, name: "IndigoHIDMessageToCreatePointerService",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x7)!)
    resolver.set(handle: sk, name: "IndigoHIDMessageToCreateMouseService",
                 ptr: UnsafeMutableRawPointer(bitPattern: 0x8)!)

    let report = ChannelProbeReport.probe(
      simulatorKitHandle: sk, ioKitHandle: ioKit, via: resolver
    )
    XCTAssertTrue(report.ok)
    XCTAssertEqual(report.channels.count, 2)
    XCTAssertEqual(report.channels[0].name, "digitizer")
    XCTAssertEqual(report.channels[0].missing, [])
    XCTAssertEqual(report.channels[1].name, "indigo9")
    XCTAssertEqual(report.channels[1].missing, [])
  }
}

/// Mirror of the digitizer-test fake but keyed on (handle, name) so this
/// suite can exercise both IOKit + SimulatorKit handles in one resolver.
private final class FakeChannelProbeResolver: DlsymResolver {
  private struct Key: Hashable {
    let handle: UnsafeMutableRawPointer
    let name: String
  }

  private var symbols: [Key: UnsafeMutableRawPointer] = [:]

  init() {}

  func set(handle: UnsafeMutableRawPointer, name: String, ptr: UnsafeMutableRawPointer) {
    symbols[Key(handle: handle, name: name)] = ptr
  }

  func open(_ path: String) -> UnsafeMutableRawPointer? {
    UnsafeMutableRawPointer(bitPattern: 0xDEADBEEF)
  }

  func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer? {
    symbols[Key(handle: handle, name: name)]
  }

  func lastErrorDescription() -> String { "fake" }
}
