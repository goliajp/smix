import FlyingFox
import Foundation

// GET /screenshot → 200 image/png (the bytes) | 503 application/json
//
// Sense layer for a device that has no other way to be seen. `simctl io
// screenshot` covers simulators and Apple exposes nothing equivalent for
// a phone through `devicectl` — but `XCUIScreen.main.screenshot()` runs
// inside this process and works on both. The OCR route has been using
// exactly that call since the a11y-i18n work; what was missing was a way
// to hand the pixels back rather than a verdict about them.
//
// The route owns the envelope only. Taking the picture needs XCUITest,
// which exists in the UI-test host and not in this library, so the host
// injects a handler — the same split `FindTextByOcrRoute` uses, and the
// reason this file's tests can run on any machine.
//
// Wire shape:
//   response: 200 <png bytes>, Content-Type: image/png
//             503 {"error":"screenshot_unavailable","reason":"..."}
//
// A failure is 503 with a reason rather than 200 with an empty body, and
// that is the whole point of the enum below. An empty 200 is written to
// disk as a zero-byte PNG that every later step treats as a screenshot —
// §9#1's third constraint in miniature: saying "this device cannot" beats
// quietly producing nothing.
public enum ScreenshotRoute {
    /// Content type for a successful capture.
    public static let pngContentType = "image/png"

    /// Build the response for a capture attempt.
    ///
    /// `nil` means the host could not produce pixels — the screenshot API
    /// returned nothing, or the image had no PNG representation. Either
    /// way the caller gets a refusal it can read, not a file it cannot.
    public static func response(png: Data?, reason: String = "the runner could not capture the screen")
        -> HTTPResponse
    {
        guard let png, !png.isEmpty else {
            let body = Data(
                #"{"error":"screenshot_unavailable","reason":"\#(reason)"}"#.utf8
            )
            return HTTPResponse(
                statusCode: .serviceUnavailable,
                headers: [HTTPHeader("Content-Type"): "application/json"],
                body: body
            )
        }
        return HTTPResponse(
            statusCode: .ok,
            headers: [HTTPHeader("Content-Type"): pngContentType],
            body: png
        )
    }
}
