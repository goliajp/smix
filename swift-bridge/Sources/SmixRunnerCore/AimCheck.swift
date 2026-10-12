import CoreGraphics
import Foundation

/// Whether the element a touch is aimed at is still under the point, read
/// the moment before the touch goes in.
///
/// A consumer's control took itself away between the host's aim and the
/// touch; the touch went in anyway and reached what was under it, and the
/// tap was reported missed after the fact. Told what was aimed at, the
/// runner does not send that touch.
///
/// This chain lists named elements only, so an unnamed target cannot be
/// looked for in it and is touched as before; a named one is matched by
/// the host's rule — identifier when both carry one, else label.
public enum AimCheck {
  /// What is under the point instead of `target`, or nil when the touch
  /// should go in: the target is there, it has no name to look for, or the
  /// point could not be read.
  public static func gone(
    target: TapAtCoordRoute.AimTarget, chain: [HitChainEntry], readable: Bool
  ) -> String? {
    guard readable else { return nil }
    if target.identifier.isEmpty && target.label.isEmpty { return nil }
    if chain.contains(where: { same(target, $0) }) { return nil }
    if chain.isEmpty { return "nothing named" }
    return chain.prefix(3).map(describe).joined(separator: " < ")
  }

  private static func same(_ t: TapAtCoordRoute.AimTarget, _ e: HitChainEntry) -> Bool {
    if !t.identifier.isEmpty && !e.identifier.isEmpty { return t.identifier == e.identifier }
    if !t.label.isEmpty && !e.label.isEmpty { return t.label == e.label }
    return false
  }

  private static func describe(_ e: HitChainEntry) -> String {
    e.identifier.isEmpty ? "\"\(e.label)\"" : "id=\(e.identifier)"
  }
}
