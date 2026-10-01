import FlyingFox
import Foundation

// POST /hittable {"id":"<a11y-id>"} → 200 {ok, exists, hittable}.
//
// Whether a touch at the element would reach it, asked of XCUITest
// (`isHittable`, which hit-tests the element's hit point). Touches nothing.
//
// The tree marks an element as covered by geometry: something a window
// draws later lies over its centre. That is right for a page sheet and
// wrong for a container that lets touches through, so the host asks this
// before refusing a tap. It is a live query — about a second under a
// presentation — which is why the tree does not ask it of every node.
public enum HittableRoute {
  public enum Answer: Equatable, Sendable {
    case found(hittable: Bool)
    case absent
    /// XCUITest raised while looking: nothing is known.
    case unknown(why: String)
  }

  public static func response(_ a: Answer) -> HTTPResponse {
    let body: String
    switch a {
    case .found(let hittable):
      body = #"{"ok":true,"exists":true,"hittable":\#(hittable)}"#
    case .absent:
      body = #"{"ok":true,"exists":false,"hittable":false}"#
    case .unknown(let why):
      let w = why.replacingOccurrences(of: "\\", with: "\\\\")
        .replacingOccurrences(of: "\"", with: "\\\"")
      body = #"{"ok":false,"error":"hittable_unknown","saw":"\#(w)"}"#
    }
    return HTTPResponse(
      statusCode: .ok, headers: [.contentType: "application/json"], body: Data(body.utf8))
  }
}
