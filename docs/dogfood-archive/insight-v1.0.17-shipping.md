# smix v1.0.17 — shipping notes for insight (v1.0.16 crash hotfix)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12-v1.0.16-runner-crash.md` — the element-enumeration crash diagnosis
Prior: `insight-v1.0.16-shipping.md`

## TL;DR (3 lines per Q10)

- **v1.0.17 fixes the exact crash your round-3 report named.** Live `element(boundBy:)` enumeration replaced by walking the frozen `XCUIElementSnapshot.dictionaryRepresentation` in memory — same pattern the runner already uses elsewhere (`collectPopupNodes`, `FocusedIdentifier.find`). No XCUITest re-resolution during the walk → mid-iteration tree shrink can't crash the runner.
- **v1.0.16's help stands** — the snapshot-refresh fix is preserved. `snapshot()` still forces XCUITest to re-scrape the a11y hierarchy from scratch (Fabric mount-item-drain fix from round-2).
- **Zero yaml changes required.** Same `clearAppData` yaml, same 30 s SDK-defaulted `wait_for_interactive_ms`, no consumer migration.

## First things first — round-3 report acknowledgment

Your report was diagnostic gold. Reading `smix-feedback-2026-07-12-v1.0.16-runner-crash.md`:

- **Flow 1 (`force-update.yaml`) reached STEP 47/47 vs previous max 34** [see round-3 §Empirical batch results, Force-update row]. That's the full 47-step chain: `enter-qa-mode` subflow + QA gate + Reload/Skip ceremony + final `Log in to Insight` re-assertion. **v1.0.16 snapshot-refresh works** — the Fabric mount-item drain race is closed. Naming this progress separately from the crash is exactly the right framing.
- **`.ips` growth 36 → 36** [same section]. Zero native regression from the v1.0.15 → v1.0.16 upgrade. The three-UAF patch chain stays stable.
- **The crash log capture was precise.** `Failed to get matching snapshot: No matches found for Element at index 48 from input {…}` at line 2643 pinpointed exactly the code path. The tree-shrink observation ("input list shrunk to 7 elements: Window + 5 Others + SplashScreenLogo") named the mechanism.
- **Direction A vs Direction B in your fix suggestions** — you correctly identified Direction A as the low-risk choice, and even called out that Direction A alone might race against `query.count` fluctuation. Went with **Direction C**: walk the frozen snapshot dict directly (which your report §"Direction A is the low-risk choice" bullet #3 essentially described as "treat the query as a snapshot in itself"). Same reliability property, no dependency on ObjC exception semantics.

## What v1.0.17 does

Before (v1.0.16):

```swift
_ = try? entry.app.snapshot()
let query = entry.app.descendants(matching: .any)
let count = query.count
for i in 0..<count {
  let element = query.element(boundBy: i)  // XCTest-lazy → hard-fail if tree shrank
  let id = element.identifier
  ...
}
```

After (v1.0.17):

```swift
guard let snap = try? entry.app.snapshot() else { return [] }
var ids: [String] = []
var enumerated = 0
collectInteractiveIds(
  snap.dictionaryRepresentation,   // frozen in-memory tree — no XCUITest re-resolution
  ignore: probeConfig.ignore,
  ids: &ids,
  enumerated: &enumerated,
  cap: 200                          // pathological-tree stall guard
)
return ids
```

Where `collectInteractiveIds` is a recursive walk over `[XCUIElement.AttributeName: Any]` — same shape/pattern the runner uses for `collectPopupNodes` (modal alert button collection since v1.4) and `FocusedIdentifier.find` (keyboard focus detection since v5.1).

- `snapshot()` is still called each polling iteration. That's the v1.0.16 fix for RN Fabric mount-item drain; it stays.
- The returned `XCUIElementSnapshot` is a frozen in-memory object. `.dictionaryRepresentation` gives us `[XCUIElement.AttributeName: Any]` we walk via children key. No XCUITest state involved; no lazy resolution; no assertion possible.
- If the app's tree shrinks between one polling iteration and the next, `try? entry.app.snapshot()` on the next iteration just returns the new (smaller) snapshot — no crash.
- If `snapshot()` itself throws (rare, but happens on driver disconnect), we `guard let` early return `[]` for this iteration and try again on the next 500 ms tick.

## Stress test in v1.0.17 ship gate

Beyond the baseline Preferences smoke that v1.0.15 and v1.0.16 both passed, this release specifically tested the tree-shrink case:

```
# 3 rapid terminate+launch cycles — the exact pattern your round-3 report described
# (stopApp+openLink dev-launcher between phases causes mid-iteration tree collapse)
for i in 1..3:
  POST /session/terminate-app
  POST /session/launch-app  waitForInteractiveMs:15000
  → reachedInteractive:true (all 3 cycles)

Post-cycles /health → 200 OK  (runner alive)
```

v1.0.16 in the same scenario would have crashed after 1-2 cycles (the second launch's polling would race the shrinking tree from the terminate). v1.0.17 walks the frozen snapshot, so tree state at query time is irrelevant.

## Zero yaml changes required

Same as v1.0.16. Your `launch-fresh.yaml` shape unchanged:

```yaml
- clearAppData:
    launchArgs:
      - '-EXInternalMetroPort'
      - '8081'
    launchEnv:
      EX_DEV_CLIENT_METRO_URL: 'http://localhost:8081'
- clearKeychain
- openLink: 'exp+focus-ai-app://expo-development-client/?url=http%3A%2F%2Flocalhost%3A8081'
- openLink: 'insight://dev-mutate?action=env&value=staging'
- extendedWaitUntil:
    visible: { id: 'dev-bubble' }
    timeout: 90000
```

`clearAppData` still routes through the session-scoped launch with SDK-defaulted 30 s `wait_for_interactive_ms`; the polling loop now walks the snapshot instead of the live query. No consumer migration.

## What v1.0.17 does NOT change

- Wire compatibility on every field. No wire changes at all.
- Runner-side HTTP surface. All v1.0.17 changes are Swift-internal to the interactive-probe loop.
- v1.0.14/v1.0.15/v1.0.16 features: `resetAppData`, `--metro-log`, `--retry`, `AppUnavailableReason`, snapshot-refresh, launchApp `waitForInteractiveMs` marker — all preserved.
- Ship-gate discipline — Preferences smoke + insight canary post-publish.

## Retest checklist (v1.0.16 → v1.0.17)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.17

# 2. Cold rebuild against v1.0.17 runner
rm -rf .smix/runner/derived-data-*
smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.focusai.app.mobile
# → "runner up: … (runner v1.0.17)"

# 3. Full bootstrap (yaml unchanged from v1.0.16)
bun test:e2e -- --metro-log /tmp/metro.log

# 4. Verify runner survives every flow
grep "test_runForever] : Failed" .smix/runner/runner-<UDID>.log | wc -l
# → 0

# 5. Verify interactive counters populate
smix diagnostic dump --metro-log /tmp/metro.log --metro-log-tail-lines 50 \
  | grep -E "interactive|reset|recent flows"

# Expected on v1.0.17 green:
#   interactive: reachedInteractive=6 timedOutBeforeInteractive=0
#   (matches launchAppTotal — every launch reached probeable tree,
#    without the runner dying in the middle)

# 6. Capture interactiveNamedIds sample — your predicted list was
#    dev-bubble + btn-login. Confirm which named IDs actually appeared.
```

## What we NOT change from v1.0.16

- Snapshot-refresh per iteration (v1.0.16 D1) preserved.
- `waitForQuiescenceIncludingAnimations` still not called (`SmixQuiescenceSwizzle.m` continues to no-op it).
- `.smix/config.yaml interactiveProbe:` schema unchanged.
- `launchApp: { waitForInteractiveMs }` yaml marker still emits the warning; `clearAppData` remains the recommended path.

## Answering your prediction from round-3

> "Prediction: `dev-bubble` and `btn-login` will show. Manual cold-boot (bypassing smix) renders the full Landing screen with those buttons."

Once the runner survives the batch, please capture `interactiveNamedIds` from the last launch-app response and report which of `dev-bubble` / `qa-bubble` / `btn-env-staging` / `btn-login` / `btn-forget-password` showed up. That tells us:

- **If matches your prediction (dev-bubble + btn-login)**: interactive probe is firing on the right screen; fingerprint list works as designed.
- **If only splash / dev-launcher artifacts (SplashScreenLogo, etc.)**: the config's ignore list is doing its job of filtering these out, but the app hasn't rendered the auth-landing state by the time the probe fires — extend `waitForInteractiveMs` to 45 s or 60 s.
- **If empty / very sparse**: your app's a11y coverage on the auth-landing is thinner than the fingerprint list implies — the testID improvements you flagged in round-2 §Follow-up #4 are the fix.

## What v1.0.17 does NOT solve (yet)

- **Full `launchApp: waitForInteractiveMs` routing** — still emits the warning; consumer-visible fix requires unifying the two launch pathways in a follow-up.
- **Docker testbed image** — your side; ship-gate stays on Preferences smoke.
- **Docker `--corpus-dir` wire-in for ship gate** — depends on Docker image landing.

## Where to file feedback

Same channel [see prior-doc `smix-feedback-2026-07-11-post-native-fix.md` ¶last]:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.17-shipping.md
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
- `smix-feedback-2026-07-12-v1.0.16-runner-crash.md` — the round-3 report this doc responds to
- **this doc** — v1.0.17 snapshot-walk hotfix
