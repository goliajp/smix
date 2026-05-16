import Foundation
#if canImport(Darwin)
import Darwin
#endif
#if canImport(ObjectiveC)
import ObjectiveC.runtime
#endif

/// C2 host-side probe for `AccessibilityPlatformTranslation.framework`.
///
/// Composition of 4 pure sub-probes against the live macOS runtime — they
/// can be unit-tested in isolation (closure / mock-injection) before the
/// production entrypoint `probeLive` wires them up against the real
/// `NSClassFromString` / `class_getMethodImplementation` / `responds(to:)`.
///
/// CLAUDE.md §9.6 invariant maintained: framework is **not** `linkedFramework`
/// in Package.swift and **not** imported as a Swift module — load is via
/// `Darwin.dlopen(...,RTLD_NOW | RTLD_GLOBAL)` (RTLD_GLOBAL is required so
/// the dyld registers the AXP ObjC class metadata before `NSClassFromString`
/// is called — see plan-hot §S1 decision #2).
public enum AxpCapabilityProbe {
  /// Absolute path of the private framework binary inside the
  /// `/System/Library/PrivateFrameworks` bundle (the file itself lives in
  /// the dyld shared cache on macOS 11+, but the path is still the
  /// canonical handle for `dlopen`).
  public static let frameworkPath: String =
    "/System/Library/PrivateFrameworks/AccessibilityPlatformTranslation.framework/AccessibilityPlatformTranslation"

  /// 5 ObjC classes that the v0.7+ host-side AXP tree-read path will need.
  /// Order is part of the wire contract — tests pin the JSON byte sequence.
  public static let requiredClassNames: [String] = [
    "AXPTranslator",
    "AXPMacPlatformElement",
    "AXPTranslationObject",
    "AXPTranslatorRequest",
    "AXPTranslatorResponse",
  ]

  /// 3 instance selectors on `AXPTranslator.sharedInstance` that the
  /// v0.7+ host-side AXP tree-read path will eventually invoke. C2 only
  /// checks `responds(to:)` — does **not** invoke any of them (XPC routing
  /// requires a registered `bridgeTokenDelegate`, which is v0.7+ scope).
  public static let requiredInstanceSelectorNames: [String] = [
    "frontmostApplicationWithDisplayId:bridgeDelegateToken:",
    "macPlatformElementFromTranslation:",
    "objectAtPoint:displayId:bridgeDelegateToken:",
  ]

  public struct FrameworkProbe: Equatable {
    public let path: String
    public let loaded: Bool
    public init(path: String, loaded: Bool) {
      self.path = path
      self.loaded = loaded
    }
  }

  public struct ClassesProbe: Equatable {
    public let resolved: [String]
    public let missing: [String]
    public init(resolved: [String], missing: [String]) {
      self.resolved = resolved
      self.missing = missing
    }
  }

  public struct SelectorsProbe: Equatable {
    public let resolved: [String]
    public let missing: [String]
    public init(resolved: [String], missing: [String]) {
      self.resolved = resolved
      self.missing = missing
    }
  }

  public struct SharedInstanceProbe: Equatable {
    public let available: Bool
    public init(available: Bool) {
      self.available = available
    }
  }

  /// Pure: tests inject a `DlsymResolver` mock (open succeeds → loaded=true,
  /// open returns nil → loaded=false). Production composes
  /// `Darwin.dlopen(_:RTLD_NOW|RTLD_GLOBAL)` separately in `probeLive`.
  public static func probeFramework(via resolver: DlsymResolver) -> FrameworkProbe {
    let loaded = resolver.open(frameworkPath) != nil
    return FrameworkProbe(path: frameworkPath, loaded: loaded)
  }

  /// Pure: takes a lookup closure (production passes `NSClassFromString`).
  /// Result split is order-preserving: `resolved` keeps name order from
  /// `names`, `missing` keeps name order from `names`.
  public static func probeClasses(
    names: [String],
    using lookup: (String) -> AnyClass?
  ) -> ClassesProbe {
    var resolved: [String] = []
    var missing: [String] = []
    for n in names {
      if lookup(n) != nil {
        resolved.append(n)
      } else {
        missing.append(n)
      }
    }
    return ClassesProbe(resolved: resolved, missing: missing)
  }

  /// Pure: looks up `+sharedInstance` via metaclass IMP, invokes it once,
  /// and reports `available` iff the invocation returns a non-nil instance.
  /// `cls == nil` short-circuits to `available: false`. Classes that do
  /// not actually implement `+sharedInstance` are filtered out via
  /// `class_respondsToSelector` *before* IMP invocation — otherwise the
  /// metaclass returns a forwarding stub that traps with `unrecognized
  /// selector` when called.
  public static func probeSharedInstance(on cls: AnyClass?) -> SharedInstanceProbe {
    guard let cls = cls else { return SharedInstanceProbe(available: false) }
    let sel = NSSelectorFromString("sharedInstance")
    guard let metaCls = object_getClass(cls) else {
      return SharedInstanceProbe(available: false)
    }
    guard class_respondsToSelector(metaCls, sel) else {
      return SharedInstanceProbe(available: false)
    }
    let imp = class_getMethodImplementation(metaCls, sel)
    guard let imp = imp else { return SharedInstanceProbe(available: false) }
    typealias Fn = @convention(c) (AnyClass, Selector) -> AnyObject?
    let inst = unsafeBitCast(imp, to: Fn.self)(cls, sel)
    return SharedInstanceProbe(available: inst != nil)
  }

  /// Pure: instance-side `responds(to:)` for each selector name. `instance ==
  /// nil` short-circuits to `resolved=[], missing=names` (everything missing).
  public static func probeInstanceSelectors(
    names: [String],
    on instance: AnyObject?
  ) -> SelectorsProbe {
    guard let instance = instance else {
      return SelectorsProbe(resolved: [], missing: names)
    }
    var resolved: [String] = []
    var missing: [String] = []
    for n in names {
      let sel = NSSelectorFromString(n)
      if (instance as AnyObject).responds(to: sel) {
        resolved.append(n)
      } else {
        missing.append(n)
      }
    }
    return SelectorsProbe(resolved: resolved, missing: missing)
  }

  /// Production entrypoint — composes the 4 sub-probes against the live
  /// runtime. `Darwin.dlopen` flags are local override (`RTLD_NOW |
  /// RTLD_GLOBAL`) because `SystemDlsymResolver.open` uses `RTLD_NOW` only;
  /// AXP requires `RTLD_GLOBAL` so subsequent `NSClassFromString` can find
  /// the freshly-mapped ObjC classes (plan-hot §S1 decision #2).
  ///
  /// `resolver` is currently unused inside this method (the live `dlopen` /
  /// `NSClassFromString` path doesn't need it), but is kept as a parameter
  /// for parity with `IndigoSymbols.resolve(...via:)` so future v0.7+ wiring
  /// can route through a shared resolver.
  public static func probeLive(via resolver: DlsymResolver) -> AxpCapabilityReport {
    _ = resolver
    let handle = frameworkPath.withCString { Darwin.dlopen($0, RTLD_NOW | RTLD_GLOBAL) }
    let framework = FrameworkProbe(path: frameworkPath, loaded: handle != nil)
    guard handle != nil else { return AxpCapabilityReport.allUnavailable() }

    let classes = probeClasses(names: requiredClassNames, using: { NSClassFromString($0) })

    let translatorCls: AnyClass? = NSClassFromString("AXPTranslator")
    let shared = probeSharedInstance(on: translatorCls)

    // Reuse the same +sharedInstance IMP path to get the live instance for
    // selector probing — does NOT trigger XPC routing (XPC requires a
    // registered bridgeTokenDelegate which we do NOT install; v0.7+ scope).
    let instance: AnyObject? = {
      guard shared.available, let cls = translatorCls else { return nil }
      let sel = NSSelectorFromString("sharedInstance")
      guard let metaCls = object_getClass(cls) else { return nil }
      guard class_respondsToSelector(metaCls, sel) else { return nil }
      let imp = class_getMethodImplementation(metaCls, sel)
      guard let imp = imp else { return nil }
      typealias Fn = @convention(c) (AnyClass, Selector) -> AnyObject?
      return unsafeBitCast(imp, to: Fn.self)(cls, sel)
    }()
    let selectors = probeInstanceSelectors(
      names: requiredInstanceSelectorNames,
      on: instance
    )

    return AxpCapabilityReport(
      framework: framework,
      classes: classes,
      selectors: selectors,
      sharedInstance: shared
    )
  }
}
