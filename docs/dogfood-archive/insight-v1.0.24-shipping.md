# smix v1.0.24 — shipping notes for insight (round-3 Ask 8 root cause + 3 fixes)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12-round-3.md` — Ask 8 `runFlow.when.{commands,file}` + inputText silently no-ops
Prior: `insight-v1.0.23-shipping.md`

## TL;DR (3 lines per Q10)

- **Ask 8 root-cause traced + fixed in v1.0.24.** `runFlow.when.visible` predicate used tree-only `App::find`; `Selector::OcrText` inside a fallback was silently dropped by the tree resolver. Under iOS 26.5 + RN 0.86 Fabric a11y drop, `text:` missed and OCR was never fired → gate returned false → whole conditional body silently skipped → `inputText` never dispatched. Cursor blinking in the passcode field was gate autoFocus, not tapOn.
- **Fix (D1)**: `runFlow.when.visible` now goes through v1.0.23's shared `check_selector_visible` — same OCR-aware primitive as tapOn poll and scrollUntilVisible poll. Fast path preserved for OCR-free gates.
- **D2 workaround shipped too**: `runFlow.when.notVisible` inverse gate. Enables `notVisible: 'qa-bubble'` = "enter gate only if not already past it." Mutually exclusive with `when.visible` at parse.
- **D3 diagnostic upgrade**: Skipped reason now names the selector + evaluation outcome. Pre-v1.0.24 message told consumers nothing about WHY the conditional skipped.

## Round-3 report acknowledgment

Your empirical framing again drove the fix speed — the "STEP 6 ~0s" observation was the smoking gun. That measurement itself is a pre-flight print artifact (all `STEP N/M` lines print BEFORE any step executes; wall time between them is milliseconds regardless of actual step duration), but combining it with the cursor-blinking screenshot ruled out both your hypotheses:

- Not "condition true + inputText silently no-op" (the ~0s outer would be 45s+ if inner steps ran)
- Not "condition false + no tap" (the cursor is in the passcode field)

The synthesis: **`when.visible` evaluated FALSE**, so the inner steps NEVER RAN. The cursor in the passcode field is the `<PasscodeInput autoFocus>` mounting the gate — no tap required. The gate autoFocus + the false when.visible + tree-only find() dropping OCR together caused the silent skip.

That was the moment "same pattern, different call site" (your v1.0.22-round-2 framing on tapOn/scrollUntilVisible) turned into "same pattern, DIFFERENT VERB" (runFlow.when.visible). The fix belongs at the shared primitive — v1.0.23 already gave us `check_selector_visible`. This cycle rewires runFlow to consume it.

Also worth noting: **`--all` batch 0/12 → 7/12 with URL-scheme bypass** proves the v1.0.14 → v1.0.23 arc is doing its job. Your bypass workaround wasn't a defect — it was the fastest path to getting the app-code fixes empirically validated. The gate ceremony coverage catches up in v1.0.24 without you having to touch the bypass.

## D1 — `runFlow.when.visible` fires OCR when selector contains OcrText

### Root cause (concrete trace)

Pre-v1.0.24 dispatch for both `Step::RunFlowInline` and `Step::RunFlowConditional`:

```rust
let visible = match when_visible {
    None => true,
    Some(sel) => self.app.find(sel).await.unwrap_or(false),
};
```

`self.app.find(sel)` → `driver.find(sel)` → tree resolver via `resolve_selector_all`. That resolver's OcrText branch:

```rust
Selector::OcrText { modifiers, .. } => {
    // v5.19 c1 — adapter handles OcrText dispatch directly via
    // App::find_by_text_ocr + tap_at_norm_coord, bypassing the
    // resolver pipeline. Variant reaches resolver only when
    // adapter forgot to dispatch; treat same as LocalizedText
    // (compile modifiers; matches_base returns false).
    Self::compile_modifiers(modifiers, out)  // ← returns false at match time
}
```

So the OCR sub-selector silently returns "no match" from the tree resolver. For `Selector::Fallback`, the resolver walks each sub-selector; OCR ones silently fail; tree-based ones actually check. Result: `find(fallback: [text: "For internal testers only", ocrText: "For internal testers only"])` under iOS 26.5 + Fabric a11y drop returns FALSE because `text:` misses (Fabric drop) and OCR silently reports miss (it wasn't fired).

### v1.0.24 fix

`Step::RunFlowInline` and `Step::RunFlowConditional` now route through new adapter method `evaluate_run_flow_gate`:

```rust
async fn evaluate_run_flow_gate(&mut self, when_visible, when_not_visible) -> (bool, String) {
    if let Some(sel) = when_visible {
        // v1.0.23's shared check_selector_visible: fires OCR when
        // selector contains OcrText anywhere.
        let visible = self.check_selector_visible(sel).await.unwrap_or(false);
        return (visible, format!("when.visible={visible} ({describe})"));
    }
    if let Some(sel) = when_not_visible {
        let visible = self.check_selector_visible(sel).await.unwrap_or(false);
        return (!visible, format!("when.notVisible visible={visible} ({describe})"));
    }
    (true, "unconditional".to_string())
}
```

`check_selector_visible` was introduced in v1.0.23 D2 as the shared primitive for "probe once via tree + OCR." Now consumed by:
- v1.0.22 D1 `wait_for_visible_with_ocr` (extendedWaitUntil)
- v1.0.23 D1 tapOn poll body
- v1.0.23 D2 scrollUntilVisible poll body
- **v1.0.24 D1** `evaluate_run_flow_gate` (runFlow.when.visible / when.notVisible)

Every OCR-in-verb ask now composes the same primitive — no new patches per verb.

### Empirical impact on your yaml

Your Shape A (inline `commands:`) from round-3:

```yaml
- runFlow:
    when:
      visible:
        fallback:
          - text: 'For internal testers only'
          - ocrText: 'For internal testers only'
    commands:
      - tapOn: { id: 'qa-passcode' }
      - waitForAnimationToEnd: 1500
      - inputText: '0429'
      - extendedWaitUntil:
          visible:
            fallback:
              - id: 'qa-bubble'
              - ocrText: 'QA'
          timeout: 45000
      - runFlow: launch-warm.yaml
```

Pre-v1.0.24: `text:` misses under Fabric drop → gate returns false → whole 5-step body skipped → passcode empty → outer Landing wait times out at 30 s. STEP 6 wall ~0s = conditional never entered.

Post-v1.0.24: `text:` misses → OCR fires on `ocrText: 'For internal testers only'` → gate returns TRUE → body runs → tapOn qa-passcode → inputText '0429' → qa-bubble wait → launch-warm. Outer flow completes.

## D2 — `runFlow.when.notVisible` inverse gate

You asked for this as the workaround shape in case D1 was intrusive. It shipped alongside D1 as a first-class feature.

```yaml
- runFlow:
    when:
      notVisible:              # ← NEW — fires only when NOT visible
        id: 'qa-bubble'
    file: enter-qa-mode.yaml
```

Semantics: enter the conditional only if the selector is NOT visible. Same OCR-aware `check_selector_visible` under the hood — you can spell `notVisible: {fallback: [id, ocrText]}` and OCR fires for the check.

Idempotency pattern this enables:
```yaml
# Once per batch: enter QA mode if we're not already there.
- runFlow:
    when:
      notVisible:
        id: 'qa-bubble'         # panel already open? skip the ceremony
    file: enter-qa-mode.yaml
```

Mutually exclusive with `when.visible` at parse time — both set → clear parse error:
```
smix run: parse FAIL flow.yaml: invalid runFlow.when: `visible` and
`notVisible` are mutually exclusive; use one
```

## D3 — better `runFlow` Skipped diagnostic

Pre-v1.0.24 message: `runFlow when.visible=false; skipped inline body (5 steps)`.

Post-v1.0.24:
```
runFlow when.visible=false ({ fallback=[{ text="For internal testers only" }, { ocr_text="For internal testers only" }] }); skipped inline body (5 steps)
```

For `notVisible`:
```
runFlow when.notVisible visible=true ({ id="qa-bubble" }); skipped subflow enter-qa-mode.yaml
```

Consumer gets the selector's `describe_selector` form + evaluation outcome in one line. No more "the conditional skipped, why?" mystery.

## About Ask 9 (blank Landing after Skip) + Ask 10 (M/N chip nav)

Both explicitly flagged as app-side in your report, not smix asks. Filing acknowledgment: **the D2 auto-capture from v1.0.22 turning 3-5 turns of "what does the sim look like" into 1 turn is exactly what it was designed for**. Your ROI callout ("D2 has been the highest-ROI feature we've integrated across the v1.0.14-v1.0.23 arc") is the strongest single validation we've had of the round-1 delivery.

The app-side root-cause work stays on your branch. Any new smix asks that emerge from those investigations, file as usual — same channel.

## About the STEP counter observation

Your report inferred timing from adjacent STEP N/M lines in the runner log. That's a pre-flight print artifact: `entry.rs:277-279` prints ALL step lines in a tight loop BEFORE dispatch starts. Timestamps between adjacent lines are milliseconds regardless of actual step wall time.

To measure actual step timing, use `--debug-output <dir>` — every step writes `step-N-<verb>.json` with `wallMs` on completion. Alternatively, real-sim wall time surfaces on the `smix run` exit summary. But for the round-3 diagnosis, the pre-flight artifact reading was fine — it ruled out one hypothesis while corroborating another.

## Wire compatibility

- `Step::RunFlowConditional.when_not_visible` + `Step::RunFlowInline.when_not_visible` new fields, both `Option<Selector>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Pre-v1.0.24 serialized flows deserialize with `when_not_visible: None` = unchanged behaviour.
- `runFlow.when.notVisible` parser is additive on the accept-set. Any yaml that parsed before still parses.
- Runtime `evaluate_run_flow_gate` shared method centralizes gate evaluation. No wire / HTTP surface changes.
- CLI / other subsystems byte-identical to v1.0.23.

## Ship gate

- 3 new parser tests: `parse_run_flow_conditional_when_not_visible`, `parse_run_flow_inline_when_not_visible`, `parse_run_flow_when_visible_and_not_visible_together_rejects`.
- 66 total parser tests green (+3 new).
- 119 workspace test-result-ok buckets green (unchanged bucket count).
- CLI smoke on 3-step yaml with `when.notVisible` + `when.visible` + mutually-exclusive rejection — all parses match spec.
- Real-sim empirical validation pending on your next batch — you have the qa-gate ceremony yaml ready. Just re-run without the URL-scheme bypass; D1 will fire OCR on `ocrText: 'For internal testers only'` and the ceremony should complete.

## What v1.0.24 does NOT change

- v1.0.23 D1/D2/D3/D4 preserved.
- v1.0.22 D1/D2/D3 preserved.
- v1.0.21 iOS 26.5 UIAlertController promotion preserved.
- v1.0.20 D1/D2/D3 preserved.

## What v1.0.24 does NOT solve

- **Ask 9 (blank Landing after Skip)** — app-side per your diagnosis. `src/app/_layout.tsx` root-gate re-render.
- **Ask 10 (M/N chip nav)** — app-side per your diagnosis. `Linking.openURL` deeplink handler or route guard.

## Retest checklist (v1.0.23 → v1.0.24)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.24

# 2. Cold rebuild — this cycle changes the CLI adapter runtime;
#    Swift runner unchanged (byte-identical tarball vs v1.0.23).
smix runner up <UDID> --bundle <BUNDLE>

# 3. Restore the qa-gate ceremony in a conditional runFlow
- runFlow:
    when:
      notVisible:                      # ← v1.0.24: enter only if not past
        id: 'qa-bubble'
    commands:
      - openLink: 'insight:///(auth)/qa-gate'
      - extendedWaitUntil:             # settle for gate render
          visible:
            fallback:
              - text: 'For internal testers only'
              - ocrText: 'For internal testers only'
          timeout: 15000
      - tapOn: { id: 'qa-passcode' }
      - waitForAnimationToEnd: 500     # v1.0.18 D2
      - inputText: '0429'
      - extendedWaitUntil:
          visible:
            fallback:
              - id: 'qa-bubble'
              - ocrText: 'QA'
          timeout: 45000

# 4. Full batch
bun test:e2e --all -- --metro-log /tmp/metro.log

# 5. If any runFlow still skips unexpectedly, the new diagnostic
#    shows exactly what was checked. Grep the STDERR for:
grep "runFlow.*skipped" /tmp/smix-stderr.log
# → "runFlow when.visible=false ({ fallback=[...] }); skipped..."
#   tells you which layers were checked and that OCR was fired.
```

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.24 feedback:
1. Does the qa-gate ceremony in `runFlow.when.notVisible: 'qa-bubble'` (or `when.visible: fallback: [text, ocrText]`) now complete end-to-end?
2. Does the `--all` batch improve past 7/12 with the ceremony re-wired (no URL-scheme bypass)?
3. Any Skipped-with-diagnostic lines in your batch stderr that suggest a further OCR gap?

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.24-shipping.md
```

## Prior chain

- ... (v1.0.14 → v1.0.23; see prior shipping docs)
- `insight-v1.0.23-shipping.md`
- `smix-feedback-2026-07-12-round-3.md` — the round-3 report this doc responds to
- **this doc** — v1.0.24 runFlow.when OCR + notVisible + Skipped diagnostic
