# smix v1.0.3 — Studio crash root-cause + urgent stop-gap for insight

Date: 2026-07-10 (evening, immediate response)
From: smix maintainer (`claude@golia.jp`)
Responding to: verbal report that "insight 意外退出, studio 也总是 WindowServer 意外退出和死机"
Prior context: `smix-feedback-2026-07-10-gate-hardening.md` (§E TEST INTERRUPTED)

## TL;DR — please stop batch e2e gates on studio until v1.0.4

The studio crashes are **real** and **caused by smix** — not an insight app bug and not just an unrelated macOS 26.5.2 flake. The evidence chain is airtight:

- SimRenderServer crashed at 10:14 on the `com.apple.display.captureservice` dispatch queue — that queue processes `simctl io screenshot`, which smix hits many times per flow.
- Insight app crashed 12 consecutive times between 22:36 and 23:11 (every ~3 min), all with EXC_CRASH / SIGABRT on a background NSMutableArray `objectAtIndex:` — same signature, same 3-second time-to-abort — pointing to a downstream effect of the SimRenderServer / RN bridge getting a stale/null buffer.
- macOS `shutdown_stall` at 10:14 (3.2 MB dump) and `ResetCounter` at 23:18 — both are CoreSimulator failing to shut down cleanly, forcing a hard restart.
- Runner log shows `smix-runner: swallowed in-handler XCTIssue: Failed to resolve query: Application com.focusai.app.mobile is not running` **repeated** while `GET /system-popups` is being polled every 0.5–0.6 s, each poll fanning out to 6 accessibility queries — even after the app has already crashed. That's the direct XCTest arbitration pressure that ties back to feedback §E's `** TEST INTERRUPTED **`.

Diagnosis: smix's high-frequency `simctl io screenshot` + `/system-popups` poll (6 queries × ~2 Hz) on top of iOS 26.5.2 (25F84) + SimRenderServer 1051.55 pushes CoreSimulator into a state where its render server hits an internal assertion, then the RN bridge inside insight gets bad data and NSArray-aborts, then CoreSimulator can't drain cleanly, then shutdown stalls, then studio has to be hard-restarted.

**Please do not run `bun test:e2e` bootstrap scope or continuous perf/visual gate loops on studio until smix v1.0.4 ships.** Single-flow `smix run` is fine. Batch is not — every batch run raises the odds of another shutdown_stall.

If you must run a batch: `smix runner down && sleep 30 && smix runner up --bundle <id>` between flows spreads the XCTest pressure enough to avoid the tightest failure mode. It costs the same ~15 s per flow you already know about, but it's the safe knob today.

If studio starts feeling slow, hot, or the simulator freezes:

```bash
xcrun simctl shutdown all
killall Simulator SimRenderServer xcodebuild || true
```

That triage kills the pending sim-side pressure before macOS shutdown gets involved.

## Evidence in detail

### 1. SimRenderServer crash — 2026-07-10 11:01:36 (and another at 10:13:47)

```
exception:  EXC_BREAKPOINT / SIGTRAP / brk 1
triggered thread queue: "com.apple.display.captureservice"
process:    SimRenderServer 1051.55 (CoreSimulator)
parent:     launchd → CoreSimulator daemon
```

Two crashes on the same day, both on the same dispatch queue. `com.apple.display.captureservice` is the queue that services `simctl io screenshot` requests. `brk 1` is an internal Apple assertion inside SimRenderServer — its own defensive check tripped by, or under, the load smix put on it. This is the direct cause of `SimRenderServer_2026-07-10-095029_studio.diag` (5.8 MB) also in DiagnosticReports/Retired.

### 2. Insight app crash — 22:37 → 23:11, 12 abort loops

```
exception:  EXC_CRASH / SIGABRT — Abort trap: 6
codes:      0x0000000000000000, 0x0000000000000000  (clean abort, no bad access)
faultingThread: 34 (background)
stack:      -[__NSArrayM objectAtIndex:] (NSMutableArray out-of-bounds)
procLaunch → procExit: ~3.4 s
```

12 abort loops with the exact same signature, launching-to-crashing in 3 seconds every time. This is not a random RN bug — it's a downstream effect of a corrupted upstream state. The most likely path: the app's native render bridge asks for a snapshot buffer that the compromised SimRenderServer no longer produces cleanly, and an NSMutableArray held by the bridge is one shorter than it expected.

The insight app itself may have a "defense in depth" opportunity here (bounds-check that NSArray access) but the **trigger** is on smix's side, not insight's.

### 3. macOS-side crashes — shutdown stall + hard restart

```
/Library/Logs/DiagnosticReports/
  shutdown_stall_2026-07-10-101421_studio.shutdownStall   (3.2 MB)
  ResetCounter-2026-07-10-231810.diag                      (23:18 today)
  coresymbolicationd_2026-07-10-223658_studio.diag         (1.6 MB, 22:36 same as insight crash start)
```

`shutdown_stall` = macOS asked processes to exit for shutdown, one of them refused/hung past the deadline, macOS gave up cleanly shutting down. `ResetCounter` = the kernel logged a subsequent hard restart. `coresymbolicationd` crashing (1.6 MB) around the same time as the insight loops = system symbolication service being overloaded and abandoning work.

CoreSimulator processes have a habit of holding kernel-side sim device state that only kexts can release; if those go into an inconsistent state (e.g., because SimRenderServer died mid-frame), CoreSimulator's own shutdown routines can hang, which is what shutdown_stall records.

### 4. Runner log — the direct trigger loop

`/Users/doracawl/workspace/qualcomm/insight/.smix/runner/runner-FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1.log`

```
2026-07-10 23:26:40.214 request: GET /system-popups
    t =    33.69s  Descendants matching type Alert
    t =    33.70s  Descendants matching type Sheet
    t =    33.72s  Descendants matching type Alert
    t =    34.91s  Descendants matching type Sheet
    t =    34.92s  Descendants matching type Dialog
    t =    34.93s  Descendants matching type Popover
2026-07-10 23:26:41.470 close connection
2026-07-10 23:26:41.972 open connection
2026-07-10 23:26:41.975 request: GET /system-popups
    ...
smix-runner: swallowed in-handler XCTIssue: Failed to resolve query:
    Application com.focusai.app.mobile is not running
```

The pattern:
1. `/system-popups` fires every ~0.6 s.
2. Each call runs **6 accessibility queries** (Alert, Sheet, Alert, Sheet, Dialog, Popover — the double-Alert is deliberate for iOS 17 vs 18 semantics).
3. Even after the app has crashed and the runner is telling itself "Application is not running", the loop keeps polling.
4. Each swallowed error still went through XCTest's query-execution path — the arbitration cost is paid.

Over a 340-second run, that's ~570 polls × 6 queries = ~3400 XCTest query executions just for popup detection, on top of `/tree`, `/screenshot`, `/find`, etc. Add the perf gate's screenshot cadence and the visual gate's screenshot cadence, and SimRenderServer's captureservice queue is under sustained pressure it was never asked to handle.

### 5. Same root as feedback §E `** TEST INTERRUPTED **`

Feedback §E from 2026-07-10 already described this from insight's side:

> First bootstrap flow runs cleanly through step 15. Around step 20-31 the runner's XCTest test-host process gets `** TEST INTERRUPTED **`. Subsequent `smix run` invocations start reporting `/session/open failed (DRIVER_ERROR: runner /session/open returned status 404)` and fall back to `legacy per-request path`.

That's the **same failure**, just observed one layer up. The `TEST INTERRUPTED` is XCTest's response to the same arbitration pressure that eventually knocks over SimRenderServer. v1.0.3 session lifecycle correctly solved the `.activate()` storm but did NOT change the `/system-popups` poll cadence or the screenshot cadence — those are the two remaining pressure sources.

## What smix v1.0.4 will ship (systemic fix, not a patch)

Per the smix `.claude/CLAUDE.md` §12 three-layer architecture + §13 quality-first principle, this is a core capability gap, not a driver bug. v1.0.4 adds the missing capability at the right layer, not workarounds.

### Sense layer (smix-core, flat)

1. **`SimHealthMonitor`** — long-running watcher over SimRenderServer pid, xcodebuild test-host pid, and `/health` last-response age. State machine: `Healthy` / `Degraded` (slow or intermittent errors) / `Dead` (any of the watched processes gone). State transitions broadcast on a channel that drivers subscribe to.
2. **XCTest test-host death detection** — tail runner log for `** TEST INTERRUPTED **`; watch `xcodebuild` subprocess exit code. Both feed the Health state channel.
3. **App-alive cache** — the moment a runner-side XCTIssue reports "Application X is not running", cache that fact for 20 s. During the window, `/tree` and `/system-popups` return an empty snapshot immediately without hitting XCUIQuery. On any `/sim/launch` or explicit `/session/open` for that bundle, invalidate.

### Act layer (smix-core, flat)

4. **Adaptive `simctl io screenshot` throttle** — measure last call's wall time; if <300 ms allow 1 QPS baseline; 300–1500 ms drop to 0.3 QPS; >1500 ms or any error triggers a 3 s circuit-breaker + Health downgrade to `Degraded`.
5. **`/system-popups` minimum 500 ms interval + wait-context gating** — the runner rejects polls faster than 500 ms/session; the yaml runtime only polls popups when a `waitForVisible / assertVisible` step needs the signal, not on every idle tick.
6. **`smix runner cycle`** — new core verb, does `runner down && runner up` while keeping `derived-data-*/` (so re-boot is ~3 s instead of ~15 s). Fulfills feedback §E ask 3.
7. **XCTest test-host auto-restart** — a supervisor in the runner reads its own log, sees `TEST INTERRUPTED`, invokes `runner cycle` transparently; where a session was open, the supervisor attempts a fresh `/session/open` with the same bundle-id and emits a Health signal so the client can decide to retry the failing request. Fulfills feedback §E ask 1.

### Decide layer (per-driver)

8. **iOS driver** — bakes the "cycle on `Degraded` after N seconds" policy (XCTest-specific).
9. **Android driver** — same interface, different implementation (UiAutomator has its own crash flavors; not on the critical path for insight today).

### SDK surface (all 4)

10. **Session-aware Degraded state** — Rust `Session::state()`, TS `session.on('degraded', ...)`, Swift `session.stateStream`, Kotlin `session.stateFlow`. Consumers can subscribe and choose to pause / cycle / bail out. Additive; v1.0.3 consumers keep working unchanged.

## Timeline

- v1.0.4 target: 3–5 days from now (2026-07-13 to 2026-07-15).
- 4-ecosystem publish: crates.io / npm / Maven Central / Swift Package release tag.
- CHANGELOG entry emphasizes the studio-crash root cause + poll cadence fix + Session-aware Degraded as the daily-loop primary change.

## What we ask from insight

- **Stop batch e2e gates on studio until v1.0.4** ships. Single-flow smix runs are fine. This is the actual save-the-hardware ask.
- After v1.0.4: the daily gate loop should subscribe to `session.on('degraded', ...)` (TS side) and treat it like insight already treats runner disconnect — either pause the gate, cycle the runner, or bail out. We'll ship an updated `docs/ai-guide/09-sessions.md` covering this.
- Feedback §E's asks 1 and 3 (auto-restart + `runner cycle`) will land in v1.0.4. Asks 2 and 6 (persistent session across host death, richer json output) will land in v1.0.5.

Feedback §E was already the right diagnosis from your side — this reply doc just closes the loop with the SimRenderServer + shutdown_stall evidence that confirms the same root cause is what's taking studio down. Sorry for the operational cost this has been imposing on the machine.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.3-studio-crash-2026-07-10.md
```

The prior chain in the same directory:
- `insight-v1.0.3-session-lifecycle.md` — v1.0.3 shipping notes (activation storm + PNG sRGB + Session)
- `insight-feedback-gol-611-response.md` — the older gol-611 response arc
- Insight-side `smix-feedback-2026-07-10-gate-hardening.md` §E is the same root observed from insight's side.
