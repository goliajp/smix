# Response — accepting the systemic pause

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-systemic-pause.md`

## Accepting the ask

You're right. I stop shipping.

Six consecutive patch releases each closed the surface symptom they named and each left the deeper symptom (`waitFor(id) → tree "unknown"`) untouched because I never once ran your bootstrap corpus. Every "green" I claimed in a shipping doc was `cargo test --workspace --lib` green + `xcodebuild build-for-testing` compile green — neither exercises the wire that fails.

That's not a discipline gap in the specific fixes. It's a discipline gap in what I call "shipped". I'm changing that discipline before I write another patch.

## Specific mistakes I own

Not general apology — the specific things you can hold me to:

- **v1.0.8 `clearAppData` was ship-broken from birth.** The verb hard-requires a session id (via `App::clear_app_data → App::http_runner_client → session_id`). `/session/open` was returning 404 100% of the time in your workload (7/7 in a 45-second batch). I could have caught this by running the yaml I documented as the migration path. I didn't. The `insight-v1.0.8-shipping.md` "Migration diff" is misleading and the corresponding CHANGELOG entry oversells the fix.
- **v1.0.9 §D4 shipped without an observable counter.** I added a stderr log line (`smix-runner: app-alive cache re-probe hit …`). You correctly grepped and found zero hits. I have no way to tell you if that means "code path never triggered", "trigger fired but log emit is behind a build flag", or "condition matched but the log write got flushed after a runner cycle". Whichever it is, that's on me for not shipping an observable counter alongside the fix.
- **`/session/open` 404 has been reported since v1.0.5.** In your v1.0.7 followup you said "`fallback message '/session/open failed 404' only shows up ONCE on the second flow after the cascade of interrupts + fallback path — not on every request"`. You were reporting a chronic issue as intermittent; I heard "intermittent" and moved on. Now it's 100% of requests. I own that I never chased the root cause of the 404 across those releases; I trusted the fallback path was doing its job. It clearly wasn't — the fallback path degrades silently.
- **Diagnostic ring buffer is empty when it matters.** You reported v1.0.7's dump was `[]` because supervisor cycles reset the runner in-memory state. My v1.0.7 shipping doc says "wire this into your gate driver's failure path". That advice is basically useless if the payload is empty during real failures. I should have persisted the ring on cycle.
- **Six releases, one visible symptom.** Cumulatively the pattern is that I fix a proximate cause without walking upstream to the systemic one. That's the exact thing you told me not to do on 2026-07-11 ("反馈得到的是点，我们要变成体系"). I said that lesson landed with v1.0.7; it obviously didn't.

## New process — the one thing that has to change

Before another release, I run your bootstrap corpus end-to-end and observe it passing. Not a smoke; the full 3-flow bootstrap scope you've described. If a fix I designed doesn't survive that observation, it doesn't get a version number.

Concretely I'm accepting your offers, both:

1. **Corpus PR.** Please open the PR you offered — `crates/smix-cli/tests/fixtures/insight-bootstrap-corpus/` (or equivalent path — I'll defer to whatever integrates cleanly). ~200 lines of yaml + a way to run them against a booted sim. I'll wire it into `scripts/release/` as a hard gate on ship.
2. **Docker seed image.** If you have bandwidth to ship `smix-insight-seed:test`, that becomes my acceptance target. The gate is: sim booted with that image + the corpus yaml runs → 0 dialogs, 0 `element not found`, 0 fallback WARNs, 0 new `.ips` in DiagnosticReports.

For point 7 (Full-corpus real-sim stress harness planned for v1.1 in the roadmap): I bring the WIP to you before I flip it on. You wire it into your sim; I read the raw log; we iterate on the actual failure modes before I add anything to smix.

## Concrete plan (not commitments — a plan you can push back on)

I don't promise dates. But here's the sequence I intend, in order:

### Phase A — Root-cause the 404 permanently

Before touching anything else, I sit with the `/session/open` failure. The three plausible root causes I have so far:

- **Race between test_runForever init and first POST.** `sessionTable` is initialized inside the `test_runForever()` method; the runner reports healthy on `/health` before that init finishes. First `/session/open` beats it. If the route reads a nil / missing table it 404s.
- **Route not registered in some xcodebuild lifecycle window.** `runForever(sessionHandlers: SessionHandlers?)` takes optional handlers. If a code path elsewhere calls `runForever` without them, no `/session/open` route exists.
- **Session table lost on XCTest re-entry.** The v1.0.5 persistence file I added writes `~/Documents/smix-sessions.json`, but if the runner's XCUIApplication home directory changed between the write and the next boot the file might silently not-exist.

Once I know which one it is, I fix it. Not "cache better" or "reduce frequency" — actually make the 200 the deterministic outcome. If the failure is real (runner genuinely can't open a session), then the 200 is the wrong answer and I return a 5xx with an actionable message; either way silent 404 → legacy fallback stops.

### Phase B — Instrumentation counters on every observability feature

Every observability landing gets a counter. `/diagnostic/dump` gains fields:

```
{
  "aliveCache": {
    "markDeadTotal": N,
    "reprobeAttempted": N,
    "reprobeInvalidatedEarly": N,
    "reprobeExhaustedWindow": N
  },
  "sessions": {
    "openTotal": N,
    "openFailedTotal": N,
    "openFallbackTotal": N
  },
  ...
}
```

So "is it working" becomes a diff on the counter, not a grep on log lines that may or may not survive a runner cycle.

Ring buffer persists to `~/Documents/smix-diagnostic-ring.json` on every mutation (piggyback on the v1.0.5 session-persistence pattern). Post-mortem tools read the file, not the in-memory state.

### Phase C — Decide the `clearState: true` question

The right thing is your option (a): once `/session/open` is fixed, auto-expand `launchApp: { clearState: true }` internally to the fixed code path. Consumers see zero yaml change; consumers on old smix see zero behaviour change. The legacy shape becomes a shape that resolves at runtime — no deprecation dance you have to coordinate on.

I was leaving you two half-working paths waiting for you to migrate. That was wrong. I take that decision.

### Phase D — Run your corpus. Iterate. Ship one release.

Once phases A-C are landed on `main` and pass unit tests, I run the corpus on a real sim booted to your seed. Every failure is a blocker for shipping. When the whole batch runs green + 0 dialogs + 0 fallback WARNs + `.ips` count unchanged, I bundle everything into one release. Timeline: whatever this takes.

## What I'm asking of you

- **Open the corpus PR at your convenience.** No hurry — I'd rather wait for the corpus + seed than start guessing about which flow shape to target. If the Docker seed image takes a week to package, take the week; the alternative is me guessing wrong again.
- **Keep running v1.0.9 with the legacy shape** if that's the least-bad current state. I know you get the dialog on every run; that's the honest current state of what I've shipped. I'm not going to ship a v1.0.10 to paper over it.

## What I'm NOT doing

- No v1.0.10 emergency patch. No "one more fix and this batch works."
- No `.claude/rfcs/1.0.10-*.md` for another iteration cycle. When I write the next RFC it's the release RFC for the one bundled fix.
- No unilateral decision on the shape of the corpus integration; I let you drive the PR shape.
- No promise it's fast. If it takes a month, it takes a month.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-2026-07-11-systemic-pause-response.md
```

Prior chain (chronological, for anyone reading in future):

- `smix-feedback-2026-07-10-gate-hardening.md` — 8 findings A-H from v1.0.3
- `smix-feedback-2026-07-11-v1.0.5-followup.md` — 3-item ask
- `smix-feedback-2026-07-11-blocking-crash-dialog.md` — hard-requirement escalation on finding H
- `smix-feedback-2026-07-11-systemic-pause.md` — the pause + one-release ask you sent me today
- **this doc** — my accepting the ask, the specific mistakes I'm owning, and the plan
