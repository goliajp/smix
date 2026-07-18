# smix v1.0.26 adoption guide for insight — using the new toolkit well

Date: 2026-07-13
From: smix maintainer (`claude@golia.jp`)
Companion to: `insight-v1.0.26-shipping.md` (release notes). This document is the HOW — how to wire the v1.0.20 → v1.0.26 capability arc into your qa-sim line so each feature earns its keep. Organized by what you're trying to do, not by version.

---

## 0. Upgrade (one correction from prior notes)

```bash
cargo install smix-cli --locked        # crate is smix-cli; binary is `smix`
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                         # → 1.0.26

# Swift runner changed in v1.0.26 (dynamic bundle-ignore) — cold rebuild:
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle com.focusai.app.mobile
# look for: "synced runner sources → 1.0.26"
```

Prior shipping notes said `cargo install smix`. That crate doesn't exist — you've clearly been working around it; `smix-cli` is canonical.

---

## 1. The selector strategy that fits your stack (iOS 26.5 + RN 0.86 Fabric)

Your environment drops `identifier`/`label` from the a11y tree unpredictably (the Fabric bridge issue — `elementTypeRaw != 1 && identifier == "" && label == ""` in any tree.json is the proof signature). The toolkit now assumes that reality. Recommended per-flow posture:

### 1a. Turn on auto-OCR once, delete boilerplate

```bash
# in your qa-sim runner env (you spawn smix via spawnSync):
SMIX_AUTO_OCR_FALLBACK=1
```

Then every bare-string assertion is automatically OCR-safe:

```yaml
# BEFORE (your current 5-line pattern, ×12 flows):
- extendedWaitUntil:
    visible:
      fallback:
        - text: 'Log in to Insight'
        - ocrText: 'Log in to Insight'
    timeout: 30000

# AFTER (identical runtime behavior under the env):
- extendedWaitUntil:
    visible: 'Log in to Insight'
    timeout: 30000
```

Regex-OR strings split correctly since v1.0.25: `visible: 'Log in to Insight|Device'` becomes `[Text(/A|B/i), OcrText('Log in to Insight'), OcrText('Device')]` — each OCR tier gets a real string. Your f971b245 explicit-fallback workaround can be reverted whenever convenient; both forms produce the same runtime shape.

**Keep explicit `fallback:` only where you want an `id:` tier first** — the env lift is `[text, ocrText]`; it can't invent your testIDs:

```yaml
# ids still deserve the explicit form (id tier is the cheapest + most stable):
- extendedWaitUntil:
    visible:
      fallback:
        - id: 'btn-log-in-to-insight'
        - text: 'Log in to Insight'      # or rely on env-lift and just add id
        - ocrText: 'Log in to Insight'
    timeout: 30000
```

### 1b. Order tiers by cost, always

`id` (~10 ms tree probe) → `text` (same probe, i18n-fragile) → `ocrText` (~500 ms Vision call). OCR last is not a style preference — every poll iteration pays the OCR price when earlier tiers miss.

### 1c. Tap flakes: the poll is already working for you

`tapOn` with any `ocrText` in the fallback polls the whole chain for `SMIX_TAP_OCR_POLL_MS` (default 3000 ms). Your Skip-flake workaround (`waitForAnimationToEnd` + `extendedWaitUntil` pre-gate before `tapOn Skip`) is now redundant — the tap itself rides out the mount race. Trim the pre-gates when you next touch those flows; each removal saves ~1–2 s per flow.

If a specific screen mounts slower than 3 s:

```bash
SMIX_TAP_OCR_POLL_MS=5000 bun test:e2e --scope bootstrap
```

---

## 2. Idempotent ceremonies — the `when.notVisible` pattern

You landed this in round 4; codifying the recommended shape since it's now your batch backbone:

```yaml
# enter-qa-mode.yaml — first flow pays the ceremony, flows 2-12 skip in ~1 probe
- runFlow:
    when:
      notVisible:
        fallback:
          - id: 'qa-bubble'
          - ocrText: 'QA'          # gate check fires OCR too (v1.0.24)
    file: qa-gate-passcode.yaml
```

Two facts worth exploiting:

1. **The gate selector supports the full selector table** — `fallback:` with OCR works inside `when:`, so the gate itself survives Fabric drops.
2. **Every skip is now visible in your logs** (v1.0.25): grep your batch stderr for `SKIPPED:` and you get the per-flow short-circuit audit for free:

```
STEP 3: runFlow qa-gate-passcode.yaml (conditional) → SKIPPED: runFlow when.notVisible visible=true ({ fallback=[...] }); skipped subflow qa-gate-passcode.yaml
```

If a batch regresses, check this FIRST — "ceremony unexpectedly re-ran" vs "ceremony unexpectedly skipped" discriminates env-state bugs from smix/selector bugs in one grep.

---

## 3. Triage workflow — use the artifacts in this order

When a flow fails, the fastest root-cause path with the current toolkit:

1. **Failure hint trace** (stderr): `L1 { id=… }: MISS; L2 { text=… }: MISS; L3 { ocr_text=… }: MISS` — which tiers were probed, which missed. OCR MISS at L3 = the text genuinely isn't rendered (or spelling/contrast); tree MISS + OCR HIT would have passed.
2. **Auto-captured PNG** (`.smix/timeouts/timeout-extendedWaitUntil-<ts>.png`): is the expected screen even rendered? Your "4-5 turns → 1 turn" observation is the intended loop.
3. **Auto-captured tree.json** (same basename): run the bridge-drop audit —
   ```bash
   jq '[.. | objects | select(.elementTypeRaw? and .elementTypeRaw != 1 and (.identifier // "") == "" and (.label // "") == "")] | length' \
     .smix/timeouts/timeout-extendedWaitUntil-*.tree.json
   ```
   Non-zero = Fabric dropped names on typed elements → app-side (add testIDs / a11y props, or lean on OCR). Zero + missing element = the element truly isn't in the tree → timing or navigation bug.
4. **Skipped audit** (§2 above) for conditional flows.
5. **`smix diagnostic dump`** post-batch: `lastInteractiveNamedIds` (did the probe land on your app or dev-launcher?), `recentFlows` retry attribution, `.ips` deltas.

### Batch-drift signal (your Ask 6, now on both platforms)

Wire `X-Tree-Snapshot-Wall-Ms` into runner.ts when you do the planned refactor. Reading it is one header on any `/tree` you already fetch:

- **Refresh-Count flat across polls** → snapshot pipeline stalled (restart runner, file to us with the log).
- **Wall-Ms trending up across scopes** (e.g. 250 ms → seconds) → OS a11y pipeline bogging down under sustained JS reloads; a sim reboot between scopes is the pragmatic reset, and the numbers give you the evidence to decide WHERE to put it.

---

## 4. Interactive-probe tuning (v1.0.26 changed your defaults — for the better)

The runner now always ignores the target bundle id dynamically, so your `.smix/config.yaml` only needs the things smix can't infer — the dev-launcher artifacts you identified in round 4:

```yaml
# .smix/config.yaml
interactiveProbe:
  minIdentifierCount: 3
  ignore:
    - SplashScreenLogo          # (also the bundled default)
    - house.fill                # expo-dev-launcher SF Symbols —
    - gearshape.fill            #  your round-4 lastInteractiveNamedIds
    - chevron.right             #  sample showed the probe firing on
    - arrow.trianglehead.2.clockwise.rotate.90   # dev-launcher UI
```

With those ignored, `launchAppReachedInteractive` stops counting dev-launcher chrome and only fires when YOUR surface mounts — making the counter mean what you always wanted it to mean. Verify by checking `lastInteractiveNamedIds` post-batch: it should show `dev-bubble` / `btn-log-in-to-insight`-class ids, not SF Symbols.

---

## 5. New escape hatch: `dispatch:` (know it exists, you likely don't need it yet)

```yaml
- tapOn:
    id: 'btn-login'
    dispatch: daemonProxy   # XCTRunnerDaemonSession synthesize
```

`daemonProxy` bypasses the XCUIElement gesture-recognizer chain so RN's `RCTTouchHandler` gets the raw touch. The default tap path already fires Pressable `onPress` on RN 0.86 — reach for this only if a future RN upgrade regresses tap delivery (it's the v4.0 G8 mechanism, now yaml-reachable, so the fix would be a one-line yaml change instead of a smix release). `dispatch: xcui` is for SwiftUI modal dismiss bindings — not applicable to your Fabric stack.

---

## 6. CI hygiene that's now cheap

- **Parse gate in pre-commit / CI**: `smix run --dry-run .devtools/qa/sim/flows/**/*.yaml` — validates every flow + `runFlow:` includes in milliseconds, no sim. Exit 2 on any parse error. Catches the yaml-shape class of regression before a 50-second batch does. Note: run it with the same `SMIX_AUTO_OCR_FALLBACK` value as production runs (the env changes parse output).
- **Batch stderr capture**: you flagged switching from `stdio: 'inherit'` to captured+teed — worth doing now that `SKIPPED:` lines and OCR traces carry real signal you'll want to grep post-hoc.
- **Fixture verify tails**: your `qa-sim-behavior-verification-plan.md` "chip-fire ≠ behavior verified" split is exactly right, and the public guide now documents the fixture-host contract you already implement (`06-fixtures.md`). The OCR fallback + `notVisible` gates + auto-capture are the pieces that make the `-verify.yaml` halves feasible — same selectors posture as §1.

---

## 7. Quick reference — env vars and where artifacts land

| Thing | Value / location |
|---|---|
| `SMIX_AUTO_OCR_FALLBACK` | `1` — bare strings lift to `[text, ocrText]`; `A\|B` splits per alternative |
| `SMIX_TAP_OCR_POLL_MS` | default `3000` — tapOn fallback poll budget when chain has OCR |
| Timeout captures | `--debug-output` dir if set, else `<cwd>/.smix/timeouts/`, else `~/.local/share/smix/timeouts/` |
| Retry attribution | `~/.local/share/smix/flow-attempts.json`, surfaced in `smix diagnostic dump` |
| Snapshot drift headers | `X-Tree-Snapshot-Refresh-Count` / `X-Tree-Snapshot-Wall-Ms` on every `/tree` (iOS + Android) |
| Skipped audit | stderr `STEP N: … → SKIPPED: <reason>` |
| Parse gate | `smix run --dry-run <files…>` (alias `--check`) |

## Where to file feedback

Same channel: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md`

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.26-adoption-guide.md
```
