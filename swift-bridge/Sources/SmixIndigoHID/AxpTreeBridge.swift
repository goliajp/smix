import Foundation
import SmixRunnerCore

#if canImport(CoreGraphics)
import CoreGraphics
#endif

// v1.1 C3 — host-side AX tree resolver via AccessibilityPlatformTranslation.
//
// This file has two concerns:
//
// 1. **Pure mapping** (always works, unit-tested without a sim):
//    - `AxpElementLike` — a struct mirror of the NSAccessibility key set
//      we read off an AXPMacPlatformElement.
//    - `elementToSnapshot(_:)` — recursive map → TreeRoute.A11ySnapshotData
//      (wire-compatible with the existing runner /tree path).
//    - `elementTypeRaw(forAXRole:)` — `AX{Role}` string → XCUIElementType
//      raw integer (reverse of `TreeRoute.elementTypeName`).
//
// 2. **Live AXP invocation** (`acquire` / `release` / `captureSnapshot`):
//    real `dlopen(AccessibilityPlatformTranslation)` + bridgeTokenDelegate
//    install + XPC routing. This is C3's spike-gate work — until that's
//    cleared, `acquire(udid:)` throws `.notImplemented`. The pure mapping
//    above is independently useful for tests and future caching.
public enum AxpTreeBridge {
  public enum AxpTreeError: Error, Equatable {
    case notImplemented(String)
    case dlopenFailed
    case classMissing(String)
    case selectorMissing(String)
    case noFrontmostApp
    case xpcRoutingFailed(String)
  }

  // -- pure mapping ----------------------------------------------------------

  /// Minimal POCO mirroring the NSAccessibility keys we read off a live
  /// `AXPMacPlatformElement`. Used by tests to exercise `elementToSnapshot`
  /// without touching the live framework.
  public struct AxpElementLike: Equatable {
    public let role: String           // e.g. "AXButton" / "AXStaticText"
    public let frame: CGRect
    public let label: String          // AXLabel (NSAccessibility) — may be ""
    public let value: String?         // AXValue — String? subset
    public let identifier: String     // AXIdentifier — may be ""
    public let enabled: Bool          // AXEnabled
    public let focused: Bool          // AXFocused
    public let selected: Bool         // AXSelected
    public let children: [AxpElementLike]

    public init(
      role: String,
      frame: CGRect,
      label: String,
      value: String?,
      identifier: String,
      enabled: Bool,
      focused: Bool,
      selected: Bool,
      children: [AxpElementLike]
    ) {
      self.role = role
      self.frame = frame
      self.label = label
      self.value = value
      self.identifier = identifier
      self.enabled = enabled
      self.focused = focused
      self.selected = selected
      self.children = children
    }
  }

  /// Recursive: `AxpElementLike` → `TreeRoute.A11ySnapshotData`.
  /// `isFocused` from AXFocused is dropped to match the existing
  /// A11ySnapshotData shape (v1 wire keeps isEnabled+isSelected only).
  public static func elementToSnapshot(_ e: AxpElementLike) -> TreeRoute.A11ySnapshotData {
    let kids = e.children.map(elementToSnapshot)
    let valueNorm: String? = (e.value?.isEmpty == true) ? nil : e.value
    return TreeRoute.A11ySnapshotData(
      elementTypeRawValue: elementTypeRaw(forAXRole: e.role),
      identifier: e.identifier,
      label: e.label,
      value: valueNorm,
      frame: e.frame,
      isEnabled: e.enabled,
      isSelected: e.selected,
      children: kids
    )
  }

  /// Reverse of `TreeRoute.elementTypeName`: NSAccessibility role string
  /// (`"AXButton"`, `"AXCell"`, ...) → the XCUIElementType numeric raw value
  /// that the wire uses (kept in sync with `src/core/role.ts`).
  ///
  /// Unknown / missing roles map to 1 ("other"), matching the existing
  /// fallback convention.
  public static func elementTypeRaw(forAXRole role: String) -> UInt {
    switch role {
    case "AXApplication":           return 2
    case "AXGroup":                 return 3
    case "AXWindow":                return 4
    case "AXSheet":                 return 5
    case "AXDrawer":                return 6
    case "AXAlert":                 return 7
    case "AXDialog":                return 8
    case "AXButton":                return 9
    case "AXRadioButton":           return 10
    case "AXRadioGroup":            return 11
    case "AXCheckBox":              return 12
    case "AXDisclosureTriangle":    return 13
    case "AXPopUpButton":           return 14
    case "AXComboBox":              return 15
    case "AXPopover":               return 18
    case "AXKeyboard":              return 19
    case "AXKey":                   return 20
    case "AXNavigationBar":         return 21
    case "AXTabBar":                return 22
    case "AXTabGroup":              return 23
    case "AXToolbar":               return 24
    case "AXStatusBar":             return 25
    case "AXTable":                 return 26
    case "AXRow", "AXTableRow":     return 27
    case "AXColumn", "AXTableColumn": return 28
    case "AXOutline":               return 29
    case "AXOutlineRow":            return 30
    case "AXBrowser":               return 31
    case "AXCollectionView":        return 32
    case "AXSlider":                return 33
    case "AXPageIndicator":         return 34
    case "AXProgressIndicator":     return 35
    case "AXBusyIndicator", "AXActivityIndicator": return 36
    case "AXSegmentedControl":      return 37
    case "AXPicker":                return 38
    case "AXPickerWheel":           return 39
    case "AXSwitch":                return 40
    case "AXToggle":                return 41
    case "AXLink":                  return 42
    case "AXImage":                 return 43
    case "AXIcon":                  return 44
    case "AXSearchField":           return 45
    case "AXScrollArea", "AXScrollView": return 46
    case "AXScrollBar":             return 47
    case "AXStaticText":            return 48
    case "AXTextField":             return 49
    case "AXSecureTextField":       return 50
    case "AXDateTimeArea":          return 51    // approximation for AXDatePicker
    case "AXTextArea", "AXTextView": return 52
    case "AXMenu":                  return 53
    case "AXMenuItem":              return 54
    case "AXMenuBar":               return 55
    case "AXMenuBarItem":           return 56
    case "AXMap":                   return 57
    case "AXWebArea", "AXWebView":  return 58
    case "AXCell":                  return 75
    default:
      return 1   // "other"
    }
  }

  // -- live AXP invocation (C3 spike — gated) --------------------------------

  /// Lifetime-managed handle to an acquired AXP bridge for a given sim. The
  /// host-side implementation will be filled in once the bridgeTokenDelegate
  /// XPC routing has been characterised; until then `acquire(udid:)` throws
  /// `.notImplemented` so the spike test (case 5) can be opted into via
  /// `SMIX_C3_SPIKE_OK=run`.
  public final class Bridge {
    public func captureSnapshot() throws -> TreeRoute.A11ySnapshotData {
      throw AxpTreeError.notImplemented("AxpTreeBridge.Bridge.captureSnapshot — live AXP invocation pending C3 spike (see plan-hot)")
    }

    public func release() {
      // no-op until acquire() actually allocates resources
    }
  }

  public static func acquire(udid: String) throws -> Bridge {
    _ = udid
    throw AxpTreeError.notImplemented("AxpTreeBridge.acquire — live AXP invocation pending C3 spike")
  }
}
