# smix v1.0.15 — shipping notes for insight

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-11-post-native-fix.md` §3/§4/§6 + `smix-feedback-2026-07-11-v1.0.12-answers.md` Q7/Q8
RFC: `.claude/rfcs/1.0.15-cluster-c-plus-retry.md`

## TL;DR

- **v1.0.15 completes the Cluster C + §6 work v1.0.14 wire-scaffolded** [see 2026-07-11-post-native-fix.md §3/§4/§6]. Insight's three followup asks are now implementation-complete, not just wire-ready.
- **No yaml changes required.** `App::clear_app_data_with_launch` defaults `wait_for_interactive_ms: Some(30_000)` — existing `clearAppData` yaml automatically populates the interactive counter.
- **`--retry N` is opt-in** on `smix run` — default 1 keeps pre-v1.0.15 behaviour.

## Cluster C D1 — `launchAppReachedInteractive` counter (§3 of your post-native-fix feedback)

The runner-side `launchApp` handler now polls `entry.app.descendants(matching: .any)` at 500 ms cadence after `.state == .runningForeground` is observed. Counts descendants with a non-empty `accessibilityIdentifier` that isn't in the ignore list. Fires `reachedInteractive` on ≥ `minIdentifierCount`; timeout increments `launchAppTimedOutBeforeInteractive` per your Q8 answer (a) — `launchApp` still returns success either way, consumer detects via counter delta.

### Config

`.smix/config.yaml` at your repo root (adjacent to `.smix/sims.json`):

```yaml
interactiveProbe:
  minIdentifierCount: 3
  ignore:
    - SplashScreenLogo
    - com.focusai.app.mobile
```

Missing file → bundled defaults per your Q7 answer:
- `minIdentifierCount: 3`
- `ignore: [SplashScreenLogo, com.focusai.app.mobile]`

Consumer overrides via the yaml above. CLI reads it via `serde_norway`, JSON-encodes, forwards to the runner as `TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON` env at boot. Xcode strips the `TEST_RUNNER_` prefix; runner sees `SMIX_INTERACTIVE_PROBE_JSON` at `test_runForever` scope.

### Zero-yaml-migration path

`App::clear_app_data_with_launch` defaults `wait_for_interactive_ms: Some(30_000)` — your existing yaml:

```yaml
- clearAppData
- clearKeychain
- launchApp: {}
```

or your v1.0.14 migrated shape:

```yaml
- clearAppData:
    launchArgs: ["-EXInternalMetroPort", "8081"]
    launchEnv:
      EX_DEV_CLIENT_METRO_URL: "http://localhost:8081"
```

both automatically start populating `launchAppReachedInteractive` counter deltas in `smix diagnostic dump`. No yaml migration required.

### Sample observation

```
POST /session/launch-app  {sessionId, waitForForegroundMs:15000, waitForInteractiveMs:15000}
→ HTTP 200
{"ok":true,
 "wallMs":2805,
 "waitedMs":0,
 "terminalState":4,
 "reachedInteractive":true,
 "interactiveNamedIds":["dev-bubble","btn-env-staging","qa-bubble", … up to 8]}

# When reachedInteractive is false:
# - process is foreground BUT tree unusable (splash / dev-launcher / sparse annotation)
# - counter launchAppTimedOutBeforeInteractive +1

$ smix diagnostic dump
  interactive: reachedInteractive=6 timedOutBeforeInteractive=0
```

`reachedInteractive == launchAppTotal` means every launch got a probeable tree. Any positive `timedOutBeforeInteractive` = "process foreground but tree unusable" — the exact case that stumped `waitFor(dev-bubble)` in your pre-v1.0.14 batches.

## Cluster C D2 — `AppUnavailableReason` + `hint` on `/tree` unavailable envelope (§4)

Wire shape (v1.0.15+):

```jsonc
// GET /tree unavailable envelope, v1.0.15
{
  "ok": false,
  "error": "snapshot_unavailable",
  "reason": "alive-but-tree-empty",
  "hint": "Process foreground but no named a11y descendants — likely splash-screen ceremony still running, or your app's accessibility tree lacks accessibilityIdentifier coverage."
}
```

- `reason` — the categorized enum. String kebab-case values: `crashed-during-init`, `alive-but-tree-empty`, `alive-but-tree-stale`, `driver-disconnected`, `unknown`.
- `hint` — actionable text steering downstream tooling. Compiled into smix (English only); consumers reformatting for i18n can key off `reason` and substitute their own.

Runner detection strategy [see 2026-07-11-post-native-fix.md §4 for the four categories]:

- **`crashed-during-init`** — either the cache is suppressing (observed `XCTIssue "Application X is not running"`) OR the UITest inference sees `XCUIApplication.state == .notRunning`. Hint suggests looking at `~/Library/Logs/DiagnosticReports/`.
- **`alive-but-tree-empty`** — `.state == .runningForeground` (or backgroundRunning) but snapshot returned nil. Hint: check splash ceremony or a11y annotation coverage.
- **`driver-disconnected`** — the `contextGuardedResponse` fallback fired (guarded closure threw entirely). Hint: try `smix runner cycle`.
- **`alive-but-tree-stale`** and full stale-hash comparison — wire-defined but not yet emitted by the runner (would need a per-session tree hash cache to compare against; complexity vs value tradeoff we can revisit).
- **`unknown`** — pre-v1.0.15 fallback; when the inferer closure isn't wired, or `XCUIApplication.state` returns `.unknown`.

Rust client-side:

```rust
match err {
    RunnerTransportError::AppUnavailable {
        endpoint,
        target,
        category: Some(ref reason),  // ← v1.0.15 category enum
        hint: Some(ref hint),         // ← v1.0.15 actionable text
        ..
    } => {
        eprintln!("app unavailable ({reason}): {hint}");
    }
    RunnerTransportError::AppUnavailable {
        endpoint,
        target,
        reason,  // ← pre-v1.0.15 legacy free-form
        ..
    } => {
        eprintln!("app unavailable (legacy runner): {reason:?}");
    }
    _ => unreachable!(),
}
```

Pre-v1.0.15 runners emit the legacy `{"ok":false,"error":"snapshot_unavailable"}` — client's `category` and `hint` come back as `None`. Backward compat preserved.

## §6 D3 — `smix run --retry N` + per-flow attempt attribution

### CLI

```bash
smix run flow1.yaml flow2.yaml --retry 2
```

Default 1 = one attempt per flow = pre-v1.0.15 behaviour. `--retry N > 1` runs each flow up to N times; first success short-circuits.

### Attribution

Each attempt records:

```jsonc
{
  "attemptIndex": 0,     // zero-based; 0 = first try
  "status": "ok",        // "ok" | "timeout" | "error"
  "errorClass": "TIMEOUT", // "TIMEOUT" | "DRIVER_ERROR" | "EXPECTATION_FAILURE" | "RUNNER_UNREACHABLE" | "EXIT_<n>"; null on ok
  "ipsGenerated": "Insight-2026-07-11-153125.ips",  // filename that appeared during this attempt's window; null when none
  "wallMs": 12345
}
```

Persistence: `~/.local/share/smix/flow-attempts.json` (rolling last 32 flows). `smix diagnostic dump` reads + overlays into JSON payload `runner.recentFlows[]` and renders `=== recent flows (retry attribution) ===` in text output:

```
=== recent flows (retry attribution) ===
  flow: launch-fresh
    attempt #0 status=error errorClass=TIMEOUT wallMs=95_432 ipsGenerated=Insight-2026-07-11-153125.ips
    attempt #1 status=ok wallMs=42_123
  flow: force-update
    attempt #0 status=ok wallMs=38_456
```

The `.ips` attribution comes from diffing `~/Library/Logs/DiagnosticReports/` before/after each attempt, filtered by bundle-id last-component match. Best-effort — deduplicates across attempts naturally since the set diff only shows new entries.

## Wire compatibility

All v1.0.15 additions are wire-additive with `#[serde(default)]`:

- `SessionAppLifecycleRequest.wait_for_interactive_ms: Option<u64>` — opt-in.
- `SessionAppLifecycleResponse.reached_interactive: bool` + `.interactive_named_ids: Vec<String>` — additive; pre-v1.0.15 clients ignore.
- `TreeRoute.unavailable(reason:hint:)` — new variant; legacy `unavailable()` unchanged.
- `RunnerTransportError::AppUnavailable.category: Option<String>` + `.hint: Option<String>` — additive; `None` for pre-v1.0.15 runners.
- `SessionLifecycleCounters.launch_app_reached_interactive` + `.launch_app_timed_out_before_interactive` — already in v1.0.14 wire; v1.0.15 populates.
- `DiagnosticDumpResponse.recent_flows` — already in v1.0.14 wire; v1.0.15 populates via CLI overlay.

A v1.0.14 client reading a v1.0.15 wire body sees zero behaviour change on the fields it ignores. A v1.0.15 client reading a v1.0.14 wire body sees the new fields as their defaults (`false` / empty / `None`).

## Ship gate (real-sim, `sim-insight` iOS 26.5 Preferences smoke)

```
$ smix --version                                          → smix 1.0.15
$ smix runner install --force                            → extracted 303 files at v1.0.15
$ smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.apple.Preferences
runner up: http://localhost:22087/health = 200 (runner v1.0.15)
$ curl -s http://127.0.0.1:22087/health | jq .runnerVersion    → "1.0.15"

$ curl -X POST http://127.0.0.1:22087/session/launch-app -d '{"sessionId":"…","waitForForegroundMs":15000,"waitForInteractiveMs":15000}'
→ HTTP 200
→ reachedInteractive: true
→ interactiveNamedIds: ["Settings", "AdditionalDimmingOverlay", "com.apple.settings.primaryAppleAccount", "com.apple.settings.general", "com.apple.settings.accessibility", "com.apple.settings.actionButton", "com.apple.settings.camera", "com.apple.settings.homeScreen"]

$ smix diagnostic dump | grep interactive
  interactive: reachedInteractive=1 timedOutBeforeInteractive=0

$ /diagnostic/dump payload sessionCounters
  launchAppReachedInteractive: 1
  launchAppTimedOutBeforeInteractive: 0
```

680 workspace tests + all pre-existing tests green. `smix run --retry` mechanism not exercised in real-sim gate (needs a yaml with flaky expectations to fail-then-retry, out of scope for Preferences smoke); implementation locked by static tests.

## Retest checklist (v1.0.14 → v1.0.15)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                                # → 1.0.15

# 2. Cold rebuild against v1.0.15 runner
rm -rf .smix/runner/derived-data-*
smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile
# → "runner up: … (runner v1.0.15)"

# 3. (Optional) Consumer config for the interactive probe
cat > .smix/config.yaml <<'EOF'
interactiveProbe:
  minIdentifierCount: 3
  ignore:
    - SplashScreenLogo
    - com.focusai.app.mobile
EOF

# 4. Full bootstrap — no yaml changes required, clearAppData
#    automatically opts into interactive polling now
bun test:e2e -- --metro-log /tmp/metro.log

# 5. Verify interactive counters
smix diagnostic dump --metro-log /tmp/metro.log --metro-log-tail-lines 50 | grep -E "interactive|reset|recent flows"

# 6. Try retry on a known-flaky flow
smix run .devtools/qa/sim/subflows/launch-fresh.yaml --retry 2 --debug-output /tmp/qa-retry
# → runs at most 2 attempts; first success short-circuits
smix diagnostic dump | grep -A5 "recent flows"

# 7. Fresh .ips count
ls ~/Library/Logs/DiagnosticReports/ | grep -c Insight
```

**Expected on green**:
- `interactive: reachedInteractive` matches `launchAppTotal` (every launch got a probeable tree).
- `interactive: timedOutBeforeInteractive == 0` (no "up but unusable" states).
- `recent flows` section shows attempt #0 status=ok for every flow (no retries needed).
- `.ips` count unchanged over the run.

**If timed-out-before-interactive > 0**:
- Grab `interactiveNamedIds` from the runner response — what ax-ids did we see? If nothing in the "usable" set (see §Cluster A of the v1.0.14 shipping doc for your known bootstrap fingerprints), the probe is right and your app's a11y is sparse at that moment.
- Check `smix diagnostic dump | grep aliveCache` — is `markDeadTotal > 0`? If yes → app is crashing repeatedly. If no + timedOutBeforeInteractive > 0 → app is up but sparse a11y or scaffolding covering the real UI.

**If `smix diagnostic dump` shows `recent flows` but `attempt #0 ipsGenerated` filenames**:
- Attribution worked — each `.ips` links to the specific attempt that generated it. Now you can tell "flow X crashed on attempt 0, succeeded on retry" from "flow X crashed on attempt 0 AND 1, gave up".

## What v1.0.15 does NOT change

- `clearAppData` / `resetAppData` / `clearState` verbs — unchanged.
- Runner-side HTTP surface (all v1.0.15 fields are additive on existing endpoints; no new routes).
- Config file format — `.smix/config.yaml` was in the RFC for v1.0.13 but the actual field name (`interactiveProbe:`) hasn't been used before; adding it is opt-in.
- Ship-gate discipline (Preferences smoke + insight canary post-publish).

## Where to file feedback

Same channel [see prior-doc `smix-feedback-2026-07-11-post-native-fix.md` ¶last]:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-<name>.md
```

For v1.0.15 feedback: please pin observations to counter deltas + `interactiveNamedIds` sample from the launch-app response. That's the whole point of the observability the last two releases built out.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.15-shipping.md
```

## Prior chain

- `smix-feedback-2026-07-10-gate-hardening.md`
- `smix-feedback-2026-07-11-v1.0.5-followup.md`
- `smix-feedback-2026-07-11-blocking-crash-dialog.md`
- `smix-feedback-2026-07-11-systemic-pause.md`
- `insight-v1.0.10-shipping.md`
- `smix-feedback-2026-07-11-v1.0.10-observations.md`
- `insight-v1.0.11-shipping.md`
- `smix-feedback-2026-07-11-post-native-fix.md` — the systemic feedback [see for §-numbered items]
- `insight-2026-07-11-post-native-fix-response.md`
- `insight-2026-07-11-v1.0.12-open-questions.md`
- `smix-feedback-2026-07-11-v1.0.12-answers.md` — insight's Q&A [see for Q-numbered items]
- `insight-v1.0.14-shipping.md` — v1.0.14 Cluster A + B + verb-selection guide
- **this doc** — v1.0.15 Cluster C + §6 completes the feedback
