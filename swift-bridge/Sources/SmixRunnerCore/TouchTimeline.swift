// When each touch in a burst happens.
//
// A rapid-tap gesture could not be driven through smix: ten `tapOn`
// steps took 4.28 s, and an app gating a hidden trigger on a 500 ms
// inter-tap window sat right on that boundary — so a flow could not
// tell "the app is broken" from "the harness is too slow" (EXT1, #3).
//
// Measured on iPhone 17 Pro / iOS 26.5: `GET /tree` is 68 ms and
// `POST /tap-at-norm-coord` is 466 ms. The cost is the synthesise, not
// the resolve — which is the opposite of what the plan for this
// assumed. Sending ten taps therefore means ten ~400 ms round trips,
// and the interval between them is whatever the network and the
// runner happened to add.
//
// `XCSynthesizedEventRecord` takes several pointer paths, each with
// its own offset on one timeline. So a burst is one synthesise
// carrying N touches, and the spacing stops being a consequence of
// round-trip latency and becomes a number the caller states.

import Foundation

public enum TouchTimeline {
  /// The default gap between touches in a burst, in milliseconds.
  ///
  /// Fast enough to drive a double-tap-style trigger and slow enough
  /// that a recogniser distinguishes the touches. Callers wanting a
  /// specific cadence say so.
  public static let defaultIntervalMs: Int = 80

  /// How long a single touch is held before lifting, in milliseconds.
  public static let defaultHoldMs: Int = 50

  /// Offsets, in seconds, at which each touch of a burst goes down.
  ///
  /// One entry per touch. `times` below 1 yields a single touch —
  /// a burst of none is a caller mistake, and refusing it at the wire
  /// would turn a typo into a failed run rather than a tap.
  ///
  /// Interval is clamped to at least 1 ms: two paths sharing an offset
  /// are one event as far as the recogniser is concerned, so a zero
  /// interval would silently deliver fewer touches than asked for.
  public static func downOffsets(times: Int, intervalMs: Int) -> [TimeInterval] {
    let n = max(1, times)
    let gap = Double(max(1, intervalMs)) / 1000.0
    return (0..<n).map { Double($0) * gap }
  }

  /// The offset at which the touch that went down at `downOffset`
  /// lifts.
  public static func upOffset(downOffset: TimeInterval, holdMs: Int) -> TimeInterval {
    downOffset + Double(max(1, holdMs)) / 1000.0
  }

  /// Wall-clock the whole burst occupies, in seconds.
  ///
  /// The caller waits for the synthesise, so this is what it is
  /// waiting for — worth stating separately because a burst's cost is
  /// its timeline, not a per-touch round trip.
  public static func duration(times: Int, intervalMs: Int, holdMs: Int) -> TimeInterval {
    let last = downOffsets(times: times, intervalMs: intervalMs).last ?? 0
    return upOffset(downOffset: last, holdMs: holdMs)
  }
}
