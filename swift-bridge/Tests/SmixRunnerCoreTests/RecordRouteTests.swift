import XCTest
import FlyingFox
@testable import SmixRunnerCore

// RecordRoute POCO unit tests. Mirrors BackRouteTests /
// SwipeOnceRouteTests structure. RecordRoute owns decode + envelope only
// (no XCUITest — capture is in the UITest target's EventRecorder).
final class RecordRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // MARK: decode happy paths

  func test_decode_start_empty_body_ok() throws {
    XCTAssertEqual(try RecordRoute.decodeStart(Data()), RecordRoute.StartRequest())
  }

  func test_decode_start_empty_object_ok() throws {
    XCTAssertEqual(try RecordRoute.decodeStart(Data(#"{}"#.utf8)), RecordRoute.StartRequest())
  }

  func test_decode_stop_empty_body_ok() throws {
    XCTAssertEqual(try RecordRoute.decodeStop(Data()), RecordRoute.StopRequest())
  }

  func test_decode_poll_empty_body_ok() throws {
    XCTAssertEqual(try RecordRoute.decodePoll(Data()), RecordRoute.PollRequest())
  }

  // MARK: decode unhappy paths

  func test_decode_start_non_json_throws() {
    XCTAssertThrowsError(try RecordRoute.decodeStart(Data("not-json".utf8))) { error in
      XCTAssertEqual(error as? RecordRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_stop_array_body_throws() {
    XCTAssertThrowsError(try RecordRoute.decodeStop(Data("[1,2]".utf8))) { error in
      XCTAssertEqual(error as? RecordRoute.DecodeError, .invalidJSON)
    }
  }

  // MARK: success envelopes

  func test_start_success_serializes() async throws {
    let resp = RecordRoute.startSuccess()
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
  }

  func test_poll_success_empty_events_serializes() async throws {
    let resp = RecordRoute.pollSuccess(events: [])
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    let events = json["events"] as? [Any]
    XCTAssertNotNil(events)
    XCTAssertEqual(events?.count, 0)
  }

  func test_poll_success_with_event_serializes() async throws {
    let ev = RecordedEvent(
      timestampMs: 1716_700_000_000,
      rawCode: 5,
      kind: "tap",
      location: Point(x: 100.0, y: 200.0),
      selectorHints: SelectorHints(label: "Settings"),
      payloadClassName: "XCAXNotificationPayload"
    )
    let resp = RecordRoute.pollSuccess(events: [ev])
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    let events = json["events"] as? [[String: Any]]
    XCTAssertEqual(events?.count, 1)
    XCTAssertEqual(events?[0]["timestampMs"] as? Int64, 1716_700_000_000)
    XCTAssertEqual(events?[0]["rawCode"] as? Int, 5)
    XCTAssertEqual(events?[0]["kind"] as? String, "tap")
    let loc = events?[0]["location"] as? [String: Any]
    XCTAssertEqual(loc?["x"] as? Double, 100.0)
    XCTAssertEqual(loc?["y"] as? Double, 200.0)
    let hints = events?[0]["selectorHints"] as? [String: Any]
    XCTAssertEqual(hints?["label"] as? String, "Settings")
  }

  func test_stop_success_serializes() async throws {
    let resp = RecordRoute.stopSuccess(events: [])
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
    let events = json["events"] as? [Any]
    XCTAssertEqual(events?.count, 0)
  }

  // MARK: bad request

  func test_bad_request_serializes() async throws {
    let resp = RecordRoute.badRequest(reason: "boom")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, false)
    XCTAssertEqual(json["error"] as? String, "bad_request")
    XCTAssertEqual(json["reason"] as? String, "boom")
  }

  // MARK: RecordedEvent round-trip

  func test_recorded_event_round_trip_full() throws {
    let ev = RecordedEvent(
      timestampMs: 1_000,
      rawCode: 7,
      kind: "swipe",
      location: Point(x: 1.0, y: 2.0),
      endLocation: Point(x: 3.0, y: 4.0),
      durationMs: 500,
      text: "hello",
      selectorHints: SelectorHints(
        identifier: "id-1",
        label: "lbl",
        title: "t",
        placeholderValue: "ph",
        valueString: "v"
      ),
      elementType: 9,
      frame: Rect(x: 0, y: 0, width: 100, height: 50),
      appBundleId: "com.app",
      payloadClassName: "X",
      payloadDescription: "<...>"
    )
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let data = try encoder.encode(ev)
    let back = try JSONDecoder().decode(RecordedEvent.self, from: data)
    XCTAssertEqual(back, ev)
  }

  func test_recorded_event_round_trip_minimal() throws {
    let ev = RecordedEvent(timestampMs: 42, rawCode: 0)
    let data = try JSONEncoder().encode(ev)
    let back = try JSONDecoder().decode(RecordedEvent.self, from: data)
    XCTAssertEqual(back, ev)
  }
}
