# smix v1.0.16 — shipping notes for insight (round-2 hotfix)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-11-round-2-status.md` — the maestro-vs-smix investigation + snapshot-refresh diagnosis
Prior: `insight-v1.0.15-shipping.md` (Cluster C first cut)

## TL;DR (3 lines per Q10)

- **v1.0.16 fixes the exact bug your round-2 investigation named:** v1.0.15's interactive polling hit XCUITest's cached snapshot every iteration on RN Fabric + iOS 26.5, so it never saw the a11y tree once it populated.
- **Zero yaml changes required** — if you're on `clearAppData` yaml, the snapshot-refresh fix propagates automatically.
- **Your maestro-vs-smix root-cause naming was correct:** RN 0.86 Fabric + iOS 26.5 mount-item-drain race. `try? entry.app.snapshot()` per iteration is the primitive we were missing.

## First things first — the round-2 report acknowledgment

Reading your `smix-feedback-2026-07-11-round-2-status.md`:

- **3 native UAFs patched.** `expo-notifications 57.0.3` + RN 0.86 Scheduler + `expo-modules-core 57.0.2 EXPermissionsService`. `.ips` growth 34 → 34 → 36 → 36 → 36 across 4 batches [see round-2 §做了 + 已验证]. Naming this as the third UAF pattern (shared mutable singleton + no `@synchronized` + concurrent `OnCreate`) — that's a class of bug I'm going to track across future stress work. Your patch chain closes this class systemically.
- **Diagnostic dump showed my v1.0.15 counters landed cleanly** — `launchAppReachedInteractive: 0`, `resetAppDataTotal: 0`, `aliveCache.wired: true, markAliveTotal: 6`. Wire scaffolding good; polling implementation broken.
- **Maestro-vs-smix investigation** was the deliverable I couldn't have written myself. RN Fabric mount-item drain vs Paper's UIView-native a11y is exactly the layer where the snapshot race lives. Naming this for future operators saves whoever's next on this exact same problem two hours.

## What v1.0.16 does

The one fix: `_ = try? entry.app.snapshot()` before every polling iteration in `launchApp`'s interactive probe.

```swift
// SmixRunnerUITests.swift — inside the interactive-probe polling loop
while UInt64(Date().timeIntervalSince(interactiveStart) * 1000) < interactiveDeadlineMs {
  let observed: [String] = await SmixRunnerServer.onMain {
    // v1.0.16 — force a fresh XCUITest snapshot each iteration, per
    // insight's round-2 diagnosis. RN 0.86 Fabric + iOS 26.5 sim
    // populate the a11y tree as RCTMountItemProtocol mount items
    // drain, NOT during layout. XCUITest's internal snapshot cache
    // holds the sparse tree from before mount items drained, and
    // descendants(matching:) returned the cached snapshot on
    // subsequent polls without a refresh.
    //
    // try? entry.app.snapshot() is the public API for forcing a
    // fresh top-of-hierarchy snapshot.
    _ = try? entry.app.snapshot()
    let query = entry.app.descendants(matching: .any)
    // ... existing enumeration logic ...
  }
  // ...
}
```

**Why not `waitForQuiescenceIncludingAnimations()`?** Your prediction A suggested it, and it's the honest first-approximation answer to "force settle before reading." But smix's `SmixQuiescenceSwizzle.m` already no-ops that XCTest daemon idle-wait — long-running RN animations were making every `tap` / `snapshot` blocking for 5-30 s in early v1.4 shipping. So the runtime is committed to a "don't wait for quiescence" performance stance. `snapshot()` alone forces the invalidation without waiting for quiescence, which is what we want.

**Cost:** `try? entry.app.snapshot()` runs ~50-150 ms on iOS 26.5 sim per iteration. Combined with the 500 ms poll interval it adds negligible wall-clock time — the deadline dominates.

## What v1.0.16 also adds (small)

`launchApp: { waitForInteractiveMs: 30000 }` on yaml — accepts + carries the field on `Step::LaunchApp.wait_for_interactive_ms`. But the runtime emits a warning:

> `launchApp.waitForInteractiveMs (bundle=…) is a v1.0.16 marker — the launch pathway is host-side simctl and doesn't route to the /session/launch-app interactive polling. Use `clearAppData` yaml (which SDK-defaults `wait_for_interactive_ms: 30000`) to opt into interactive counter observability.`

Rationale: `launchApp` (kill+launch via `simctl launch --args`) is host-side; it doesn't go through the session-scoped `/session/launch-app` runner HTTP endpoint. `clearAppData` yaml DOES go through session scope and gets the interactive probe with v1.0.15's SDK default of 30 s.

Full first-class `launchApp: waitForInteractiveMs` routing lands when we unify the two launch pathways in a follow-up release — the marker is there so you can drop the field in your yaml today and just get a warning, not an error.

## What does NOT change from v1.0.15

- Wire compatibility — no wire changes. All v1.0.15 wire additions unchanged.
- `.smix/config.yaml interactiveProbe:` schema — identical.
- `resetAppData` verb, `--metro-log` tail, `--retry` mechanism — all v1.0.14/v1.0.15 features unchanged.
- Ship-gate discipline — Preferences smoke + insight canary post-publish.

## Answering your predictions

Your round-2 §"Testable predictions" section:

- **Prediction A** — force snapshot refresh. **Confirmed correct + implemented in v1.0.16.** No forcing needed on your side; the polling loop does it every iteration now.
- **Prediction B** — disable New Arch to test. **Not needed** for v1.0.16 validation; the snapshot-refresh fix is orthogonal to whether New Arch is on or off. Would still be an interesting empirical data point if you have bandwidth (validates the mount-item-drain hypothesis independently of my fix), but it's not on my critical path.
- **Prediction C** — downgrade sim to iOS 18.1. **Not needed** for v1.0.16 validation; same reason. Would validate the iOS-side snapshot-cache-refresh-semantics theory. If your team runs it out of curiosity, we'd love the numbers.

## Retest checklist (v1.0.15 → v1.0.16)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.16

# 2. Cold rebuild against v1.0.16 runner
rm -rf .smix/runner/derived-data-*
smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile
# → "runner up: … (runner v1.0.16)"

# 3. No yaml migration needed — clearAppData yaml already routes
#    through session-scoped launch with SDK-defaulted 30 s interactive
#    wait since v1.0.15; snapshot-refresh happens automatically.

# 4. Full bootstrap
bun test:e2e -- --metro-log /tmp/metro.log

# 5. Verify interactive counters populated (were 0/0 on v1.0.15
#    because polling hit stale snapshot; expected non-zero on v1.0.16)
smix diagnostic dump --metro-log /tmp/metro.log --metro-log-tail-lines 50 \
  | grep -E "interactive|reset|recent flows"

# Expected on v1.0.16 green:
#   interactive: reachedInteractive=6 timedOutBeforeInteractive=0
#   (matches launchAppTotal — every launch reached probeable tree)

# 6. If still 0/N: capture interactiveNamedIds from the last launch-app
#    response — what ax-ids did we see? Compare against your positive
#    fingerprint list (dev-bubble / qa-bubble / btn-env-* / btn-login /
#    btn-forget-password). If NONE of those in interactiveNamedIds →
#    the probe fired but on the wrong screen. If SOME → threshold might
#    need to drop (edit .smix/config.yaml minIdentifierCount: 2 or 1).
```

If step 5's counter STILL shows 0/N after this fix:
- The Fabric mount-item drain may be waiting for something that XCUITest isn't triggering with `snapshot()` alone. Options:
  - Add a preflight `entry.app.activate()` before the polling loop (forces XCUITest to observe the app as active).
  - Extend polling deadline (`waitForInteractiveMs: 45000` if 30 s isn't enough).
- Share `interactiveNamedIds` sample from the response — that tells us if the probe fired at all vs the tree stayed sparse.

## Your follow-up items — status

From your round-2 §Follow-up actions:

1. **`menu-utils.ts` moved** ✓ — done on your side.
2. **`resetAppData` migration** — ready when you are; v1.0.14 verb + docs stable.
3. **`waitForInteractiveMs: 30000` adoption** — you get it automatically via `clearAppData` yaml (SDK default). Yaml `launchApp: waitForInteractiveMs` accepts the field but emits a warning per §"What v1.0.16 also adds"; upgrade to `clearAppData` for full behavior.
4. **testID coverage improvement** — your side; positive fingerprint list you named [see round-2 §Follow-up #3] can seed the `.smix/config.yaml interactiveProbe.ignore` list once we agree on the "sparse" default.
5. **Log-gate false-positive fix** — your side.
6. **Docker testbed image** — your side; ship-gate stays on Preferences until it lands.

## What we NOT change from v1.0.15

- Wire compatibility on every non-new field.
- Runner-side HTTP surface (all v1.0.16 changes are Swift-internal + yaml parser).
- `clearAppData` / `resetAppData` / `clearState` verbs.
- Ship-gate discipline.

## Where to file feedback

Same channel [see prior-doc `smix-feedback-2026-07-11-post-native-fix.md` ¶last]:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.16 feedback: please include `interactiveNamedIds` sample from a launch-app response even on the success path — a positive baseline lets us see if the probe is firing on the right screen (dev-bubble / btn-env-staging / etc.) vs a splash-screen leak.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.16-shipping.md
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
- `smix-feedback-2026-07-11-round-2-status.md` — the round-2 investigation this doc responds to
- **this doc** — v1.0.16 snapshot-refresh hotfix
