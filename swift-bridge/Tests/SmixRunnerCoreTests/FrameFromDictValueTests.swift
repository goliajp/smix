import CoreGraphics
import XCTest

@testable import SmixRunnerCore

// v4.3 c1 — frameFromDictValue 纯函数语义锁。act side findAndTapSystemPopupButton
// snapshot 化需从 XCUIElementSnapshot.dictionaryRepresentation 的 frame value
// 拿 button tap center; frame value 形态 = [String: Double] (X/Y/Width/Height)
// 或 CGRect。本 file 锁三态解析 (dict / CGRect / 不可解析→.zero), 与 UITests 侧
// convertSnapshotDict 既有 frame 解析逻辑等价。Core 层无 XCUI 依赖, 入参 Any?。
final class FrameFromDictValueTests: XCTestCase {
  func test_dictForm_parsesXYWH() {
    XCTAssertEqual(
      frameFromDictValue(["X": 10.0, "Y": 20.0, "Width": 100.0, "Height": 44.0]),
      CGRect(x: 10, y: 20, width: 100, height: 44))
  }

  func test_dictForm_missingKeysDefaultZero() {
    XCTAssertEqual(
      frameFromDictValue(["X": 10.0, "Width": 100.0]),
      CGRect(x: 10, y: 0, width: 100, height: 0))
  }

  func test_cgRectForm_returnsAsIs() {
    let r = CGRect(x: 5, y: 6, width: 7, height: 8)
    XCTAssertEqual(frameFromDictValue(r), r)
  }

  func test_unparseable_returnsZero() {
    XCTAssertEqual(frameFromDictValue(["bad": 1.0]), .zero)
    XCTAssertEqual(frameFromDictValue("str"), .zero)
    XCTAssertEqual(frameFromDictValue(nil), .zero)
  }
}
