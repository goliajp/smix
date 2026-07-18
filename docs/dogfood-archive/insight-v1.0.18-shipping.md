# smix v1.0.18 — shipping notes for insight (round-4 QoL asks landed)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12-v1.0.17-results.md` — v1.0.17 batch results + 2 QoL asks
Prior: `insight-v1.0.17-shipping.md`

## TL;DR (3 lines per Q10)

- **Both round-4 QoL asks land in v1.0.18.** Per-session `interactiveNamedIds` in `session/list` + `/diagnostic/dump`, and `waitForAnimationToEnd` accepts a numeric ms override.
- **v1.0.17 works as designed** — 0 test_runForever failures, `launchAppReachedInteractive: 6/6`, `.ips` stable at 36→36. Snapshot-walk fix landed cleanly.
- **Your `bugfix/GOL-611-native-cold-boot-crash` branch is ready to merge to develop** [see round-4 §Ship-worthiness signal]. The remaining 3/3 flow fail is on your side (RN Fabric a11y-exposure lag; testIDs + timeouts + `waitForAnimationToEnd` tuning).

## Round-4 report acknowledgment

Your v1.0.17 batch numbers were the cleanest counter dump we've had:

```
launchAppTotal:                    6
launchAppReachedForeground:        6
launchAppReachedInteractive:       6   ← every launch reached probeable tree
launchAppTimedOutBeforeInteractive: 0
terminateAppTotal:                 6
terminateAppViaFallback:           0
aliveCache.wired: true, markAliveTotal: 6, markDeadTotal: 0
.ips: 36 → 36
```

This is the shape v1.0.10-v1.0.17 was building toward: numerically visible, every fired mechanism accounted for. Naming the remaining failure — post-`tapOn` extendedWaitUntil timing out during Fabric animation transitions — as **decoupled** from `launchAppReachedInteractive` is exactly right. The primitive works; the timing gap is downstream of the primitive.

**Branch merge readiness [see round-4 §Ship-worthiness signal]** — you validated 5 batch rounds (v1.0.11 → v1.0.14 → v1.0.15 → v1.0.16 → v1.0.17) with no `.ips` regression at any step. The three native UAF patches (`NotificationCenterManager`, `RN Scheduler`, `EXPermissionsService`) are ship-worthy independent of the flow-completion question.

## D1 — `interactiveNamedIds` per-session in `session/list` + `/diagnostic/dump`

Previously the sample was only in the `/session/launch-app` response body — persist gap. Now every `launchApp` completion updates the session's `lastInteractiveNamedIds` on the runner side; subsequent `session/list` and `/diagnostic/dump` calls surface it.

### Wire

```jsonc
// /session/list (v1.0.18+)
{
  "sessions": [
    {
      "sessionId": "…",
      "bundleId": "com.focusai.app.mobile",
      "openedAtMs": …,
      "lastActivatedAtMs": …,
      "interactiveNamedIds": ["dev-bubble", "btn-login", …]  // ← new, default []
    }
  ]
}

// /diagnostic/dump (v1.0.18+) — same field on each session in `sessions[]`
```

### Real-sim verification (Preferences smoke)

```
$ curl -s -X POST http://127.0.0.1:22087/session/launch-app \
    -d '{"sessionId":"…","waitForForegroundMs":15000,"waitForInteractiveMs":15000}'
{
  "ok": true, "reachedInteractive": true,
  "interactiveNamedIds": ["Settings", "AdditionalDimmingOverlay", …8]
}

$ curl -s -X POST http://127.0.0.1:22087/session/list | jq
{
  "sessions": [
    {
      "sessionId": "B26C2CA3-…",
      "bundleId": "com.apple.Preferences",
      "openedAtMs": 1783794274620,
      "lastActivatedAtMs": 1783794274620,
      "interactiveNamedIds": [                    ← same 8-name sample
        "Settings", "AdditionalDimmingOverlay",
        "com.apple.settings.primaryAppleAccount",
        "apple.id", "chevron.forward",
        "com.apple.settings.general", "chevron.forward",
        "com.apple.settings.accessibility"
      ]
    }
  ]
}
```

Zero yaml migration. Runner writes the field on every launch; the same JSON body will appear per-session as your bootstrap batch runs.

### What you get to see now

For your predicted list from round-3 (`dev-bubble` / `qa-bubble` / `btn-env-staging` / `btn-login` / `btn-forget-password`):

- If interactive-probe fired at auth-landing → `interactiveNamedIds` includes 3+ of those, PLUS maybe `Insight`/`Landing`/other named layout containers. You know the probe hit the right screen.
- If probe fired at splash-screen leak → `interactiveNamedIds` includes 3+ Fabric-native `RCTView` / bounded a11y annotations that aren't in your positive fingerprint. You know to bump `.smix/config.yaml interactiveProbe.minIdentifierCount` from 3 to 5, or extend the ignore list, or delay the probe.
- If empty across all 6 launches → probe never fired despite 30 s window. That signals the "waitForInteractiveMs default too tight" case; extend or use `clearAppData` with an explicit override.

The `interactiveNamedIds` sample IS the tiebreaker.

## D2 — `waitForAnimationToEnd` numeric override + doc

Your round-4 §"Smix ask" bullet 2 asked whether `SmixQuiescenceSwizzle.m` no-ops this verb. **Answer: No.** The swizzle only touches XCTest's internal daemon-side idle-wait (which `descendants(matching:)` calls internally). This yaml verb has always been a fixed 400 ms `tokio::time::sleep` at the Rust runtime layer — never went through XCTest quiescence in the first place. Undocumented. Fixed now.

### v1.0.18 additions

```yaml
# Bare (unchanged, maestro-compat) — 400 ms sleep
- waitForAnimationToEnd

# Numeric override (v1.0.18) — integer = ms sleep
- waitForAnimationToEnd: 500
- waitForAnimationToEnd: 750
- waitForAnimationToEnd: 1000
```

The variant type is now `Step::WaitForAnimationToEnd { duration_ms: u64 }`. `Duration::from_millis(duration_ms)` sleep at runtime. No XCTest interaction, no snapshot invalidation — just a bounded pause.

### For your SlideModal case

Your round-4 tap-modal-transition sequence:
```yaml
- tapOn: { id: 'dev-bubble' }
- waitForAnimationToEnd: 500     # ← bump from bare 400 ms to accommodate slide-in
- extendedWaitUntil: { visible: { id: 'btn-env-staging' }, timeout: 30000 }
```

The 500 ms captures the SlideModal slide-in animation completing (~333 ms per your round-4 §Root-cause hypothesis timeline). The 30 s `extendedWaitUntil` accommodates whatever additional a11y-exposure latency Fabric adds post-animation.

If the Fabric a11y bridge lags animation completion (Possibility B in your report), even the 30 s `extendedWaitUntil` isn't enough. In that case:
1. Screenshot at failure moment to confirm the panel is visible (rules out Possibility A).
2. Consider `- takeScreenshot: /tmp/after-tap-dev-bubble.png` immediately before the `extendedWaitUntil` to capture what smix thinks is on-screen.
3. If panel visible + smix sees `unknown` descendants → RN Fabric a11y-timing regression; consider adopting testIDs on `SlideModal`/`DevPanel` container components explicitly (their child buttons already have IDs but the SlideModal wrapper may not).

## Retest checklist (v1.0.17 → v1.0.18)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.18

# 2. Cold rebuild against v1.0.18 runner
rm -rf .smix/runner/derived-data-*
smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile

# 3. Optional yaml experiment for dev-bubble → btn-env-staging (D2)
#    Edit launch-fresh.yaml or subflow:
#      - tapOn: { id: 'dev-bubble' }
#      - waitForAnimationToEnd: 500
#      - extendedWaitUntil: { visible: { id: 'btn-env-staging' }, timeout: 30000 }

# 4. Full bootstrap
bun test:e2e -- --metro-log /tmp/metro.log

# 5. Verify runner survived (v1.0.17's guarantee, sanity-checked here)
grep "test_runForever] : Failed" .smix/runner/runner-*.log
# → 0 matches (same as v1.0.17)

# 6. NEW — verify D1 per-session interactiveNamedIds
smix diagnostic dump --json | jq '.runner.sessions[] | {sessionId, interactiveNamedIds}'
# → For every session, interactiveNamedIds contains the 8-name sample
#   from that session's last launch.

# Or via session/list:
curl -s -X POST http://127.0.0.1:22087/session/list | jq '.sessions[].interactiveNamedIds'

# 7. Report the interactiveNamedIds sample as promised in round-3 §"Answering
#    your v1.0.16 retest checklist §6" — compare against your positive
#    fingerprint list (dev-bubble / qa-bubble / btn-env-staging / btn-login /
#    btn-forget-password).
```

## What we NOT change from v1.0.17

- Snapshot-walk fix (v1.0.17 D1) preserved.
- Snapshot-refresh per iteration (v1.0.16 D1) preserved.
- Rest of Cluster A + B + C + retry, `resetAppData`, `--metro-log`, etc — all preserved.
- Wire compatibility on every non-new field. `SessionSummary.interactiveNamedIds: [String] = []` default; pre-v1.0.18 consumers ignoring it see zero change.

## What v1.0.18 does NOT solve

- **Root cause of 3/3 flow fail** — that's your side (Fabric a11y-exposure lag during animation). D2 gives you the tuning knob (`waitForAnimationToEnd: N`); D1 gives you the diagnostic signal (`interactiveNamedIds` sample). But the underlying RN Fabric a11y timing is upstream of smix.
- **Full `launchApp: waitForInteractiveMs` routing** — still emits the warning (v1.0.16 marker unchanged); use `clearAppData` for full behavior.
- **Docker testbed image** — your side; ship-gate stays on Preferences smoke.

## Where to file feedback

Same channel [see prior-doc `smix-feedback-2026-07-11-post-native-fix.md` ¶last]:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.18 feedback: please include the `interactiveNamedIds` sample from the `session/list` post-batch. That's the whole point of D1 — we finally have a way to tie "reachedInteractive: 6/6" back to a specific screen fingerprint.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.18-shipping.md
```

## Prior chain

- `smix-feedback-2026-07-10-gate-hardening.md`
- `smix-feedback-2026-07-11-v1.0.5-followup.md`
- `smix-feedback-2026-07-11-blocking-crash-dialog.md`
- `smix-feedback-2026-07-11-systemic-pause.md`
- `insight-v1.0.10-shipping.md`
- `smix-feedback-2026-07-11-v1.0.10-observations.md`
- `insight-v1.0.11-shipping.md`
- `smix-feedback-2026-07-11-post-native-fix.md`
- `insight-2026-07-11-post-native-fix-response.md`
- `insight-2026-07-11-v1.0.12-open-questions.md`
- `smix-feedback-2026-07-11-v1.0.12-answers.md`
- `insight-v1.0.14-shipping.md`
- `insight-v1.0.15-shipping.md`
- `smix-feedback-2026-07-11-round-2-status.md`
- `insight-v1.0.16-shipping.md`
- `smix-feedback-2026-07-12-v1.0.16-runner-crash.md`
- `insight-v1.0.17-shipping.md`
- `smix-feedback-2026-07-12-v1.0.17-results.md` — the round-4 results this doc responds to
- **this doc** — v1.0.18 QoL asks (per-session interactiveNamedIds + waitForAnimationToEnd override)
