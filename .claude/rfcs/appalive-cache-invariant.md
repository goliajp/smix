# AppAliveCache — invariant and consumer-visible symptoms

Written: 2026-07-11 (v1.0.11 §D6, insight §C.6 ask)
Location: `.claude/rfcs/appalive-cache-invariant.md`

This is not an RFC for a change — it's a standing note that explains
what the `AppAliveCache` actor in `Sources/SmixRunnerCore/SmixRunnerServer.swift`
is protecting, so consumers reading a `unknown` a11y-tree row can pattern-
match on the model instead of the symptom.

## The invariant

> When XCUITest raises an `XCTIssue "Application X is not running"`
> during a handler, the cache marks bundle X as "known dead" for
> `ttlMs` (default 20 s). Any subsequent request touching X's tree
> during that window short-circuits without going back through
> XCUITest — the handler returns a `unknown` envelope immediately.
> A background re-probe polls `XCUIApplication.state` every 3 s during
> the window; on the first `.runningForeground` (or any non-`.notRunning`
> state), it calls `markAlive` and the suppression drops.

The purpose is to break the retry-spam loop where a caller sees an
error, retries, XCUITest raises the same `XCTIssue` again, and the
cycle burns ~5-10 s per turn amplifying the failure. Suppression
returns a definitive "unknown" fast so the caller's next `waitFor`
can move on.

## What the state values mean over the wire

`/find`, `/tree`, `/tap` handlers that observe a suppressed bundle
emit an envelope carrying `visible: "unknown"` on the top row.
Descendants also come back as `unknown` because no query hits
XCUITest at all — the tree resolver returns a synthetic
"here's what we can say about the process, nothing about children"
snapshot.

Read as: "the runner has proof the target was recently dead, and it's
been less than 20 s. It might be alive again, but we're not going to
find out by hammering XCUITest."

## When the invariant fires unhelpfully

Two known failure modes:

1. **The app is alive but its a11y hierarchy is sparse.** Common for
   pre-connected React Native app screens where the JS bundle hasn't
   finished loading and only the root native view has a11y annotations.
   Also for Expo dev-launcher server-picker screens (insight case:
   `com.focusai.app.mobile` picker on SDK 57): the picker lives inside
   the target's process, but its subviews use no `accessibilityIdentifier`.

   From the cache's perspective the app IS running (`.state == .runningForeground`)
   and the re-probe never fires because there's nothing to fire on —
   the cache was never engaged. But the tree still comes back `unknown`
   because the actual a11y query has no annotated children.

2. **The app repeatedly crashes AND restarts during the 20 s window.**
   The cache marks dead, the re-probe sees running, calls markAlive,
   the caller retries, XCUITest raises the issue again, cycle. This
   is the healthiest use of the cache — the retry-spam is broken by
   requiring 3 s per re-probe iteration rather than 300 ms per raw
   XCUITest call — but does prolong the visible failure.

## Distinguishing the two on a `.aliveCache` counter dump

`/diagnostic/dump` returns `aliveCache: { wired: true, markDeadTotal, markAliveTotal, reprobeAttemptedTotal, reprobeSucceededTotal, reprobeInvalidatedEarly, reprobeExhaustedWindow, suppressHitTotal, suppressMissTotal }`.

- `markDeadTotal == 0` + `suppressHitTotal == 0` — **cache never engaged**.
  Your `unknown` state is NOT the cache — it's genuine a11y sparsity
  (case 1). Look at your app's a11y annotation coverage, or the
  scaffolding (dev-launcher / splash / gate screen) obscuring the
  real UI.
- `markDeadTotal > 0` + `reprobeSucceededTotal < markDeadTotal` —
  the app is going down but the re-probe isn't seeing it come back.
  Either your app's cold-launch takes longer than the 18 s
  re-probe window, or it's crashing repeatedly (case 2).
- `markDeadTotal > 0` + `reprobeExhaustedWindow > 0` — the cache
  gave up on at least one re-probe cycle. Combined with a repeating
  failure, this is case 2 in its worst form. Consumer-side action:
  slow down the retry loop; runner-side action: extend `ttlMs` or add
  a second, longer window.

## What v1.0.11 changed

- Cache observability was already in v1.0.10 §D5, but `aliveCache`
  was omitted from `/diagnostic/dump` when nil — insight's v1.0.10
  followup reported `aliveCache: null` and couldn't tell "runner has
  no cache" from "cache never fired." v1.0.11 §D1 makes emission
  unconditional with a `wired: bool` sentinel.
- Cumulative session lifecycle counters landed alongside so
  `terminateAppViaFallback > 0` can flag the SIGKILL fallback path
  as the root cause of `bug_type: 309` `.ips` writes (case 3 —
  covered separately in the v1.0.11 RFC, not in this doc).

## What this doc is not

- Not a promise about future behaviour. The cache invariants are
  runner-scoped and can change per release (they will in v2 when we
  rework `test_runForever` around a proper per-request task-local
  scope). Read the current `AppAliveCache.swift` for authoritative
  semantics.
- Not a design proposal. The design has been in the tree since
  v1.0.4 §D2; this doc explains what's there.

## Related

- `.claude/rfcs/1.0.4-sim-health-and-backpressure.md` — original
  `AppAliveCache` design.
- `.claude/rfcs/1.0.11-launch-lifecycle-and-observability-under-load.md`
  — v1.0.11 RFC that closed the observability gap.
- `Sources/SmixRunnerCore/SmixRunnerServer.swift::AppAliveCache` —
  source of truth for the actor's semantics.
- `SmixRunnerUITests/SmixRunnerUITests.swift::record(_:)` — where the
  cache is fed from XCTIssue observations.
