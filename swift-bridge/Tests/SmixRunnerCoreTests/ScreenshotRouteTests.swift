import XCTest
import FlyingFox
@testable import SmixRunnerCore

// ScreenshotRoute POCO unit tests. No XCUITest here — the route owns the
// envelope, and taking the picture belongs to the UI-test host.
//
// case A: bytes in → 200, image/png, body unchanged
// case B: nil in → 503 + a JSON reason, NOT an empty 200
// case C: empty bytes in → same refusal as nil (a zero-byte PNG is not a
//         screenshot, and it is the shape a silent failure takes)
final class ScreenshotRouteTests: XCTestCase {

    // The first eight bytes of any PNG.
    private let pngMagic = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])

    // case A
    func test_bytes_come_back_verbatim_as_png() async throws {
        let pixels = pngMagic + Data(repeating: 0x42, count: 64)
        let resp = ScreenshotRoute.response(png: pixels)
        XCTAssertEqual(resp.statusCode, .ok)
        XCTAssertEqual(resp.headers[HTTPHeader("Content-Type")], "image/png")
        let body = try await resp.bodyData
        // Verbatim matters: a re-encode here would change the bytes the
        // caller compares against a baseline.
        XCTAssertEqual(body, pixels)
    }

    // case B
    func test_no_pixels_is_a_refusal_not_an_empty_success() async throws {
        let resp = ScreenshotRoute.response(png: nil, reason: "XCUIScreen returned no image")
        XCTAssertEqual(resp.statusCode, .serviceUnavailable)
        let body = try await resp.bodyData
        let json = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] ?? [:]
        XCTAssertEqual(json["error"] as? String, "screenshot_unavailable")
        XCTAssertEqual(json["reason"] as? String, "XCUIScreen returned no image")
    }

    // case C
    func test_empty_bytes_refuse_the_same_way() async throws {
        // A 200 with nothing in it lands on disk as a zero-byte PNG, and
        // every step after that treats it as a picture of the screen.
        // That is the failure this route exists to not have.
        let resp = ScreenshotRoute.response(png: Data())
        XCTAssertEqual(resp.statusCode, .serviceUnavailable)
    }
}
