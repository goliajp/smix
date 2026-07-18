# smix v1.0.14 — shipping notes for insight

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-11-post-native-fix.md` + `smix-feedback-2026-07-11-v1.0.12-answers.md`
RFC: `.claude/rfcs/1.0.14-cluster-a-b-c-plus-retry.md`; verb-selection guide at `.claude/rfcs/verb-selection-guide.md`.

## TL;DR (3 lines per Q10)

- **v1.0.14 delivers Cluster A (`resetAppData` verb) + Cluster B (`--metro-log <path>` on `smix diagnostic dump`) + verb-selection RFC.**
- **No yaml changes required unless you want to adopt `resetAppData` — legacy `clearAppData` / `clearState` still work byte-identical.**
- **Cluster C (Swift-side interactive polling + reason disambiguation) and §6 (`--retry N`) scaffolded on the wire but land in v1.0.15 with Swift impl; version-skip 1.0.11 → 1.0.14 per no-interim-ships directive [see 2026-07-11-post-native-fix.md §Asks ordering].**

## What v1.0.14 closes from your feedback

- **§1 clearAppData ceremony cost** → **closed via new `resetAppData` verb.** URL-scheme JS-wipe; app owns the reset semantics. Dev-launcher metro URL + Metro bundle cache + dev-tools state all survive.
- **§2 external metro log-gate coverage** → **closed via `smix diagnostic dump --metro-log <path>`.** File tail at dump time; no coupling to Metro's WebSocket protocol per your Q5 preference.
- **§5 diagnostic-dump includes metro log tail** → **closed as part of §2.** `runner.metroLogTail` in JSON payload; new text section in the human-readable dump.
- **Q10 shipping-doc format** → **applied here.** TL;DR at top; back-links to prior docs where relevant.
- **Verb-selection guide** → **written.** `.claude/rfcs/verb-selection-guide.md` covers when to reach for `clearAppData` vs `resetAppData` vs `clearState`.

Deferred to v1.0.15 (wire scaffolding in place; Swift/impl deferred):

- **§3 launchAppReachedInteractive counter** — wire field `SessionLifecycleCounters.launchAppReachedInteractive` lands v1.0.14 (currently always 0); Swift-side polling per your Q7/Q8 in v1.0.15.
- **§4 app-unavailable reason disambiguation** — same reason. Bundled with Cluster C in v1.0.15.
- **§6 retry attribution** — wire types `FlowAttemptRecord` + `DiagnosticDumpResponse.recentFlows` land v1.0.14 (currently empty); `smix run --retry N` mechanism in v1.0.15.

Rationale for the split: Cluster C + §6 need substantial Swift runner-side work + a new retry primitive that doesn't exist in smix-adapter-maestro today. Bundling them into v1.0.14 pushed the session over budget; splitting doesn't affect what you most need [see 2026-07-11-post-native-fix.md TL;DR — "§1 is the core one"]. Wire scaffolding lets v1.0.15 land the Swift work without a wire migration on your side.

## Single-command upgrade (same as v1.0.10-v1.0.11)

```bash
cargo install smix                                            # → smix 1.0.14
cp ~/.cargo/bin/smix ~/.local/bin/smix                        # if you use the ~/.local/bin symlink
smix --version                                                # → smix 1.0.14
smix runner up <UDID> --bundle <bundle>                       # auto-syncs sources to v1.0.14
curl -s http://127.0.0.1:22087/health | jq .runnerVersion     # → "1.0.14"
```

The auto-sync path from v1.0.10 remains — nothing new to know.

## Cluster A — `resetAppData` verb

### Short-form

```yaml
- resetAppData: 'insight://dev-mutate?action=reset'
```

Fires `simctl openurl <UDID> insight://dev-mutate?action=reset`, returns immediately. Best-effort "did the app get the URL" — no completion signal.

### Map-form with log-line completion signal (recommended)

```yaml
- resetAppData:
    via: url-scheme
    url: 'insight://dev-mutate?action=reset'
    waitFor:
      logLinePattern: '\[insight-dev\] reset-complete token='
    timeoutMs: 5000
```

Requires `smix run --metro-log /tmp/metro.log` on the run command so smix-metro-log has the tail subscribed. Runtime:

1. Fire `simctl openurl <UDID> <url>`.
2. Advance `resetAppDataTotal` counter.
3. Poll the metro log tail for the pattern with the specified timeout.
4. On match → succeed. On timeout → advance `resetAppDataTimedOut` and fail the step.

### App-side contract (yours to add if not already)

Per your Q1 answer, adding this in `src/debug/dev-url-handler.ts` is a one-file change:

```typescript
case 'reset': {
  await mmkv.clearAll();
  const token = crypto.randomUUID();
  console.log(`[insight-dev] reset-complete token=${token}`);
  return response.json({ ok: true, token });
}
```

The token disambiguates concurrent runs — you can grep for a specific token in the metro log to check "did MY reset finish" if you have multiple flows in-flight (rare).

### Map-form with sleep fallback (no metro log)

```yaml
- resetAppData:
    url: 'insight://dev-mutate?action=reset'
    waitFor:
      sleepMs: 500
```

Best-effort per Q1 answer tail — fire URL, sleep 500 ms, return. Only reach for this when you can't pass `--metro-log <path>`.

### Counter observability

```bash
$ smix diagnostic dump | grep -A1 resetAppData
  resetAppData: total=6 timedOut=0
```

- `total` — every time the verb dispatched (success or timeout).
- `timedOut` — dispatches where the URL fired but the pattern didn't arrive inside `timeoutMs`. `> 0` means your app either didn't handle the URL or didn't emit the completion log line inside the window.

Persisted at `~/.local/share/smix/reset-app-data-counters.json` so `smix run` and `smix diagnostic dump` share state across process boundaries.

## Cluster B — `--metro-log <path>` on `smix diagnostic dump`

Per your Q5 (path stable, no rotation, no mid-run truncate), we went with O_APPEND + tail-from-EOF; no inotify machinery.

```bash
$ smix diagnostic dump --metro-log /tmp/metro.log --metro-log-tail-lines 200
# … existing sections …
=== metro log tail (last 200 of file) ===
  <line 1>
  <line 2>
  …
```

Default 200 per Q6; `--metro-log-tail-lines N` overrides.

JSON payload:

```jsonc
{
  "runner": {
    // … existing fields …
    "metroLogTail": ["line 1", "line 2", …]
  }
}
```

Empty array when `--metro-log` is not set. Backward-compat additive — pre-v1.0.14 consumers ignoring the field see zero change.

For **runtime** tail during `smix run` (used by the pre-existing `expect.signal` verb and now by `resetAppData waitFor.logLinePattern`), the existing `smix-metro-log FileTailSubscriber` continues to serve — same `--metro-log-url file:///path` shape you already know.

## Cluster D — verb-selection guide

`.claude/rfcs/verb-selection-guide.md` at `/Users/doracawl/workspace/goliajp/smix/.claude/rfcs/verb-selection-guide.md` covers:

- Decision tree: `clearAppData` vs `resetAppData` vs `clearState` based on your build shape + whether your app has a URL-scheme reset primitive.
- Comparison matrix: 13 attribute rows across all three verbs.
- Migration crib: your current `clearAppData + clearKeychain + launchApp` yaml pattern → v1.0.14 split baseline + fast-path pattern.

TL;DR of the guide: your bootstrap batch of "3 flows × N launches per flow" wants `clearAppData` for the first launch (fresh baseline) and `resetAppData` for every subsequent launch (skip the 15-30 s dev-client ceremony). See the guide for the detailed migration.

## Cluster C + §6 deferred to v1.0.15 (wire scaffolding land here)

We did the wire work in v1.0.14 so v1.0.15 doesn't require a wire migration on your side. Fields sit at 0 / empty until v1.0.15 populates them:

- `SessionLifecycleCounters.launchAppReachedInteractive` (Cluster C — will populate once Swift-side polling lands)
- `SessionLifecycleCounters.launchAppTimedOutBeforeInteractive` (same)
- `DiagnosticDumpResponse.recentFlows: Vec<FlowAttemptRecord>` (§6 — will populate once `smix run --retry N` mechanism lands)
- `FlowAttemptRecord { flowName, attempts: [{ attemptIndex, status, errorClass, ipsGenerated, wallMs }] }` — new struct on the wire; consumers can deserialize now, values arrive in v1.0.15.

All `#[serde(default)]`; ignore the fields safely if you're on a v1.0.14 SDK.

## Ship gate observations (real-sim, `sim-insight` iOS 26.5)

Same discipline as v1.0.10 / v1.0.11 — Preferences smoke, then insight canary post-publish. Docker testbed image (§C.4 from your v1.0.10 followup, Q9 in the v1.0.12 open-questions) still on your TODO; when it arrives we wire `scripts/release/corpus-gate.sh` and v1.0.15+ ships gate on your real batch.

```
smix --version                                          → smix 1.0.14
smix runner install --force                            → extracted 303 files at v1.0.14
smix runner up …                                        → runner up: http://localhost:22087/health = 200 (runner v1.0.14)
/health.runnerVersion                                  → "1.0.14"
smix diagnostic dump --metro-log <tmp> --metro-log-tail-lines 3
  → renders "=== metro log tail (last 3 of file) ===" section correctly
smix diagnostic dump | grep resetAppData
  → "resetAppData: total=0 timedOut=0" (baseline before any dispatch)
```

680 workspace tests + 3 new resetAppData parser + 6 new tail_lines unit + 1 new counter roundtrip green.

## Retest checklist (v1.0.11 → v1.0.14)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                                   # → 1.0.14

# 2. Cold rebuild against v1.0.14 runner
rm -rf .smix/runner/derived-data-*
smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile
# → "runner up: … (runner v1.0.14)"

# 3. Add URL scheme reset handler on your side (if not already):
#    src/debug/dev-url-handler.ts: case 'reset' → mmkv.clearAll() +
#    console.log('[insight-dev] reset-complete token=' + genUuid())

# 4. Split your yaml — first-flow baseline via clearAppData, subsequent via resetAppData
cat > .devtools/qa/sim/subflows/launch-fresh-baseline.yaml <<'EOF'
- clearAppData:
    launchArgs: ["-EXInternalMetroPort", "8081"]
- clearKeychain
- launchApp: { waitForForegroundMs: 15000 }
EOF

cat > .devtools/qa/sim/subflows/launch-fresh-fast.yaml <<'EOF'
- resetAppData:
    via: url-scheme
    url: 'insight://dev-mutate?action=reset'
    waitFor:
      logLinePattern: '\[insight-dev\] reset-complete token='
    timeoutMs: 5000
- launchApp: { waitForForegroundMs: 15000 }
EOF

# 5. Full bootstrap with --metro-log for the completion-signal path
bun test:e2e -- --metro-log /tmp/metro.log

# 6. Verify resetAppData counters advanced
smix diagnostic dump --metro-log /tmp/metro.log --metro-log-tail-lines 50 | grep -E "resetAppData|metro log"

# 7. Fresh .ips count baseline
ls ~/Library/Logs/DiagnosticReports/ | grep -c Insight

# Expected on green batch:
# - resetAppData.total > 0 (equal to num non-first launches per batch)
# - resetAppData.timedOut = 0
# - No new .ips from resetAppData path (URL-scheme JS-wipe doesn't SIGKILL)
```

If step 5's `resetAppData` step fires but `timedOut > 0`:

- The URL fired but no `[insight-dev] reset-complete token=` line arrived in the metro log within `timeoutMs`. Either:
  - Your handler isn't wired yet (add per step 3).
  - Handler wired but not producing the log line (grep metro log for `insight-dev` — if empty, handler isn't running).
  - Metro log path wrong. `smix diagnostic dump --metro-log <path>` verifies the tail reads from where you think.

If step 5's flows still time out on `dev-bubble` even with the reset working: that's now app-side (JS bundle load, network, etc.), no longer a smix concern.

## What v1.0.14 does NOT change from v1.0.11

- Wire compatibility on every non-new field.
- Runner-side HTTP surface (no new endpoints; all v1.0.14 work is CLI + host).
- `clearAppData` / `clearState` / `clearKeychain` / legacy yaml verbs all identical.
- `SmixCoreFFI.xcframework` distribution + auto-sync.
- Ship-gate discipline (Preferences smoke + insight canary post-publish).

## Where to file feedback

Same channel [see 2026-07-11-post-native-fix.md ¶last]:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-<name>.md
```

For v1.0.14 feedback: please pin observations to counter deltas (`resetAppData.total`, `resetAppData.timedOut`, `sessionCounters.terminateAppViaFallback`, etc.) rather than log-line grep. That's the whole point of the observability layer this cycle built out.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.14-shipping.md
```

## Prior chain

- `smix-feedback-2026-07-10-gate-hardening.md`
- `smix-feedback-2026-07-11-v1.0.5-followup.md`
- `smix-feedback-2026-07-11-blocking-crash-dialog.md`
- `smix-feedback-2026-07-11-systemic-pause.md`
- `insight-v1.0.10-shipping.md`
- `smix-feedback-2026-07-11-v1.0.10-observations.md`
- `insight-v1.0.11-shipping.md`
- `smix-feedback-2026-07-11-post-native-fix.md` — the 6-ask systemic feedback [see for §-numbered items]
- `insight-2026-07-11-post-native-fix-response.md` — smix response + direction
- `insight-2026-07-11-v1.0.12-open-questions.md` — 10 blocking questions
- `smix-feedback-2026-07-11-v1.0.12-answers.md` — insight's Q&A [see for Q-numbered items]
- **this doc** — v1.0.14 shipping notes
