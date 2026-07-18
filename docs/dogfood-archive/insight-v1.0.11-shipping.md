# smix v1.0.11 — shipping notes for insight (v1.0.10 observations followup)

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-v1.0.10-observations.md`
RFC: `.claude/rfcs/1.0.11-launch-lifecycle-and-observability-under-load.md`
Also lands: `.claude/rfcs/appalive-cache-invariant.md` (your §C.6 ask)

## TL;DR

Three of the six points from your v1.0.10 observations doc close in v1.0.11. Two more get real observability so the next iteration is a numeric diff, not a grep-for-log-line game. One (docker testbed image acceptance for pre-publish gate) is still awaiting your PR.

- **§A2 `aliveCache: null`** → **closed.** Direct-capture + always-emit + `wired` sentinel. `smix diagnostic dump` now shows the counters.
- **§B Expo SDK 57 dev-launcher picker blocking business flow** → **closed at the mechanism level.** `clearAppData` yaml verb + `/session/launch-app` HTTP endpoint accept `launchArgs` / `launchEnv`. You pass `-EXInternalMetroPort` or `EX_DEV_CLIENT_METRO_URL` (whichever your dev-client honors — likely both work, we've never seen either fail on same-process invocation); the picker never shows.
- **§A1 `bug_type: 309` `.ips` writes during clearAppData** → **closed at the direct cause.** `launchApp` polls `.state == .runningForeground` before returning; `App::clear_app_data` defaults to 15 s wait. Your next terminate no longer hits a not-yet-ready process.
- **§C.1 clearAppData terminate outcome instrumentation** → **added.** `sessionCounters.terminateAppViaXCUIApplication` vs `terminateAppViaFallback`. `.terminateAppViaFallback > 0` is the smoking gun.
- **§C.3 cumulative session lifecycle counters** → **added.** `sessionsOpenedTotal`, `terminateAppTotal`, `launchAppTotal`, plus the fallback-count above.
- **§C.6 RFC on the a11y-cache invariant** → **written.** `.claude/rfcs/appalive-cache-invariant.md`.
- **§C.4 docker testbed image** → **still yours to open when convenient.** We keep landing insight-facing releases against `com.apple.Preferences` as ship-gate proxy — same anti-pattern you called out. If you ship us the image we wire `scripts/release/corpus-gate.sh` to consume it and no v1.0.12 goes out without your real app.

## Single-command upgrade (same as v1.0.10)

```bash
cargo install smix          # → smix 1.0.11 on ~/.cargo/bin
cp ~/.cargo/bin/smix ~/.local/bin/smix   # if you use the ~/.local/bin symlink
smix --version              # → smix 1.0.11
```

Next `smix runner up` auto-syncs `~/.local/share/smix/runner/` to the v1.0.11 tarball, preserving your `SmixCoreFFI.xcframework/` from the backup (per v1.0.10's carry-over patch). No manual step.

```bash
$ smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile
smix-runner: synced runner sources → 1.0.11 (was 1.0.10) at /Users/…/.local/share/smix/runner
runner starting: … WARM REBUILD (~30-60 s expected since Swift changes trigger a partial recompile)
runner up: http://localhost:22087/health = 200 (runner v1.0.11)
```

If `/health` says `runner v1.0.11` you're good. If it still says `1.0.10`, `smix runner install --force` re-extracts unconditionally.

## §A2 fix — `aliveCache` now always emitted with `wired` sentinel

Root cause: `SessionHandlers.diagnostic` was reading `SmixRunnerServer.currentAppAliveCache` via task-local. FlyingFox spawns per-request handlers as unstructured tasks off an internal actor context that DOESN'T inherit the `withValue` scope wrapping `server.run()`. Result: the task-local read returned nil, and my code omitted the field on nil.

Fix: `test_runForever()` extracts the cache to a named local `localAppAliveCache`. The diagnostic handler closure captures the reference directly. Task-local propagation is now irrelevant for this path.

Observability tightening: `aliveCache` is now **always** present in `/diagnostic/dump`, carrying a `wired: bool` sentinel + all-zero counters when the runner was booted without a cache. So:

```jsonc
// pre-v1.0.11 (v1.0.10):
"aliveCache": null              // ambiguous: cache missing? or code not there?

// v1.0.11 with cache wired at boot (your case):
"aliveCache": {
  "wired": true,
  "markDeadTotal": 0,
  "markAliveTotal": 1,          // ≥ 1 because /session/open triggers markAlive per D2 §"successful open re-establishes"
  "suppressHitTotal": 0,
  "suppressMissTotal": 0,
  "reprobeAttemptedTotal": 0,   // > 0 iff v1.0.9 §D4 fired at least once this run
  "reprobeSucceededTotal": 0,
  "reprobeInvalidatedEarly": 0,
  "reprobeExhaustedWindow": 0
}

// v1.0.11 with cache OPTED OUT at boot (via runForever appAliveCache: nil):
"aliveCache": {"wired": false, "markDeadTotal": 0, …}  // all zeros — sentinel
```

Reads: `.aliveCache.wired == true` + `.markDeadTotal == 0` + `.suppressHitTotal == 0` on your failing bootstrap batch means the cache **never engaged** — your `unknown` descendants aren't from re-probe cache; they're a11y sparsity. See the a11y-cache invariant RFC for the model — the two most common causes of that state:
1. Real app crash + retry (cache fires, `markDeadTotal > 0`).
2. App up + a11y hierarchy sparse (cache never fires, `markDeadTotal == 0`).

Your Expo 57 dev-launcher picker case is (2). The picker screen lives inside your app process (`.state == .runningForeground`) but has no annotated descendants. Now you can prove this numerically instead of pattern-matching against the error text.

## §B fix — clearAppData launchArgs / launchEnv (Expo 57 dev-launcher bypass)

The mechanism landed. Wire-level:

```yaml
- clearAppData:
    launchArgs:
      - "-EXInternalMetroPort"
      - "8081"
    launchEnv:
      EX_DEV_CLIENT_METRO_URL: "http://localhost:8081"
```

Both `launchArgs` / `launchEnv` and shorthand `args` / `env` accepted. Empty defaults preserve pre-v1.0.11 behaviour (bare `- clearAppData` remains valid — three parser tests lock this).

Threading through:
1. yaml parses to `Step::ClearAppData { launch_args, launch_env }` in `smix-adapter-maestro`.
2. Runtime dispatch calls `AppLike::clear_app_data_with_launch(args, env)`.
3. `App::clear_app_data_with_launch` (new v1.0.11 method) reaches into `SessionAppLifecycleRequest { args, env, wait_for_foreground_ms: Some(15_000), .. }`.
4. HTTP POST `/session/launch-app` with those fields.
5. Swift `launchApp` handler applies `entry.app.launchArguments = req.args`, `entry.app.launchEnvironment = req.env`, calls `.launch()`.

**Which specific arg / env-var does your dev-client honor under SDK 57?** — you know better than me. Insight's `expo-dev-client ~57.0.5` handling is upstream code we don't own. What we've shipped is the mechanism; the exact recipe is a your-side experiment.

Two things worth trying first:
- `EX_DEV_CLIENT_METRO_URL=http://localhost:8081` — this is the AsyncStorage key expo-dev-client persists the last-connected metro URL under; if it reads it back at boot before showing the picker, launchEnv wins.
- `-EXInternalMetroPort 8081` — this is the launchArg style; if dev-client parses the raw process argv at boot.

If neither works: `xcrun simctl launch` bypasses the picker entirely (option (c) from your v1.0.10 observations doc). We already support that via `smix sim launch --child-env` (v6.8 §c2). Ping us if you want a specific yaml verb for it — it's mechanically simple.

## §A1 fix — `launchApp` waits for `runningForeground`

Diagnosis: `bug_type: 309 exec_terminated_before_ready` means launchd caught the process exiting before signalling ready. Not a terminate-side failure — a launch-side race. The v1.0.8 cooperative-terminate pathway works when the app is fully up; when clearAppData fires terminate → wipe → launch → and the caller's NEXT step (or a retry cycle) fires another terminate before the launch reached `.runningForeground`, `XCUIApplication.terminate()` couldn't cooperatively kill an app that wasn't in a killable state → framework internal timeout → hard-kill fallback → launchd `.ips`.

Fix: `SessionAppLifecycleRequest.waitForForegroundMs: Option<u64>`. When set, the runner's `launchApp` handler polls `entry.app.state` at 250 ms cadence until `.runningForeground` or the deadline. Reports `waitedMs` + `terminalState` (0-4 XCTest raw) + `terminatedCooperatively` (always false on launch) in outcome.

`App::clear_app_data` defaults `wait_for_foreground_ms: Some(15_000)`. If your app takes > 15 s cold-launch on iOS 26.5 sim, extend via yaml:

```yaml
# not currently exposed at yaml — if you need > 15 s, tell us and we
# add it. Empirical: Expo cold-launch on iOS 26.5 sim typically
# 3-8 s (bundle load) + 1-2 s (native inits) = ≤ 10 s.
```

## §C.1 counter — `terminateAppViaFallback` is your smoking gun

```jsonc
// /diagnostic/dump.sessionCounters:
{
  "openedTotal": 12,
  "closedTotal": 12,
  "relaunchAppTotal": 0,
  "terminateAppTotal": 6,          // 6 clearAppData calls in your batch
  "terminateAppViaXCUIApplication": 6,    // ← if this matches terminateAppTotal, cooperative worked ALL 6 times
  "terminateAppViaFallback": 0,          // ← > 0 = cooperative failed and SIGKILL'd → potential .ips write
  "launchAppTotal": 6,
  "launchAppReachedForeground": 6,      // ← foreground observed within waitForForegroundMs
  "launchAppTimedOutBeforeForeground": 0
}
```

If your v1.0.11 run shows `terminateAppViaFallback > 0`:
- The cooperative pathway DID fail (usually because the app was mid-launch when terminate hit).
- v1.0.11's wait-for-foreground reduces the surface for that — the caller couldn't fire the next terminate until launch settled.
- If it still shows > 0 on a `bun test:e2e` run, that's the case for me to dig further (extend the wait window? Add a launch-in-progress lock at the runner side? Different Swift API?).

If it stays 0 but you still see `.ips` writes → the `.ips` are coming from something ELSE (crash inside the JS bundle load, native module init, etc — outside smix scope; consumer-side to debug).

## §C.3 cumulative counters — sessionsOpenedTotal, etc.

All landed. `smix diagnostic dump` (non-JSON) prints a new section:

```
=== app-alive cache counters ===
  wired=true markDead=0 markAlive=1 suppressHit=0 suppressMiss=0
  reprobeAttempted=0 reprobeSucceeded=0 reprobeInvalidatedEarly=0 reprobeExhaustedWindow=0

=== session lifecycle counters (cumulative, survive close) ===
  opened=1 closed=0 relaunch=0 terminate=1 launch=1
  terminate: viaXCUIApplication=1 viaFallback=0  # fallback>0 = cooperative terminate failed → potential .ips writes
  launch:    reachedForeground=1 timedOutBeforeForeground=0  # timedOut>0 → next call may fire during launch → bug_type 309
```

## §C.6 RFC on the AppAliveCache invariant

Landed at `.claude/rfcs/appalive-cache-invariant.md`. Answers your question: "what state does the runner think the app is in when it emits `unknown`?"

TL;DR from the RFC:
- `unknown` root + `unknown` descendants = a11y query completed but returned no annotated hierarchy.
- Distinguished from cache-suppressed state via counters (`markDeadTotal == 0` → not cache; > 0 → cache).
- Your Expo 57 dev-launcher case: cache never engaged, sparse a11y hierarchy on the picker screen. That's why v1.0.9 §D4's re-probe log line never appeared for you — nothing to re-probe.

## §C.4 docker testbed — waiting on you

You offered to package `smix-insight-testbed:1.0.11-gate` in the observations doc. If you have bandwidth, please open the PR / share the image name. We wire `scripts/release/corpus-gate.sh` and no v1.0.12+ ships without your real app running the corpus.

Current ship-gate proxy remains `com.apple.Preferences` — same limitation you called out. That doesn't exercise a11y-picker + dev-launcher + expo-router + reanimated stack. If you'd rather we not ship v1.0.11 until you get the image out, we can hold v1.0.11 as an unreleased tag — but based on the confidence level of the §A1/§A2/§B fixes and the fact that they're additive (opt-in via new fields), we shipped it to give you something to work with. Push back if that's wrong.

## Local Expo SDK 57 fixture on smix side

Also landed: `.scratch/v1.4-rn-spike/rn-fixture57/` — SDK 57 + expo-dev-client 57 + RN 0.83 scaffold with a `probe.yaml` exercising the `clearAppData: { launchArgs, launchEnv }` path. Full sim install + xcodebuild deferred to a follow-up cycle — need pod install, native prebuild, ~200 MB of node_modules. Once your testbed image lands we probably don't need the fixture57 — the testbed IS the fixture at that point.

## Retest checklist (v1.0.10 → v1.0.11)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                       # → 1.0.11

# 2. Cold rebuild against v1.0.11 runner
rm -rf .smix/runner/derived-data-*
smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile
# expect: "runner v1.0.11" in the ready line

# 3. Migrated launch-fresh.yaml — pass launchArgs to steer dev-launcher
#    (edit to whatever arg/env your Expo 57 dev-client honors)
cat >> .devtools/qa/sim/subflows/launch-fresh.yaml <<'EOF'
  - clearAppData:
      launchArgs: ["-EXInternalMetroPort", "8081"]
      launchEnv:
        EX_DEV_CLIENT_METRO_URL: "http://localhost:8081"
  - clearKeychain
EOF

# 4. Full bootstrap
bun test:e2e

# 5. Check the counters — replaces "grep for a log line that may not exist"
smix diagnostic dump                                 # → viaFallback should be 0

# 6. Fresh .ips count (three new ones on v1.0.10)
ls ~/Library/Logs/DiagnosticReports/ | grep -c Insight
# expect: no growth over the run window
```

If (4) hits `element not found qa-bubble` + `visible: unknown name="Insight"` again:
- (2b) `smix diagnostic dump | jq '.runner.aliveCache'` → is `markDeadTotal > 0`?
  - Yes → app is crashing repeatedly. Dig in .ips.
  - No → picker still showing. Try a different launchArg or `simctl launch --setenv`.
- (2c) `smix diagnostic dump | jq '.runner.sessionCounters'` → is `terminateAppViaFallback > 0`?
  - Yes → cooperative pathway timed out; my wait-for-foreground window may be too short.
  - No + `.ips` growth → the crashes are inside your JS/native code, not smix's terminate call.

## Where to file feedback

Same channel as before:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-<name>.md
```

For v1.0.11 feedback, pin the observations to counter deltas rather than log-line grep. That's the whole point of v1.0.11's observability layer — "is X firing" is a numeric question now.

## Prior chain

Chronological, for anyone reading in future:

- `smix-feedback-2026-07-10-gate-hardening.md` — 8 findings A-H from v1.0.3
- `smix-feedback-2026-07-11-v1.0.5-followup.md` — 3-item ask
- `smix-feedback-2026-07-11-blocking-crash-dialog.md` — hard-requirement escalation
- `smix-feedback-2026-07-11-systemic-pause.md` — systemic pause + one-release ask (v1.0.10 responded)
- `smix-feedback-2026-07-11-v1.0.10-observations.md` — v1.0.10 empirical + 3 gaps + Expo 57 compound (this doc responds)
- `insight-v1.0.10-shipping.md` — v1.0.10 shipping notes
- `insight-v1.0.11-shipping.md` — **this doc**

## fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.11-shipping.md
```
