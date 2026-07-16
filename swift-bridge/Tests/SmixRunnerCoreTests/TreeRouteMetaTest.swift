import XCTest
import FlyingFox
#if canImport(CoreGraphics)
import CoreGraphics
#endif
@testable import SmixRunnerCore

/// /tree response carries size + node count as HTTP response
/// headers (`X-Tree-Size-Bytes`, `X-Tree-Node-Count`). JSON body shape
/// is unchanged so legacy A11yNode consumers stay byte-identical; new SDK
/// reads the headers for hot-spot instrumentation.
final class TreeRouteMetaTest: XCTestCase {

  func test_countNodes_singleLeaf_returnsOne() {
    let leaf = TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 9,
      identifier: "",
      label: "OK",
      value: nil,
      frame: CGRect(x: 0, y: 0, width: 100, height: 44),
      isEnabled: true,
      isSelected: false,
      children: []
    )
    XCTAssertEqual(TreeRoute.countNodes(leaf), 1)
  }

  func test_countNodes_threeLevelNested_returnsFour() {
    let leaf = TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 9, identifier: "", label: "", value: nil,
      frame: .zero, isEnabled: true, isSelected: false, children: []
    )
    let cell = TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 75, identifier: "", label: "", value: nil,
      frame: .zero, isEnabled: true, isSelected: false, children: [leaf]
    )
    let window = TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 4, identifier: "", label: "", value: nil,
      frame: .zero, isEnabled: true, isSelected: false, children: [cell]
    )
    let app = TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 2, identifier: "", label: "", value: nil,
      frame: .zero, isEnabled: true, isSelected: false, children: [window]
    )
    XCTAssertEqual(TreeRoute.countNodes(app), 4)
  }

  func test_successWithMeta_emitsHeaders() async throws {
    let payload = Data(#"{"rawType":"button"}"#.utf8)
    let resp = TreeRoute.successWithMeta(payload, sizeBytes: payload.count, nodeCount: 7)
    XCTAssertEqual(resp.statusCode, .ok)
    XCTAssertEqual(resp.headers[.contentType], "application/json")
    XCTAssertEqual(resp.headers[HTTPHeader("X-Tree-Size-Bytes")], String(payload.count))
    XCTAssertEqual(resp.headers[HTTPHeader("X-Tree-Node-Count")], "7")
    // Body must be byte-identical to payload (wire shape unchanged).
    let body = try await resp.bodyData
    XCTAssertEqual(body, payload)
  }

  func test_success_legacy_omitsTreeHeaders() {
    let payload = Data("{}".utf8)
    let resp = TreeRoute.success(payload)
    XCTAssertNil(resp.headers[HTTPHeader("X-Tree-Size-Bytes")])
    XCTAssertNil(resp.headers[HTTPHeader("X-Tree-Node-Count")])
  }
}
