import Foundation

// One-shot graceful-shutdown signal. `/shutdown` fires it; the
// stop-observer task in `runForever` awaits it then calls `server.stop()`.
// Repeated `fire()` is idempotent (continuation resumed at most once) so a
// double POST /shutdown cannot crash. Never fired (no /shutdown call) →
// `wait()` suspends forever → `server.run()` is never stopped → `runForever`
// blocks exactly as it would without the signal (purely additive opt-in).

actor ShutdownSignal {
  private var fired = false
  private var waiters: [CheckedContinuation<Void, Never>] = []

  var didFire: Bool { fired }

  func fire() {
    if fired { return }
    fired = true
    let pending = waiters
    waiters.removeAll()
    for w in pending { w.resume() }
  }

  func wait() async {
    if fired { return }
    await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
      waiters.append(c)
    }
  }
}

// Re-armable restart signal fired by `POST /soft-cycle`. DISTINCT from
// `ShutdownSignal`: firing it bounces the in-process FlyingFox server
// (stop → re-run on the same actor) WITHOUT ending `test_runForever`, so
// the XCUITest host survives an `smix runner cycle` and the ~23 s SIGINT
// teardown is skipped entirely. Never fired → the restart-loop's driver
// simply blocks in `server.run()` exactly as before (purely additive).
//
// `wait()` is cancellation-aware (returns `false` on cancel) so the loop
// can tear the stop-observer down cleanly when a /shutdown — not a
// restart — ends the loop; without that the non-cancellable park would
// wedge the task group at shutdown.
actor RestartSignal {
  private var pending = false
  private var waiter: (id: UUID, cont: CheckedContinuation<Bool, Never>)?
  private var cancelledBeforePark: Set<UUID> = []

  func fire() {
    if let w = waiter {
      waiter = nil
      w.cont.resume(returning: true)
    } else {
      // Fired with no one waiting (server just came back, observer not
      // yet re-parked). Remember it so the next wait() consumes it.
      pending = true
    }
  }

  private func park(_ id: UUID, _ cont: CheckedContinuation<Bool, Never>) {
    if cancelledBeforePark.remove(id) != nil {
      cont.resume(returning: false)
      return
    }
    if pending {
      pending = false
      cont.resume(returning: true)
      return
    }
    if let existing = waiter {
      // Single observer at a time; release any stale waiter defensively.
      waiter = nil
      existing.cont.resume(returning: false)
    }
    waiter = (id, cont)
  }

  private func cancelWaiter(_ id: UUID) {
    if let w = waiter, w.id == id {
      waiter = nil
      w.cont.resume(returning: false)
    } else {
      cancelledBeforePark.insert(id)
    }
  }

  /// Suspend until `fire()` (returns `true`) or the calling task is
  /// cancelled (returns `false`).
  func wait() async -> Bool {
    let id = UUID()
    return await withTaskCancellationHandler {
      await withCheckedContinuation { (cont: CheckedContinuation<Bool, Never>) in
        Task { await self.park(id, cont) }
      }
    } onCancel: {
      Task { await self.cancelWaiter(id) }
    }
  }
}
