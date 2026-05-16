import XCTest
@testable import SimxIndigoHID

/// v0.3 C2 — host-side `AccessibilityPlatformTranslation.framework` probe
/// + capability report. Wire JSON byte sequence is the contract with
/// `src/cli/commands/doctor.ts` `JSON.parse` + `isAxpCapability` narrow,
/// so any drift here breaks `simx doctor`.
final class AxpCapabilityProbeTests: XCTestCase {

  // MARK: - probeFramework (DlsymResolver mock injected)

  func test_probeFramework_dlopenSucceeds_returnsLoadedTrue() {
    let r = FakeDlsymResolver(openResult: UnsafeMutableRawPointer(bitPattern: 0xDEAD_BEEF))
    let report = AxpCapabilityProbe.probeFramework(via: r)
    XCTAssertTrue(report.loaded)
    XCTAssertEqual(report.path, AxpCapabilityProbe.frameworkPath)
  }

  func test_probeFramework_dlopenFails_returnsLoadedFalse() {
    let r = FakeDlsymResolver(openResult: nil)
    let report = AxpCapabilityProbe.probeFramework(via: r)
    XCTAssertFalse(report.loaded)
    XCTAssertEqual(report.path, AxpCapabilityProbe.frameworkPath)
  }

  // MARK: - probeClasses (lookup closure injection)

  func test_probeClasses_allPresent_resolvedAllMissingEmpty() {
    let report = AxpCapabilityProbe.probeClasses(
      names: ["AXPTranslator", "AXPMacPlatformElement"],
      using: { _ in NSObject.self }
    )
    XCTAssertEqual(report.resolved, ["AXPTranslator", "AXPMacPlatformElement"])
    XCTAssertEqual(report.missing, [])
  }

  func test_probeClasses_oneMissing_correctSplit() {
    let names = AxpCapabilityProbe.requiredClassNames
    XCTAssertEqual(names.count, 5)
    let report = AxpCapabilityProbe.probeClasses(
      names: names,
      using: { name in name == "AXPTranslationObject" ? nil : NSObject.self }
    )
    XCTAssertEqual(report.resolved.count, 4)
    XCTAssertEqual(report.missing, ["AXPTranslationObject"])
  }

  func test_probeClasses_allMissing_resolvedEmpty() {
    let names = AxpCapabilityProbe.requiredClassNames
    let report = AxpCapabilityProbe.probeClasses(names: names, using: { _ in nil })
    XCTAssertEqual(report.resolved, [])
    XCTAssertEqual(report.missing, names)
  }

  // MARK: - probeSharedInstance

  func test_probeSharedInstance_classNil_returnsAvailableFalse() {
    let report = AxpCapabilityProbe.probeSharedInstance(on: nil)
    XCTAssertFalse(report.available)
  }

  func test_probeSharedInstance_classWithoutSharedInstance_returnsAvailableFalse() {
    // NSDate has no `+sharedInstance` (it has `+date`, `+distantPast`,
    // `+now`, but not `sharedInstance`). The probe must filter via
    // `class_respondsToSelector` *before* IMP invocation, otherwise the
    // metaclass forwarding stub traps with "unrecognized selector".
    let report = AxpCapabilityProbe.probeSharedInstance(on: NSDate.self)
    XCTAssertFalse(report.available)
  }

  func test_probeSharedInstance_classWithSharedInstance_returnsAvailableTrue() {
    // FileManager.default exists; `+sharedInstance` doesn't on it. Use
    // a class we know has `+sharedInstance`: NotificationCenter has
    // `+defaultCenter` (legacy `+default`), but no `+sharedInstance`.
    // NSURLSession.sharedSession exists but the selector name differs.
    // NSUserDefaults `+standardUserDefaults` — wrong name.
    // The simplest cross-platform host class with literal `+sharedInstance`:
    // NSFileCoordinator? — no.  ProcessInfo? — `+processInfo`, not shared.
    // We synthesize a positive case by registering a tiny ObjC class at
    // runtime that does implement `+sharedInstance`. (objc_allocateClassPair
    // is host-side legal.)
    let cls: AnyClass = AxpProbeTestSharedClass.self
    let report = AxpCapabilityProbe.probeSharedInstance(on: cls)
    XCTAssertTrue(report.available)
  }

  // MARK: - probeInstanceSelectors

  func test_probeInstanceSelectors_allRespond_emptyMissing() {
    // NSString really does respond to both `length` and `UTF8String`.
    let s = NSString(string: "hello")
    let report = AxpCapabilityProbe.probeInstanceSelectors(
      names: ["length", "UTF8String"],
      on: s
    )
    XCTAssertEqual(report.resolved, ["length", "UTF8String"])
    XCTAssertEqual(report.missing, [])
  }

  func test_probeInstanceSelectors_instanceNil_allMissing() {
    let names = AxpCapabilityProbe.requiredInstanceSelectorNames
    let report = AxpCapabilityProbe.probeInstanceSelectors(names: names, on: nil)
    XCTAssertEqual(report.resolved, [])
    XCTAssertEqual(report.missing, names)
  }

  func test_probeInstanceSelectors_someMissing_correctSplit() {
    let s = NSString(string: "hello")
    let report = AxpCapabilityProbe.probeInstanceSelectors(
      names: ["length", "totallyNotARealSelector:"],
      on: s
    )
    XCTAssertEqual(report.resolved, ["length"])
    XCTAssertEqual(report.missing, ["totallyNotARealSelector:"])
  }

  // MARK: - AxpCapabilityReport.ok composition

  func test_report_ok_allTrue_returnsTrue() {
    let report = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(resolved: AxpCapabilityProbe.requiredClassNames, missing: []),
      selectors: .init(resolved: AxpCapabilityProbe.requiredInstanceSelectorNames, missing: []),
      sharedInstance: .init(available: true)
    )
    XCTAssertTrue(report.ok)
  }

  func test_report_ok_anyFalse_returnsFalse() {
    let frameworkBad = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: false),
      classes: .init(resolved: AxpCapabilityProbe.requiredClassNames, missing: []),
      selectors: .init(resolved: AxpCapabilityProbe.requiredInstanceSelectorNames, missing: []),
      sharedInstance: .init(available: true)
    )
    XCTAssertFalse(frameworkBad.ok)

    let classesBad = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(resolved: [], missing: AxpCapabilityProbe.requiredClassNames),
      selectors: .init(resolved: AxpCapabilityProbe.requiredInstanceSelectorNames, missing: []),
      sharedInstance: .init(available: true)
    )
    XCTAssertFalse(classesBad.ok)

    let selectorsBad = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(resolved: AxpCapabilityProbe.requiredClassNames, missing: []),
      selectors: .init(resolved: [], missing: AxpCapabilityProbe.requiredInstanceSelectorNames),
      sharedInstance: .init(available: true)
    )
    XCTAssertFalse(selectorsBad.ok)

    let sharedBad = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(resolved: AxpCapabilityProbe.requiredClassNames, missing: []),
      selectors: .init(resolved: AxpCapabilityProbe.requiredInstanceSelectorNames, missing: []),
      sharedInstance: .init(available: false)
    )
    XCTAssertFalse(sharedBad.ok)
  }

  // MARK: - JSON wire-stable byte sequence

  func test_report_json_wireStable_byteSequence_allAvailable() {
    let report = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(resolved: AxpCapabilityProbe.requiredClassNames, missing: []),
      selectors: .init(resolved: AxpCapabilityProbe.requiredInstanceSelectorNames, missing: []),
      sharedInstance: .init(available: true)
    )
    let expected =
      "{\"ok\":true," +
      "\"framework\":{\"path\":\"\(AxpCapabilityProbe.frameworkPath)\",\"loaded\":true}," +
      "\"classes\":{\"resolved\":[" +
      "\"AXPTranslator\",\"AXPMacPlatformElement\",\"AXPTranslationObject\"," +
      "\"AXPTranslatorRequest\",\"AXPTranslatorResponse\"" +
      "],\"missing\":[]}," +
      "\"selectors\":{\"resolved\":[" +
      "\"frontmostApplicationWithDisplayId:bridgeDelegateToken:\"," +
      "\"macPlatformElementFromTranslation:\"," +
      "\"objectAtPoint:displayId:bridgeDelegateToken:\"" +
      "],\"missing\":[]}," +
      "\"sharedInstance\":{\"available\":true}}"
    XCTAssertEqual(report.json(), expected)
  }

  func test_report_json_parseable_5topLevelKeys() throws {
    let report = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(resolved: AxpCapabilityProbe.requiredClassNames, missing: []),
      selectors: .init(resolved: AxpCapabilityProbe.requiredInstanceSelectorNames, missing: []),
      sharedInstance: .init(available: true)
    )
    let s = report.json()
    let data = Data(s.utf8)
    let dict = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
    )
    XCTAssertEqual(dict["ok"] as? Bool, true)
    let framework = try XCTUnwrap(dict["framework"] as? [String: Any])
    XCTAssertEqual(framework["loaded"] as? Bool, true)
    XCTAssertEqual(framework["path"] as? String, AxpCapabilityProbe.frameworkPath)
    let classes = try XCTUnwrap(dict["classes"] as? [String: Any])
    XCTAssertEqual((classes["resolved"] as? [String])?.count, 5)
    XCTAssertEqual((classes["missing"] as? [String])?.count, 0)
    let selectors = try XCTUnwrap(dict["selectors"] as? [String: Any])
    XCTAssertEqual((selectors["resolved"] as? [String])?.count, 3)
    let shared = try XCTUnwrap(dict["sharedInstance"] as? [String: Any])
    XCTAssertEqual(shared["available"] as? Bool, true)
  }

  // MARK: - allUnavailable fallback

  func test_report_allUnavailable_returnsAllFalse() {
    let report = AxpCapabilityReport.allUnavailable()
    XCTAssertFalse(report.ok)
    XCTAssertFalse(report.framework.loaded)
    XCTAssertEqual(report.framework.path, AxpCapabilityProbe.frameworkPath)
    XCTAssertEqual(report.classes.resolved, [])
    XCTAssertEqual(report.classes.missing, AxpCapabilityProbe.requiredClassNames)
    XCTAssertEqual(report.selectors.resolved, [])
    XCTAssertEqual(
      report.selectors.missing,
      AxpCapabilityProbe.requiredInstanceSelectorNames
    )
    XCTAssertFalse(report.sharedInstance.available)
    // Must still be well-formed JSON.
    let dict = try? JSONSerialization.jsonObject(
      with: Data(report.json().utf8), options: []
    ) as? [String: Any]
    XCTAssertNotNil(dict)
  }

  // MARK: - JSON missing-list rendering

  func test_report_json_someMissing_correctSerialization() {
    let report = AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: true),
      classes: .init(
        resolved: ["AXPTranslator", "AXPMacPlatformElement"],
        missing: ["AXPTranslationObject"]
      ),
      selectors: .init(
        resolved: ["frontmostApplicationWithDisplayId:bridgeDelegateToken:"],
        missing: ["macPlatformElementFromTranslation:"]
      ),
      sharedInstance: .init(available: true)
    )
    let s = report.json()
    XCTAssertTrue(s.contains("\"ok\":false"), s)
    XCTAssertTrue(
      s.contains("\"missing\":[\"AXPTranslationObject\"]"),
      s
    )
    XCTAssertTrue(
      s.contains("\"missing\":[\"macPlatformElementFromTranslation:\"]"),
      s
    )
  }

  // MARK: - JSON escape path

  func test_report_json_escapesSpecialChars_inNames() {
    let report = AxpCapabilityReport(
      framework: .init(path: "/weird\"path\\with\nnewline", loaded: false),
      classes: .init(resolved: [], missing: []),
      selectors: .init(resolved: [], missing: []),
      sharedInstance: .init(available: false)
    )
    let s = report.json()
    // Escaped quote, backslash, newline in framework.path.
    XCTAssertTrue(
      s.contains("\"path\":\"/weird\\\"path\\\\with\\nnewline\""),
      s
    )
  }
}

/// Minimal `DlsymResolver` fake — production tests inject the framework
/// open result; sym() and lastErrorDescription() are inert.
private final class FakeDlsymResolver: DlsymResolver {
  private let openResult: UnsafeMutableRawPointer?
  init(openResult: UnsafeMutableRawPointer?) {
    self.openResult = openResult
  }
  func open(_ path: String) -> UnsafeMutableRawPointer? { openResult }
  func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer? { nil }
  func lastErrorDescription() -> String { "fake" }
}

/// Test-only NSObject subclass that implements `+sharedInstance` so the
/// positive-path `probeSharedInstance` assertion has a stable target.
/// Lives entirely inside the test bundle; not exposed in `@_exported` API.
@objc(AxpProbeTestSharedClass)
private final class AxpProbeTestSharedClass: NSObject {
  private static let _instance = AxpProbeTestSharedClass()
  @objc class func sharedInstance() -> AxpProbeTestSharedClass { _instance }
}
