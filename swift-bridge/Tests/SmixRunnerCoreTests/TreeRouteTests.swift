import XCTest
import FlyingFox
#if canImport(CoreGraphics)
import CoreGraphics
#endif
@testable import SmixRunnerCore

final class TreeRouteTests: XCTestCase {

  // MARK: - helpers

  private func mkData(
    type: UInt = 9,
    identifier: String = "",
    label: String = "",
    value: String? = nil,
    frame: CGRect = CGRect(x: 0, y: 0, w: 100, h: 44),
    enabled: Bool = true,
    selected: Bool = false,
    hasFocus: Bool = false,
    children: [TreeRoute.A11ySnapshotData] = []
  ) -> TreeRoute.A11ySnapshotData {
    return TreeRoute.A11ySnapshotData(
      elementTypeRawValue: type,
      identifier: identifier,
      label: label,
      value: value,
      frame: frame,
      isEnabled: enabled,
      isSelected: selected,
      hasFocus: hasFocus,
      children: children
    )
  }

  private func parse(_ data: Data) -> [String: Any] {
    return (try? JSONSerialization.jsonObject(with: data, options: [])) as? [String: Any] ?? [:]
  }

  // MARK: - elementTypeName

  func test_elementTypeName_button_returnsButton() {
    XCTAssertEqual(TreeRoute.elementTypeName(9), "button")
  }

  func test_elementTypeName_cell_returnsCell() {
    XCTAssertEqual(TreeRoute.elementTypeName(75), "cell")
  }

  func test_elementTypeName_application_returnsApplication() {
    XCTAssertEqual(TreeRoute.elementTypeName(2), "application")
  }

  func test_elementTypeName_staticText_returnsStaticText() {
    XCTAssertEqual(TreeRoute.elementTypeName(48), "staticText")
  }

  func test_elementTypeName_unknown_returnsOther() {
    XCTAssertEqual(TreeRoute.elementTypeName(9999), "other")
    XCTAssertEqual(TreeRoute.elementTypeName(1), "other")
  }

  // MARK: - serialize

  func test_serialize_minimalLeaf_producesValidJSON() {
    let node = mkData(
      type: 9,
      label: "OK",
      frame: CGRect(x: 10, y: 20, w: 100, h: 44),
      enabled: true
    )
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["rawType"] as? String, "button")
    XCTAssertEqual(obj["label"] as? String, "OK")
    XCTAssertEqual(obj["enabled"] as? Bool, true)
    XCTAssertEqual(obj["selected"] as? Bool, false)
    XCTAssertEqual(obj["hasFocus"] as? Bool, false)
    XCTAssertEqual(obj["visible"] as? Bool, true)
    let bounds = obj["bounds"] as? [String: Any] ?? [:]
    XCTAssertEqual((bounds["x"] as? NSNumber)?.doubleValue, 10)
    XCTAssertEqual((bounds["y"] as? NSNumber)?.doubleValue, 20)
    XCTAssertEqual((bounds["w"] as? NSNumber)?.doubleValue, 100)
    XCTAssertEqual((bounds["h"] as? NSNumber)?.doubleValue, 44)
    XCTAssertNotNil(obj["children"] as? [Any])
  }

  func test_serialize_omitsEmptyIdentifierLabelValue() {
    let node = mkData(type: 9, identifier: "", label: "", value: nil)
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertNil(obj["identifier"], "identifier key should be omitted when empty")
    XCTAssertNil(obj["label"], "label key should be omitted when empty")
    XCTAssertNil(obj["value"], "value key should be omitted when nil")
  }

  func test_serialize_nestedTwoLevels_childrenArrayPopulated() {
    let cell = mkData(
      type: 75,
      label: "General",
      frame: CGRect(x: 10, y: 100, w: 383, h: 44)
    )
    let app = mkData(
      type: 2,
      identifier: "com.apple.Preferences",
      frame: CGRect(x: 0, y: 0, w: 393, h: 852),
      children: [cell]
    )
    let data = TreeRoute.serialize(app, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["rawType"] as? String, "application")
    XCTAssertEqual(obj["identifier"] as? String, "com.apple.Preferences")
    let kids = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(kids.count, 1)
    XCTAssertEqual(kids[0]["rawType"] as? String, "cell")
    XCTAssertEqual(kids[0]["label"] as? String, "General")
    let cb = kids[0]["bounds"] as? [String: Any] ?? [:]
    XCTAssertEqual((cb["x"] as? NSNumber)?.doubleValue, 10)
    XCTAssertEqual((cb["y"] as? NSNumber)?.doubleValue, 100)
  }

  func test_serialize_visibleHeuristic_inAppFrameTrue() {
    let node = mkData(frame: CGRect(x: 10, y: 100, w: 200, h: 44))
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["visible"] as? Bool, true)
  }

  func test_serialize_visibleHeuristic_outOfFrameFalse() {
    let node = mkData(frame: CGRect(x: -500, y: -500, w: 10, h: 10))
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["visible"] as? Bool, false)
  }

  func test_serialize_visibleHeuristic_zeroFrame_false() {
    let node = mkData(frame: CGRect(x: 0, y: 0, w: 0, h: 0))
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["visible"] as? Bool, false)
  }

  func test_serialize_truncatesAtMaxDepth() {
    // c5i-a S3: MAX_DEPTH bumped 60→500 (RN flat-deep dashboard 真 80-200 levels
    // 真 baseline; 60 真截掉 staticText). Build a chain deeper than 500 to
    // verify the cliff still works.
    var chain = mkData(type: 9, label: "leaf")
    for _ in 0..<(TreeRoute.MAX_DEPTH + 2) {
      chain = mkData(type: 3, children: [chain])
    }
    var logs: [String] = []
    let logSink: (String) -> Void = { logs.append($0) }
    let data = TreeRoute.serialize(chain, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: logSink)
    // c5i-a S3: walk down to MAX_DEPTH (bumped 60→500) — at the cliff children
    // must be empty array. Use [Any] cast (not [[String:Any]]) so we keep
    // walking while non-empty AND stop as soon as kids = [].
    var node: [String: Any] = parse(data)
    var depth = 0
    while let kids = node["children"] as? [Any], !kids.isEmpty {
      guard let next = kids.first as? [String: Any] else { break }
      node = next
      depth += 1
    }
    XCTAssertEqual(depth, TreeRoute.MAX_DEPTH, "walk should reach MAX_DEPTH=\(TreeRoute.MAX_DEPTH), actual=\(depth)")
    let kidsAtCliff = node["children"] as? [Any]
    XCTAssertEqual(kidsAtCliff?.count, 0, "expected empty children at MAX_DEPTH=\(TreeRoute.MAX_DEPTH), got \(kidsAtCliff as Any)")
    XCTAssertGreaterThanOrEqual(logs.count, 1)
    XCTAssertTrue(logs.contains { $0.contains("truncated") }, "logSink should record a 'truncated' warning, got \(logs)")
  }

  func test_serialize_hasFocusReflectsInput() {
    // v5.1 c1 — `focused` 解冻:`out["hasFocus"]` 不再是 placeholder false,
    // 而是把 POCO `hasFocus` 真实输出。POCO 这边是普通 field passthrough;
    // UITest 侧 snapshotHandler 通过 KVC `value(forKey: "hasKeyboardFocus")`
    // 在 snapshot 子树深度优先找 first responder,然后把 identifier 作为
    // focusHint 沿 convertSnapshotDict 子树下传,匹配的节点 set hasFocus=true。
    let inner = mkData(type: 9, label: "inner", hasFocus: false)
    let outer = mkData(type: 2, hasFocus: true, children: [inner])
    let data = TreeRoute.serialize(outer, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["hasFocus"] as? Bool, true)
    let kids = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(kids.first?["hasFocus"] as? Bool, false)
  }

  func test_serialize_selectedFieldReflectsInput() {
    let node = mkData(type: 9, label: "Btn", selected: true)
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["selected"] as? Bool, true)
  }

  func test_serialize_valueFieldKept_whenNonEmpty() {
    let node = mkData(type: 49, label: "Email", value: "user@example.com")
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["value"] as? String, "user@example.com")
  }

  func test_serialize_fractionalBoundsPreserved() {
    let node = mkData(frame: CGRect(x: 10.0, y: 20.5, w: 300.0, h: 44.0))
    let data = TreeRoute.serialize(node, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let bounds = obj["bounds"] as? [String: Any] ?? [:]
    XCTAssertEqual((bounds["y"] as? NSNumber)?.doubleValue, 20.5)
  }

  // MARK: - v1.0.21 D1 — action-container button promotion

  // iOS 26.5 XCUITest exposes UIAlertController action buttons with
  // elementType `.other` (rawValue 1) instead of `.button` (9). Under
  // .alert / .dialog / .sheet ancestors we promote labeled `.other` /
  // `.staticText` nodes to `rawType: "button"` so `role: button` yaml
  // still matches on iOS 26+.

  func test_serialize_alertOtherChildWithLabel_promotedToButton() {
    let cancelBtn = mkData(type: 1, label: "Cancel")
    let okBtn = mkData(type: 1, label: "OK")
    let alert = mkData(type: 7, label: "Confirm", children: [cancelBtn, okBtn])
    let data = TreeRoute.serialize(alert, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    XCTAssertEqual(obj["rawType"] as? String, "alert")
    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children.count, 2)
    XCTAssertEqual(children[0]["rawType"] as? String, "button", "Cancel .other under .alert must promote to button")
    XCTAssertEqual(children[0]["label"] as? String, "Cancel")
    XCTAssertEqual(children[1]["rawType"] as? String, "button", "OK .other under .alert must promote to button")
  }

  func test_serialize_alertStaticTextChildWithLabel_promotedToButton() {
    let btn = mkData(type: 48, label: "Reload")
    let alert = mkData(type: 7, label: "Update", children: [btn])
    let data = TreeRoute.serialize(alert, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children[0]["rawType"] as? String, "button", ".staticText with label under .alert must promote")
  }

  func test_serialize_dialogNestedButton_promoted() {
    // .other button several layers deep under .dialog still promotes.
    let deepBtn = mkData(type: 1, label: "Retry")
    let inner = mkData(type: 3, children: [deepBtn])  // group wrapper
    let dialog = mkData(type: 8, label: "Error", children: [inner])
    let data = TreeRoute.serialize(dialog, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let dialogChildren = obj["children"] as? [[String: Any]] ?? []
    let innerChildren = dialogChildren[0]["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(innerChildren[0]["rawType"] as? String, "button", "nested .other in dialog subtree must promote")
  }

  func test_serialize_alertOtherChildNoLabel_notPromoted() {
    // Empty-label .other under .alert stays as "other" — promotion
    // requires a non-empty label so we don't sweep up decorative
    // background views.
    let deco = mkData(type: 1, label: "")
    let alert = mkData(type: 7, label: "X", children: [deco])
    let data = TreeRoute.serialize(alert, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children[0]["rawType"] as? String, "other", "no-label .other under alert stays other")
  }

  func test_serialize_otherOutsideActionContainer_notPromoted() {
    // Baseline: .other with label OUTSIDE any alert/dialog/sheet must
    // remain `other` — promotion is scoped to action containers only.
    let leaf = mkData(type: 1, label: "just some other")
    let root = mkData(type: 2, children: [leaf])  // application
    let data = TreeRoute.serialize(root, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children[0]["rawType"] as? String, "other", ".other with label outside action container stays other")
  }

  func test_serialize_realButtonUnderAlert_stillButton() {
    // A real .button (rawValue 9) under .alert stays button. The
    // promotion only lifts `.other` / `.staticText` — never touches
    // pre-existing buttons.
    let btn = mkData(type: 9, label: "Yes")
    let alert = mkData(type: 7, label: "?", children: [btn])
    let data = TreeRoute.serialize(alert, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children[0]["rawType"] as? String, "button")
  }

  func test_serialize_sheetOtherChild_promoted() {
    // .sheet is an action container too — matches SwiftUI
    // .confirmationDialog / actionSheet exposure.
    let btn = mkData(type: 1, label: "Delete")
    let sheet = mkData(type: 5, label: "Delete confirm", children: [btn])
    let data = TreeRoute.serialize(sheet, appFrame: CGRect(x: 0, y: 0, w: 393, h: 852), logSink: nil)
    let obj = parse(data)
    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children[0]["rawType"] as? String, "button", ".other under .sheet must promote")
  }

  // MARK: - response builders

  func test_success_returns200WithBody() async throws {
    let payload = Data("[]".utf8)
    let resp = TreeRoute.success(payload)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, "[]")
    XCTAssertEqual(resp.headers[.contentType], "application/json")
  }

  func test_unavailable_returns500SnapshotUnavailable() async throws {
    let resp = TreeRoute.unavailable()
    XCTAssertEqual(resp.statusCode, .internalServerError)
    let bodyStr = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(bodyStr.contains("\"ok\":false"), "body should contain ok:false, got \(bodyStr)")
    XCTAssertTrue(bodyStr.contains("\"error\":\"snapshot_unavailable\""), "body should contain error key, got \(bodyStr)")
    XCTAssertEqual(resp.headers[.contentType], "application/json")
  }

  // MARK: - isVisible direct

  func test_isVisible_intersectingFrame_true() {
    XCTAssertTrue(TreeRoute.isVisible(
      CGRect(x: 0, y: 800, w: 393, h: 100),
      appFrame: CGRect(x: 0, y: 0, w: 393, h: 852)
    ))
  }
}

// Convenience init used only by tests to keep call sites short.
private extension CGRect {
  init(x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat) {
    self.init(x: x, y: y, width: w, height: h)
  }
}
