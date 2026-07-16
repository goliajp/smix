import XCTest
@testable import SmixIndigoHID

/// Version-dispatch decision table for `ChannelDispatcher.decide(...)`.
/// Pure function — zero IO, zero dlsym.
final class ChannelDispatcherTests: XCTestCase {

  func test_dispatch_ios26_returnsDigitizer() {
    let d = ChannelDispatcher.decide(
      runtimeVersion: .init(major: 26, minor: 4), override: nil
    )
    XCTAssertEqual(d, .init(channel: .digitizer, warning: nil))
  }

  func test_dispatch_ios26_zero_returnsDigitizer() {
    let d = ChannelDispatcher.decide(
      runtimeVersion: .init(major: 26, minor: 0), override: nil
    )
    XCTAssertEqual(d, .init(channel: .digitizer, warning: nil))
  }

  func test_dispatch_ios17_returnsDigitizer_with_warning_5arg_not_impl() {
    let d = ChannelDispatcher.decide(
      runtimeVersion: .init(major: 17, minor: 5), override: nil
    )
    XCTAssertEqual(
      d,
      .init(channel: .digitizer, warning: .fiveArgIndigoNotImplemented(version: "17.5"))
    )
  }

  func test_dispatch_ios18_returnsDigitizer_with_warning() {
    let d = ChannelDispatcher.decide(
      runtimeVersion: .init(major: 18, minor: 4), override: nil
    )
    XCTAssertEqual(
      d,
      .init(channel: .digitizer, warning: .fiveArgIndigoNotImplemented(version: "18.4"))
    )
  }

  func test_dispatch_override_indigo9_bypassesVersion() {
    let d = ChannelDispatcher.decide(
      runtimeVersion: .init(major: 26, minor: 4), override: .indigo9
    )
    XCTAssertEqual(d, .init(channel: .indigo9, warning: nil))
  }

  func test_dispatch_override_digitizer_bypassesVersion_ios17() {
    let d = ChannelDispatcher.decide(
      runtimeVersion: .init(major: 17, minor: 5), override: .digitizer
    )
    // Override always wins → no warning even on iOS 17 (user knows best).
    XCTAssertEqual(d, .init(channel: .digitizer, warning: nil))
  }

  // MARK: - RuntimeVersion identifier parsing

  func test_runtimeVersion_init_identifier_parses_iOS_26_4() {
    let v = ChannelDispatcher.RuntimeVersion(
      identifier: "com.apple.CoreSimulator.SimRuntime.iOS-26-4"
    )
    XCTAssertEqual(v, .init(major: 26, minor: 4))
  }

  func test_runtimeVersion_init_identifier_returnsNil_when_malformed() {
    XCTAssertNil(
      ChannelDispatcher.RuntimeVersion(identifier: "com.apple.something.weird")
    )
  }

  // MARK: - channel(for:) pure factory

  func test_channel_for_digitizer_returns_DigitizerChannel() {
    let c = ChannelDispatcher.channel(for: .digitizer)
    XCTAssertEqual(c.id, .digitizer)
    XCTAssertTrue(c is DigitizerChannel)
  }

  func test_channel_for_indigo9_returns_IndigoChannel() {
    let c = ChannelDispatcher.channel(for: .indigo9)
    XCTAssertEqual(c.id, .indigo9)
    XCTAssertTrue(c is IndigoChannel)
  }
}
