import XCTest
@testable import SmixRunnerCore

// v1.0.10 §D5 — counter regression tests.
//
// Locks in the observability contract: every mutation to
// AppAliveCache advances the paired counter, and counterSnapshot()
// returns a coherent point-in-time view. Without this test the
// noteReprobe* wire-ins in SmixRunnerUITests could silently regress
// and insight's grep-can't-tell problem would come back.

final class AppAliveCacheCountersTests: XCTestCase {
  func testMarkDeadAdvancesCounter() async {
    let cache = AppAliveCache(ttlMs: 10_000)
    let before = await cache.counterSnapshot()
    await cache.markDead(bundleId: "com.example.app")
    let after = await cache.counterSnapshot()
    XCTAssertEqual(after.markDeadTotal, before.markDeadTotal + 1)
    XCTAssertEqual(after.markAliveTotal, before.markAliveTotal)
  }

  func testMarkAliveAdvancesCounter() async {
    let cache = AppAliveCache(ttlMs: 10_000)
    await cache.markDead(bundleId: "com.example.app")
    let mid = await cache.counterSnapshot()
    await cache.markAlive(bundleId: "com.example.app")
    let after = await cache.counterSnapshot()
    XCTAssertEqual(after.markAliveTotal, mid.markAliveTotal + 1)
  }

  func testSuppressHitAndMissCounters() async {
    let cache = AppAliveCache(ttlMs: 10_000)
    // Not suppressed — the "miss" branch increments.
    _ = await cache.isSuppressed(bundleId: "com.example.app")
    let afterMiss = await cache.counterSnapshot()
    XCTAssertEqual(afterMiss.suppressMissTotal, 1)
    XCTAssertEqual(afterMiss.suppressHitTotal, 0)

    await cache.markDead(bundleId: "com.example.app")
    // Suppressed — the "hit" branch increments.
    let suppressed = await cache.isSuppressed(bundleId: "com.example.app")
    XCTAssertTrue(suppressed)
    let afterHit = await cache.counterSnapshot()
    XCTAssertEqual(afterHit.suppressHitTotal, 1)
    XCTAssertEqual(afterHit.suppressMissTotal, 1)
  }

  func testReprobeCountersAreIndependent() async {
    let cache = AppAliveCache(ttlMs: 10_000)
    await cache.noteReprobeAttempted()
    await cache.noteReprobeAttempted()
    await cache.noteReprobeSucceeded()
    await cache.noteReprobeInvalidatedEarly()
    await cache.noteReprobeExhaustedWindow()

    let c = await cache.counterSnapshot()
    XCTAssertEqual(c.reprobeAttemptedTotal, 2)
    XCTAssertEqual(c.reprobeSucceededTotal, 1)
    XCTAssertEqual(c.reprobeInvalidatedEarly, 1)
    XCTAssertEqual(c.reprobeExhaustedWindow, 1)
  }

  func testDiagnosticResponseSerialisesCounters() async throws {
    let counters = SessionRoute.AliveCacheCounters(
      markDeadTotal: 3,
      markAliveTotal: 2,
      suppressHitTotal: 5,
      suppressMissTotal: 7,
      reprobeAttemptedTotal: 1,
      reprobeSucceededTotal: 1,
      reprobeInvalidatedEarly: 0,
      reprobeExhaustedWindow: 0
    )
    let snapshot = SessionRoute.DiagnosticSnapshot(
      sessions: [],
      simHealth: "healthy",
      supervisorPid: nil,
      uptimeMs: 12345,
      aliveCache: counters
    )
    let response = SessionRoute.diagnosticResponse(snapshot)
    let bodyData = try await response.bodyData ?? Data()
    let bodyString = String(data: bodyData, encoding: .utf8) ?? ""
    XCTAssertTrue(bodyString.contains(#""aliveCache":{"#), "aliveCache field missing: \(bodyString)")
    XCTAssertTrue(bodyString.contains(#""markDeadTotal":3"#))
    XCTAssertTrue(bodyString.contains(#""reprobeSucceededTotal":1"#))
    XCTAssertTrue(bodyString.contains(#""uptimeMs":12345"#))
  }

  func testDiagnosticResponseOmitsAliveCacheWhenNil() async throws {
    // Runner opted out of app-alive caching → snapshot.aliveCache is
    // nil → the wire body doesn't emit the field (clients still parse
    // the rest, backward-compatible).
    let snapshot = SessionRoute.DiagnosticSnapshot(
      sessions: [],
      simHealth: "healthy",
      supervisorPid: nil,
      uptimeMs: 42
    )
    let response = SessionRoute.diagnosticResponse(snapshot)
    let bodyData = try await response.bodyData ?? Data()
    let bodyString = String(data: bodyData, encoding: .utf8) ?? ""
    XCTAssertFalse(bodyString.contains("aliveCache"), "aliveCache must be omitted when nil: \(bodyString)")
    XCTAssertTrue(bodyString.contains(#""uptimeMs":42"#))
  }
}
