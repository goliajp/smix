// What is at a tapped point.
//
// `tapOn` used to report success as soon as a touch was synthesised at
// a coordinate. It was telling the truth about what it did, and that is
// not what a reader takes "tapped" to mean: EXT1 watched it succeed ten
// times in a row against a button whose app-side counter never moved.
//
// The host resolves a selector to an element and sends its centre. This
// answers the other half — what the point turned out to be inside — so
// the host can say whether the two are the same thing.
//
// # Why a chain and not one element
//
// Because the innermost element at a point is usually NOT the one a
// flow aimed at. Read off a live iPhone 17 Pro running Settings, the
// named elements containing the centre of the first row are:
//
//     staticText  "登录以访问iCloud数据…"                      area 7283
//     button      id=com.apple.settings.primaryAppleAccount   area 33423
//     application id=com.apple.Preferences                    area 351348
//
// A flow aiming at that button taps its centre, and the innermost thing
// there is the button's own label. Reporting one element would have the
// host calling a perfectly good tap a miss — and text nested inside a
// row is what every list screen looks like.
//
// # Why named elements only
//
// The same point sits inside 29 elements, most of them anonymous
// full-screen layout containers, several of them siblings in different
// branches. They carry no information the host can act on: it matches
// selectors by identifier and label, and an unnamed 402x874 `other` is
// indistinguishable from a dozen others like it. Filtering to named
// elements is what makes the chain a description rather than a dump.
//
// # Why some are marked behind
//
// A screen presented over another (a photo picker over a sheet over a
// search page) leaves the screens under it in the tree, and their rows
// contain the point too. Siblings are drawn in order, so of the
// siblings that hold anything named at the point the last one is in
// front; everything in the earlier ones is marked `behind`. The host
// lists the front ones to a reader and still judges on the whole chain.

import CoreGraphics
import Foundation

/// One named element containing a point, as reported to the host.
public struct HitChainEntry: Sendable, Equatable {
  public let identifier: String
  public let label: String
  public let frame: CGRect
  /// Under a later sibling that also holds something named at the point.
  public var behind: Bool

  public init(identifier: String, label: String, frame: CGRect, behind: Bool = false) {
    self.identifier = identifier
    self.label = label
    self.frame = frame
    self.behind = behind
  }
}

public enum HitChain {
  /// Every named element whose frame contains `point`: the front ones
  /// first, innermost first within each kind.
  ///
  /// "Innermost" is by area, not by depth. Depth does not order these:
  /// the same point sits in several branches of the snapshot at once,
  /// and a deeper node in one branch is not inside a shallower node in
  /// another. Area is what "inside" means here.
  ///
  /// Empty frames are skipped — a zero-sized element contains nothing,
  /// and `TreeRoute.isVisible` already treats those as not on screen.
  public static func at(
    point: CGPoint,
    in root: TreeRoute.A11ySnapshotData,
    limit: Int = 16
  ) -> [HitChainEntry] {
    var found = walk(CellPlacement.placed(root), point: point)
    found.sort {
      $0.behind != $1.behind
        ? !$0.behind
        : $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height
    }
    // A cap, because the chain rides on every tap response. Sixteen is
    // far above the three-to-four a real screen produces, so hitting it
    // means the shape changed rather than that a screen got deep.
    return Array(found.prefix(limit))
  }

  private static func walk(
    _ node: TreeRoute.A11ySnapshotData,
    point: CGPoint
  ) -> [HitChainEntry] {
    var found: [HitChainEntry] = []
    let f = node.frame
    if f.width > 0, f.height > 0, f.contains(point),
      !node.identifier.isEmpty || !node.label.isEmpty
    {
      found.append(
        HitChainEntry(identifier: node.identifier, label: node.label, frame: f))
    }
    let branches = node.children.map { walk($0, point: point) }
    let front = branches.lastIndex(where: { !$0.isEmpty })
    for (i, branch) in branches.enumerated() {
      found += i == front ? branch : branch.map { e in
        var e = e
        e.behind = true
        return e
      }
    }
    return found
  }
}
