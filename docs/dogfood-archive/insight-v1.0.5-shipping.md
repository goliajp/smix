# smix v1.0.5 — shipping notes for insight (upgrade path + what to do)

Date: 2026-07-11 (same day as v1.0.4)
From: smix maintainer (`claude@golia.jp`)
Responding to:
- Prior: `docs/ai-guide/insight-v1.0.4-shipping.md` (same-directory)
- Prior: `docs/ai-guide/insight-v1.0.3-studio-crash-2026-07-10.md`
- RFC: `.claude/rfcs/1.0.5-supervisor-and-persistence.md` (private planning)

## TL;DR

- **Upgrade to v1.0.5 across the board.** Same-day follow-up to v1.0.4; closes the three deferrals we called out in the v1.0.4 shipping notes.
- **You can now run batch e2e gates on `studio` again.** v1.0.4 already stopped the SimRenderServer crash; v1.0.5 adds session persistence + supervisor so a mid-gate `TEST INTERRUPTED` no longer wedges the next flow.
- **Drop your `smix runner down && smix runner up` between-flow workaround** if you added one. Use `smix runner supervise` in a sidecar instead — it auto-cycles on interrupt without losing session ids.
- **First run should be single-flow smoke, then batch.** No known bugs, but this is your first run against a fully-shipped v1.0.5; a 1-flow smoke inside a real gate is the responsible move before turning batch back on.

## What changed in v1.0.5

Three follow-up items from v1.0.4:

| Item | v1.0.4 status | v1.0.5 status |
|---|---|---|
| §E ask 2 — session persistence across XCTest lifecycle | deferred | ✅ landed (D1) |
| §E ask 1 / §D6 — host-side XCTest supervisor daemon | manual only via `smix runner cycle` | ✅ landed automatic via `smix runner supervise` (D2) |
| Runner idle-close 120 s → 60 s | deferred | ✅ landed (D3) |

Plus infrastructure:

- **Real-sim smoke gate script** — `scripts/release/smoke-v1.smoke.sh` + a DAG-ordered `ship.sh` publisher. Not something insight needs to invoke, but it's the reason future ships can be trusted more than v1.0.4's (which shipped on build-green only).

## How to upgrade

### The four ecosystems

```bash
# 1. CLI + Rust SDK
cargo install smix --force                       # or: cargo install smix@1.0.5
cargo add smix-sdk@1.0.5                          # if you use the Rust SDK directly

# 2. TypeScript SDK
bun add @goliapkg/smix@1.0.5                      # or npm i @goliapkg/smix@1.0.5

# 3. Swift Package (if you use SmixSDK inside an insight companion project)
# In Package.swift:
.package(url: "https://github.com/goliajp/smix.git", .exact("1.0.5"))

# 4. Kotlin SDK (Android instrumentation gate — not currently active on insight)
# In build.gradle.kts:
implementation("jp.golia.smix:smix-sdk:1.0.5")
```

### Verify

```bash
smix --version                                    # should print `smix 1.0.5`
bun pm ls | grep '@goliapkg/smix'                # should print @goliapkg/smix@1.0.5
```

## What insight can start using immediately

### 1. `smix runner supervise` — auto-recovery for `TEST INTERRUPTED`

The scenario feedback §E called out: `bun test:e2e` bootstrap runs step 20-31, XCTest test-host gets `** TEST INTERRUPTED **`, subsequent `smix run` fail with `/session/open` returning 404 or `unknown` a11y trees. With v1.0.5, run the supervisor in a sidecar terminal:

```bash
# in a second terminal, after `smix runner up` succeeds:
smix runner supervise
```

Behavior:

- Foreground process; tails `.smix/runner/runner-<UDID>.log`.
- On `** TEST INTERRUPTED **` or `SchemeActionResultOperation started unexpectedly`: invokes `smix runner cycle` (warm re-up, ~3 s).
- Consumer `Session-Id` survives the cycle (thanks to §D1 persistence).
- Backoff: 60 s cooldown between cycles; 5-in-10-min circuit breaker → exits non-zero so your monitoring layer escalates.
- Emits `{"event":"RunnerCycled","reasonMatched":"...","atMs":N}` JSON on stdout — pipe to your CI's log aggregator to correlate cycle events with gate failures.

Integration shape for your `bun test:e2e` bootstrap scope:

```typescript
// .devtools/qa/sim/runner.ts (illustrative)
const supervisor = spawn(SMIX_BIN, ['runner', 'supervise'], {
  stdio: ['ignore', 'pipe', 'inherit'],
  detached: false,
})
supervisor.stdout.on('data', (buf) => {
  try {
    const evt = JSON.parse(buf.toString())
    if (evt.event === 'RunnerCycled') {
      logger.warn('smix supervisor cycled runner', evt)
    }
  } catch { /* non-JSON line */ }
})
try {
  await runBootstrapFlows()
} finally {
  supervisor.kill('SIGTERM')
}
```

If you don't want to run a sidecar, you can still invoke `smix runner cycle` manually between flows — but the supervisor is strictly better because it fires the moment the log emits the trigger, not after the current flow finishes timing out.

### 2. Session persistence — you can trust your `Session-Id` across cycles

Every `smix run` invocation opens its own session (session-per-flow), so this is mostly a background improvement. But if any of your gate code retains a `Session-Id` across multiple smix invocations, that id now survives:

- Runner-side test-host restart (via `smix runner cycle` or supervisor auto-cycle).
- SIGKILL-orphaned sessions (client dies without close) idle-reaped within 60-75 s — no more accumulating over long CI days.

### 3. `Session::still_valid()` — decide keep-or-reopen after state transitions

If your Node/TS side wraps smix and holds a session across steps, wire this after any transition to `cycling`/`dead`:

```typescript
session.on('state', async (state) => {
  if (state === 'cycling' || state === 'dead') {
    if (await session.stillValid()) {
      logger.info('session survived cycle')
    } else {
      session = await Session.open(runtime, appId, { activate: true })
    }
  }
})
```

The runner exposes `POST /session/list` — SDK method `session.stillValid()` on all four SDKs (Rust `still_valid()`, TS `stillValid()`, Swift `stillValid()`, Kotlin `stillValid()`).

### 4. `smix runner list-sessions` — diagnostics for CI operators

Print every session currently known to the runner:

```bash
$ smix runner list-sessions
sessionId                              bundleId                                 openedAtMs        lastActivatedAtMs
9F3E8B1A-...                           com.focusai.app.mobile                   1720689000000     1720689042000
...
```

Useful when a gate fails and you want to know whether the runner still knows the session your yaml opened, or whether it got reaped.

## What insight can now delete

If any of these are in your gate code, they're workarounds for problems v1.0.5 solved:

- **`smix runner down && smix runner up` between flows.** Use `smix runner supervise` instead. Warm cycle takes ~3 s and preserves session ids.
- **Any long sleep after `smix runner up` to "let sessions stabilize".** Persistence makes it deterministic.
- **Manual session id reset when catching a "unknown session id" error.** Use `session.stillValid()` to decide, or let the state event fire.
- **`waitForNoSessions` / accumulating-session detection.** The idle-close sweep reaps abandoned sessions within 60-75 s automatically.

## Runbook — first run after upgrading

Recommended sequence:

1. **Upgrade + verify version.**
   ```bash
   cargo install smix --force
   smix --version                                # smix 1.0.5
   ```

2. **Single-flow smoke first.**
   ```bash
   smix runner up sim-insight --bundle com.focusai.app.mobile
   smix run .devtools/verify/visual/hub-form.yaml \
     --activate --bundle-id com.focusai.app.mobile \
     --metro-log-url file:///tmp/metro.log
   smix runner down
   ```
   If this passes, batch mode is safe to turn back on.

3. **Batch bootstrap with supervisor.**
   Add the supervisor sidecar to your `bun test:e2e` bootstrap driver (see integration shape above); then run the full scope. Watch stdout for `RunnerCycled` events.

4. **Studio safety check.** Repeat the diagnostics from `insight-v1.0.3-studio-crash-2026-07-10.md` after your first batch:
   ```bash
   ls -lt ~/Library/Logs/DiagnosticReports/ | grep -iE "SimRenderServer|shutdown_stall"
   ```
   Expect: no new SimRenderServer crashes; no shutdown_stall from your batch window. If you see either, capture the timestamps + share — that's a v1.0.6 blocker.

## Deferred to v1.0.6+ (not on the critical path)

- **Sidecar auto-supervise via `smix runner up --supervise`.** Currently manual sidecar. If you'd like automatic start (spawned detached from `runner up`), file a request and it moves up.
- **Non-xcodebuild XCUITest restart (~500 ms cycle).** v1.1 material — current warm cycle is ~3 s which is fine for the interrupt-triggered path; matters more when we want cycle-per-flow for isolation.
- **Full-corpus real-sim stress harness.** The smoke gate is a smoke, not a stress; v1.1 will add a 20-flow bootstrap corpus running nightly against `sim-smoke`.

Everything else on your feedback backlog is closed. See `docs/roadmap.md` for the v1.1 preview.

## Escalation path

- **Any new SimRenderServer crash or shutdown_stall on studio during a v1.0.5 batch.** Capture DiagnosticReports timestamps + share. Emergency v1.0.6 or v1.0.5 yank.
- **`smix runner supervise` misbehaves** (cycles too aggressively, misses obvious `TEST INTERRUPTED`, or the 5-in-10-min circuit breaker fires spuriously). Share the supervisor's stdout log + a snippet of the runner-log lines that triggered — I'll tune the patterns / cooldowns.
- **Session persistence file corruption.** `~/Documents/smix-sessions.json` inside the sim is written atomic-rename; if it ever fails to decode on boot, the runner logs `sessions rehydrate failed: <err>` on stderr and starts empty (safe). Share the runner log for me to reproduce.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.5-shipping.md
```

Prior insight-facing docs in the same directory:

- `insight-v1.0.4-shipping.md` — v1.0.4 shipping notes with the 9-item feedback closure.
- `insight-v1.0.3-studio-crash-2026-07-10.md` — SimRenderServer forensic + urgent stop-gap.
- `insight-v1.0.3-session-lifecycle.md` — v1.0.3 session lifecycle intro (still relevant; v1.0.5 adds `stillValid`).
- `insight-feedback-gol-611-response.md` — original gol-611 arc.
