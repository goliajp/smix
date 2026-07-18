# smix v1.0.10 — shipping notes for insight (systemic pause closure)

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-systemic-pause.md`
RFC: `.claude/rfcs/1.0.10-runner-source-sync-and-observability.md`

## TL;DR — one release, seven charter items, ship gate observed on real sim

v1.0.10 lives on crates.io + npm + Maven Central + `swift-v1.0.10` git tag as of today. It closes the meta-root-cause of the v1.0.4→v1.0.9 same-symptom cycle you called out in the systemic-pause feedback.

Meta-root-cause confirmed with hard evidence: `cargo install smix` used to ship only the Rust binary. The Swift `SmixRunner.xcodeproj` + `Sources/SmixRunnerCore/` + `SmixRunnerUITests/` that xcodebuild actually compiles were obtained separately at consumer install time and never re-synced against later CLI upgrades. Your on-disk `~/.local/share/smix/runner/SmixRunnerUITests/SmixRunnerUITests.swift` was 2212 lines with **zero references** to `sessionHandlers` / `/session/open` / `SessionHandlers` while the v1.0.9 repo file was 2669 lines with them present. Six CLI patch releases (v1.0.4-v1.0.9) shipped session lifecycle + observability + crash-dialog + a11y-cache re-probe fixes, but insight's runner-side Swift code stayed frozen at pre-v1.0.3. `/session/open` returning 404 was a distribution gap masquerading as a route bug.

## How you upgrade (single command; the rest is automatic)

If you're on the CLI via cargo:

```bash
cargo install smix                     # picks up 1.0.10 from crates.io
smix --version                         # → smix 1.0.10
```

If you shipped a wrapper CLI symlink at `~/.local/bin/smix` (as we discussed during v6.12's `simx → smix` migration), overwrite it after the cargo install:

```bash
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                         # confirm 1.0.10 on PATH
```

**That's it.** The next `smix runner up` auto-syncs the on-disk runner sources for you:

```bash
$ smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle jp.golia.qualcomm.insight
smix-runner: synced runner sources → 1.0.10 (was <none>) at /Users/…/.local/share/smix/runner
runner starting: … COLD REBUILD expected up to 10 minutes …
runner up: http://localhost:22087/health = 200 (runner v1.0.10)
```

Under the hood: `resolve_runner_project` reads `~/.local/share/smix/runner/.smix-runner-version`. On drift or missing (which is 100% of consumers today, since that file didn't exist before v1.0.10), it moves your existing `~/.local/share/smix/runner/` tree aside to `runner.bak-<ts>/` and extracts the CLI-embedded v1.0.10 tarball in place. Your pre-v1.0.10 `SmixCoreFFI.xcframework/` (the 13 MB binary target) is preserved across the swap — the auto-sync carries it from the backup into the new tree before invoking xcodebuild. Zero manual migration.

If you want to sync explicitly (e.g., you `--force`d the version file to test a rollback path, or the auto-sync path is disabled by an explicit `$SMIX_RUNNER_PROJECT`):

```bash
smix runner install [--force] [--path <dir>]
```

Idempotent when already current; `--force` re-extracts and takes a fresh backup even when the version already matches.

## What you can observe end-to-end that closes your feedback points

Every item in `smix-feedback-2026-07-11-systemic-pause.md` maps to a specific charter item you can grep for after upgrade. Run this once you've cold-rebuilt:

### 1. `/session/open` returns 200, not 404

Was your #2. On this repro (com.apple.Preferences target, `sim-insight`, iOS 26.5, booted at UDID `FFC57DAE-…`) we observed:

```bash
$ curl -si -X POST -H "Content-Type: application/json" \
    http://127.0.0.1:22087/session/open \
    -d '{"bundleId":"com.apple.Preferences","activate":false}'
HTTP/1.1 200 OK
Content-Type: application/json
X-Sim-Health: healthy

{"sessionId":"6F7C4A73-AEA0-4C3C-8F5D-6FF9DB9E5873","activatedOnce":false,"serverTimeMs":1783746973931}
```

If your bootstrap now shows any `session/open failed 404` line, please grep the runner log for the `smix-runner: synced runner sources → 1.0.10` banner. Absence of the banner is diagnostic — it means auto-sync didn't run and your runner is still compiled from stale sources. In that case: `smix runner install --force` will re-extract unconditionally.

Consequence: `clearAppData` yaml verb (v1.0.8) — the one that failed with `clear_app_data: no session id on the client` in your v1.0.9 followup — now has a real session id to work with. Please re-run the migrated `.devtools/qa/sim/subflows/launch-fresh.yaml`.

### 2. `/health` reports the runner's actual version

Was your #6 (documented observable state). The `runnerVersion` field CHANGELOG v1.0.2 claimed but never emitted is now real:

```bash
$ curl -s http://127.0.0.1:22087/health
{"ok":true,"runnerVersion":"1.0.10","uptimeMs":16105,"lastRequestAtMs":0,"sessionsOpen":0,"activationsTotal":0}
```

The CLI's `smix runner up` reads this after xcodebuild's `/health` first responds:

- `runnerVersion` matches CLI version → prints `runner up: … (runner v1.0.10)` and proceeds.
- `runnerVersion` differs from CLI version → refuses boot with:
  ```
  runner version mismatch: CLI is v1.0.10 but the running SmixRunner reports v1.0.9. …
  Fix: `smix runner install --force` to re-extract the embedded runner sources, then retry `smix runner up`.
  ```
- `/health` returned the legacy `{"ok":true}` body (pre-v1.0.10 runner someone built out-of-band) → warns but does not refuse, so pre-v1.0.10 setups keep working during their own upgrade window.

### 3. a11y-cache re-probe fires with observable counters

Was your #3 (grep-can't-tell problem). v1.0.9 §D4 shipped the re-probe with only a stderr log line to prove it. v1.0.10 wires counters through `/diagnostic/dump`:

```bash
$ smix diagnostic dump --json | jq '.aliveCache'
{
  "markDeadTotal": 0,
  "markAliveTotal": 1,
  "suppressHitTotal": 0,
  "suppressMissTotal": 0,
  "reprobeAttemptedTotal": 0,
  "reprobeSucceededTotal": 0,
  "reprobeInvalidatedEarly": 0,
  "reprobeExhaustedWindow": 0
}
```

The four `reprobe*` fields advance from the actual v1.0.9 §D4 background Task:

- `reprobeAttemptedTotal` — the observer spawned a re-probe (an `XCTIssue "Application X is not running"` fired)
- `reprobeSucceededTotal` — a probe iteration observed the app return to running and called `markAlive`
- `reprobeInvalidatedEarly` — someone else (e.g., a fresh `/session/open`) called `markAlive` mid-loop
- `reprobeExhaustedWindow` — all 6 iterations ran and the app was still `.notRunning`

"Is v1.0.9 §D4 fixing me?" becomes a numeric check on a counter diff instead of a grep for a log line that may or may not survive a supervisor cycle.

### 4. `/diagnostic/dump` payload survives supervisor cycles

Was your #7. The subprocess ring writes-through to `~/.local/share/smix/subprocess-ring.json` on every mutation. Cycles no longer wipe the observation surface — post-mortem tools read the file after teardown, not the (now-gone) in-memory state.

### 5. `clearAppData` verb — no more `no session id` runtime error

Was your #4. Because #1 above is fixed, `App::clear_app_data` reaches `runner_client.session_id().ok_or(…)` with a real session id. The three-step orchestration (`terminate-app` → host simctl sandbox wipe → `launch-app`) now runs end-to-end.

### 6. `launchApp: clearState: true` decision

Was your #5. We're keeping the option-(a) plan (auto-expand `clearState: true` internally to the fixed `clearAppData` path). That auto-expand ships in v1.0.11 once we've observed v1.0.10 running your bootstrap corpus green — see next section.

### 7. Corpus release gate scaffolded

Was your #1. `scripts/release/corpus-gate.sh` is landed. Point it at your `.devtools/qa/sim/subflows/` via `SMIX_CORPUS_DIR` and it runs each yaml against a booted sim, refusing the release on any FAIL. When you have bandwidth to open the corpus PR against `crates/smix-cli/tests/fixtures/insight-bootstrap-corpus/`, we wire it as a hard pre-publish gate.

## What we still need from you (not blocking; explicit)

The one thing v1.0.10 ships *without* is your actual `bun test:e2e` bootstrap batch running green end-to-end. Your app wasn't installed on the sim I used for the ship gate — I validated the systemic fix against `com.apple.Preferences` to isolate route registration from app install. If you can:

- Cold rebuild against v1.0.10 (`rm -rf .smix/runner/derived-data-*`)
- Run the migrated `launchApp: { clearAppData, launchApp: {} }` yaml
- Confirm zero `POST /session/open` 404 lines, zero new ReportCrash `.ips`, and `aliveCache.reprobeAttemptedTotal` non-zero if a slow-bootstrap app in your corpus triggered the re-probe path

… that observation closes the v1.0.9 followup batch you sent us. If any of the three regresses, please file feedback under the same feedback path template — the counters + subprocess ring persistence should make the diagnosis a lot cheaper than v1.0.9 was.

## Swift SDK (`goliajp/smix` Swift Package) — bumped in lockstep

Insight's e2e stack does not currently import `SmixSDK` in a native iOS test target — you drive smix from the RN/TypeScript layer through the CLI, so the SwiftPM consumer path below is informational, not required. If that ever changes (native Swift XCTest target driving smix directly), the upgrade is:

**Xcode → Package Dependencies → smix → Update Package** (or `File > Packages > Update to Latest Package Versions`).

If you pin the version, bump it in `Package.swift`:

```swift
// swift-tools-version:5.9
dependencies: [
  .package(
    url: "https://github.com/goliajp/smix.git",
    from: "1.0.10"                      // was "1.0.9"
  ),
],
targets: [
  .testTarget(
    name: "MyAppUITests",
    dependencies: [
      .product(name: "SmixSDK", package: "smix"),
    ]
  ),
]
```

`SmixSDK` version tracks the workspace (1.0.10 across every ecosystem). No API changes from v1.0.9 — the additive changes (Session state / relaunch-app / stillValid) all landed in v1.0.4.

The one advantage of the Swift SDK path for future you: `Session.state: SessionState` (the reactive `AsyncStream<SessionState>` exposed by v1.0.4) is a first-class Swift concurrency stream, so a native-Swift test target gets `for await state in session.stateStream { … }` for free — no bridging through the RN-side `session.on('state', …)` event listener.

## What did *not* change from v1.0.9

- Wire compatibility on every non-diagnostic route.
- ABI (session id opaqueness, `App-Bundle-Id` / `Session-Id` / `Input-Dispatch-Mode` header contract).
- `simctl` invocation patterns and env forwarding.
- `.smix/runner/state.json` schema.
- `bun test:e2e` invocation shape.

`HealthResponse` is stable but now non-zero. `DiagnosticSnapshot` gains optional `aliveCache` object; consumers ignoring unknown fields work unchanged.

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-*.md
```

For v1.0.10-related feedback, please include the output of these three commands so we can rule out "you're still on stale sources" upfront:

```bash
smix --version                                       # → smix 1.0.10
cat ~/.local/share/smix/runner/.smix-runner-version  # → 1.0.10
curl -s http://127.0.0.1:22087/health | jq .runnerVersion  # → "1.0.10"
```

If any of those three don't say 1.0.10, the fix is `smix runner install --force` (worst case `smix runner down && rm -rf ~/.local/share/smix/runner && smix runner install`).

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.10-shipping.md
```

Prior chain (chronological, for the arc):

- `smix-feedback-2026-07-10-gate-hardening.md` — 8 findings A-H from v1.0.3 baseline
- `smix-feedback-2026-07-11-v1.0.5-followup.md` — 3-item ask (items A + B + D.3)
- `smix-feedback-2026-07-11-blocking-crash-dialog.md` — hard-requirement escalation
- `smix-feedback-2026-07-11-systemic-pause.md` — systemic pause + one-release ask
- `insight-v1.0.7-shipping.md` — v1.0.7 observability layer + argv-in-errors
- `insight-v1.0.8-shipping.md` — v1.0.8 clearAppData design (unusable in your workload due to /session/open 404 — this doc closes that)
- **this doc** — v1.0.10 systemic distribution fix
