import XCTest
@testable import SmixIndigoHID

/// `InputChannel` protocol shape + concrete channel identities. Pure
/// value-level assertions; no dlsym / no IO.
final class InputChannelTests: XCTestCase {

  func test_digitizerChannel_name_is_digitizer() {
    let c: InputChannel = DigitizerChannel()
    XCTAssertEqual(c.id, .digitizer)
    XCTAssertEqual(c.name, "digitizer")
    XCTAssertEqual(InputChannelId.digitizer.rawValue, "digitizer")
  }

  func test_indigoChannel_name_is_indigo9() {
    let c: InputChannel = IndigoChannel()
    XCTAssertEqual(c.id, .indigo9)
    XCTAssertEqual(c.name, "indigo9")
    XCTAssertEqual(InputChannelId.indigo9.rawValue, "indigo9")
  }

  func test_digitizerChannel_requiredSymbolNames_match_DigitizerSymbolNames_allRequired() {
    XCTAssertEqual(
      DigitizerChannel().requiredSymbolNames,
      DigitizerSymbolNames.allRequired
    )
    // And the wire-stable byte values are pinned (defensive against drift):
    XCTAssertEqual(
      DigitizerChannel().requiredSymbolNames,
      [
        "IOHIDEventCreateDigitizerEvent",
        "IOHIDEventCreateDigitizerFingerEvent",
        "IOHIDEventAppendEvent",
        "IndigoHIDMessageForTrackpadEventFromHIDEventRef",
      ]
    )
  }

  func test_indigoChannel_requiredSymbolNames_match_IndigoSymbolNames_allRequired() {
    XCTAssertEqual(
      IndigoChannel().requiredSymbolNames,
      IndigoSymbolNames.allRequired
    )
    XCTAssertEqual(
      IndigoChannel().requiredSymbolNames,
      [
        "IndigoHIDMessageForMouseNSEvent",
        "IndigoHIDMessageForButton",
        "IndigoHIDMessageToCreatePointerService",
        "IndigoHIDMessageToCreateMouseService",
      ]
    )
  }
}
