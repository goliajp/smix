import FlyingFox
import Foundation

#if canImport(UIKit)
  import UIKit
#endif

// GET /coordinate-space → 200, and it touches nothing.
//
// The runner has always been able to say what is on screen and to put a
// touch on it, and never to say whether those two agree about where
// anything is. `/tree` reports the accessibility snapshot's geometry; a
// touch is placed by multiplying a normalised offset by
// `XCUIApplication.frame`; and the synthesised event carries an
// interface orientation that decides how the resulting point is read.
// Three numbers, in three places, with nothing that returns them
// together — so when a consumer's landscape taps were reported as
// landing inside a button that never fired, the disagreement could be
// argued about but not measured.
//
// This is the measurement. Sensing is a flat core capability (CLAUDE.md
// §12.1), so it lives here beside `/tree` rather than inside whatever
// happens to need it.
public enum CoordinateSpaceRoute {
  public static func body(
    appFrame: CGRect,
    snapshotRootFrame: CGRect,
    deviceOrientation: String,
    eventRecordOrientation: String,
    nx: Double,
    ny: Double,
    resolvedPoint: CGPoint
  ) -> Data {
    // Stated, not left to the reader. Four numbers and two strings on
    // their own are something everybody reads their own hypothesis
    // into; a boolean is something a script can fail on.
    //
    // Agreement is both halves: the two frames describe the same
    // rectangle, and the space the synthesised event will be read in
    // has the same handedness as the space the point was computed in.
    // Either alone is enough to send a touch somewhere the snapshot
    // never said it would go.
    //
    // The second half is derived from the event's own stamp against the
    // app frame's shape, not from what the device reports. A simulator
    // that has never been rotated answers `unknown`, and comparing
    // against that called portrait — where every tap lands — a
    // disagreement. A predicate that fires where nothing is wrong
    // teaches its reader to ignore it.
    let sameShape =
      appFrame.size.width == snapshotRootFrame.size.width
      && appFrame.size.height == snapshotRootFrame.size.height
    let eventSpaceIsLandscape =
      eventRecordOrientation == "landscapeLeft" || eventRecordOrientation == "landscapeRight"
    let appSpaceIsLandscape = appFrame.size.width > appFrame.size.height
    let agree = sameShape && (eventSpaceIsLandscape == appSpaceIsLandscape)

    func rect(_ r: CGRect) -> String {
      #"{"x":\#(Double(r.origin.x)),"y":\#(Double(r.origin.y)),"#
        + #""w":\#(Double(r.size.width)),"h":\#(Double(r.size.height))}"#
    }

    let json =
      #"{"appFrame":\#(rect(appFrame)),"#
      + #""snapshotRootFrame":\#(rect(snapshotRootFrame)),"#
      + #""deviceOrientation":"\#(jsonEscape(deviceOrientation))","#
      + #""eventRecordOrientation":"\#(jsonEscape(eventRecordOrientation))","#
      + #""spacesAgree":\#(agree),"#
      + #""nx":\#(nx),"ny":\#(ny),"#
      + #""resolvedPoint":{"x":\#(Double(resolvedPoint.x)),"y":\#(Double(resolvedPoint.y))}}"#
    return Data(json.utf8)
  }

  private static func jsonEscape(_ s: String) -> String {
    s.replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
  }

  public static func response(body: Data) -> HTTPResponse {
    HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: body
    )
  }
}

extension CoordinateSpaceRoute {
  /// What the guard answers when the app cannot be reached at all.
  ///
  /// Not an empty 200 with zeroed frames: two 0×0 rectangles compare
  /// equal, so a route whose whole subject is whether two spaces agree
  /// would answer `spacesAgree: true` at the exact moment it knows
  /// nothing.
  public static func unavailable() -> HTTPResponse {
    HTTPResponse(
      statusCode: .serviceUnavailable,
      headers: [.contentType: "application/json"],
      body: Data(
        (#"{"ok":false,"error":"app_unavailable","#
          + #""hint":"the app under test could not be reached, so neither space could be read"}"#)
          .utf8)
    )
  }
}
