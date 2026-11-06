import SmixRunnerCore
import XCTest

// Coordinate gestures that are not a tap: a swipe between two points and
// a drag (press, hold, move, rest, lift), both placed against the frame
// of the app under test, as a tap is.

/// Where `app` sits and how its touches are stamped; nil when it
/// could not be read.
private func placement(of app: XCUIApplication, _ what: String) -> (CGRect, SmixLayout)? {
  var frame = CGRect.zero
  var layout = SmixLayout(size: .zero, orientation: .portrait, source: "frame", displayID: nil)
  let ok = smixGuarded("\(what)-setup") {
    frame = smixAppFrame(app)
    layout = smixLayout(of: app)
    return true
  }
  return ok == true ? (frame, layout) : nil
}

/// Dispatch one record; false, with the reason on stderr, when it fails.
private func synthesize(_ record: SmixEventRecord, _ what: String) async -> Bool {
  do {
    try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
    return true
  } catch {
    FileHandle.standardError.write(Data("smix-runner: \(what): synthesize error: \(error)\n".utf8))
    return false
  }
}

private func unavailable(_ what: String) -> Bool {
  FileHandle.standardError.write(
    Data("smix-runner: \(what): XCSynthesizedEventRecord / XCPointerEventPath unavailable\n".utf8))
  return false
}

/// POST /swipe-at-norm-coord: one finger from `from` to `to`, shares of
/// `app`, over the default swipe duration.
func smixSwipe(
  on app: XCUIApplication, from: (Double, Double), to: (Double, Double)
) async -> Bool {
  let what = "swipe-at-norm-coord"
  guard let (frame, layout) = placement(of: app, what) else { return false }
  func at(_ p: (Double, Double)) -> CGPoint {
    CGPoint(
      x: frame.origin.x + frame.size.width * CGFloat(p.0),
      y: frame.origin.y + frame.size.height * CGFloat(p.1))
  }
  let start = smixDelivery(of: at(from), layout: layout)
  let end = smixDelivery(of: at(to), layout: layout)
  guard let record = SmixEventRecord(orientation: start.orientation, displayID: start.displayID),
    record.addPointerSwipeEvent(from: start.point, to: end.point)
  else { return unavailable(what) }
  return await synthesize(record, what)
}

/// POST /drag-at-norm-coord: the path `DragRoute.path` plans, as one
/// finger in one record.
func smixDrag(on app: XCUIApplication, _ req: DragRoute.DragRequest) async -> Bool {
  let what = "drag-at-norm-coord"
  guard let (frame, layout) = placement(of: app, what) else { return false }
  let waypoints = DragRoute.path(req, size: frame.size).map { w in
    (
      smixDelivery(
        of: CGPoint(x: frame.origin.x + w.point.x, y: frame.origin.y + w.point.y), layout: layout),
      w.offset
    )
  }
  guard let (down, _) = waypoints.first,
    let record = SmixEventRecord(orientation: down.orientation, displayID: down.displayID),
    let path = SmixPointerEventPath.forTouch(at: down.point, offset: 0)
  else { return unavailable(what) }
  for (p, offset) in waypoints.dropFirst() { path.moveTo(point: p.point, atOffset: offset) }
  path.offset = waypoints.last?.1 ?? 0
  path.liftUp()
  record.add(path)
  return await synthesize(record, what)
}

extension SmixEventRecord {
  /// Add one finger's whole path to this record.
  func add(_ path: SmixPointerEventPath) {
    let selector = NSSelectorFromString("addPointerEventPath:")
    typealias Method = @convention(c) (NSObject, Selector, NSObject) -> Void
    let method = unsafeBitCast(record.method(for: selector), to: Method.self)
    method(record, selector, path.path)
  }
}
