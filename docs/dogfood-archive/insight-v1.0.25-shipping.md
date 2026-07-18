# smix v1.0.25 — shipping notes for insight (round-4 Ask 11 + D3 stderr emit)

Date: 2026-07-13
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-13.md` — round-4 empirical + Ask 11 regex-vs-OCR + D3 stdio-inherit note
Prior: `insight-v1.0.24-shipping.md`

## TL;DR (3 lines per Q10)

- **Both round-4 asks land in v1.0.25.** D1 fixes Ask 11: `SMIX_AUTO_OCR_FALLBACK=1` now splits bare-string `'A|B'` on top-level `|` and emits one `OcrText` tier per alternative — Vision gets real strings, not literal `"A|B"`. D2 fixes the D3-emit-under-stdio-inherit note: Skipped reasons now land on stderr as `STEP N: <verb> → SKIPPED: <reason>`.
- **Congrats on the 8/12 real-gate empirical.** The v1.0.24 D1/D2 restoration of the ceremony via `runFlow when.notVisible: qa-bubble file: qa-gate-passcode.yaml` is the strongest single validation of the v1.0.14 → v1.0.24 arc.
- **launch-chain unblock expected**: Ask 11 was launch-chain's regex-broken root selector. Rewrite launch-warm.yaml back to bare-string `visible: 'Log in to Insight|Device'` under `SMIX_AUTO_OCR_FALLBACK=1` and it should now parse to `fallback: [Text(/A|B/), OcrText('Log in to Insight'), OcrText('Device')]`. Your explicit-fallback workaround (f971b245) can stay — same runtime shape either way.

## Round-4 report acknowledgment

Your empirical framing — "8/12 with real gate coverage restored, 3 of 4 residuals app-side, the one that isn't is a docs question" — is the report shape we all wish for. Concrete numbers + causal analysis + workaround already committed. Every round-N report has raised the bar.

Two notes:

**On Option A vs Option B for Ask 11.** You wrote:
> Two shapes, either would be fine — this is a docs / semantics question we want smix's take on.

Went with **Option B (split on top-level `|`)** because:
1. Users writing `visible: 'A|B'` clearly want OR-semantics both on tree and OCR — Option A would silently drop OCR safety for those users.
2. Regex `A|B` at the tree layer is FASTER than two independent tree probes — keeping the single regex tier at layer 0 preserves that.
3. The failure mode Option A protects against (accidental regex meta in a plain phrase like `visible: 'v1.2.3'`) is empirically vanishingly rare — nobody writes version numbers as `visible: ...`. The common case is intentional `A|B` OR.

Option A stays as a fallback (see Ship gate §D1 below): if a string contains other regex meta but no `|`, `text_to_pattern` treats it as literal Text; OCR tier gets that same literal. No misapplied regex on OCR.

**On the D3 stdio-inherit observation.** You called this out as "not blocking; the D2 auto-capture path covers root-cause turnaround." Fair, but that's a bar we should meet — v1.0.24 D3 was designed to be visible on every batch's stderr, and it wasn't. D2 emit is the fix. Should have been part of v1.0.24; catching it in your review turned a stderr grep gap into a v1.0.25.

## D1 — `SMIX_AUTO_OCR_FALLBACK` splits regex-OR per alternative

### Root cause

Pre-v1.0.25 bare-string lift (v1.0.23 D4):

```rust
Selector::Fallback {
    fallback: vec![
        Selector::Text { text: text_to_pattern(s), .. },     // /A|B/i  (regex, correct)
        Selector::OcrText { ocr_text: s.clone(), .. },       // "A|B"   (literal, WRONG)
    ]
}
```

`text_to_pattern` returns `Pattern::Regex("A|B", "i")` when the string contains `|`. Correct for the tree tier. But the OCR tier just copies `s` and hands it to Apple Vision, which searches for the LITERAL string `"A|B"` — pipe character and all. That literal is never on screen.

### v1.0.25 fix

`SMIX_AUTO_OCR_FALLBACK=1` bare-string lift now emits:

```
'A|B'      → fallback: [Text('/A|B/i'), OcrText('A'), OcrText('B')]
'A|B|C'    → fallback: [Text('/A|B|C/i'), OcrText('A'), OcrText('B'), OcrText('C')]
'Sign In'  → fallback: [Text('Sign In'), OcrText('Sign In')]        # unchanged
```

Layer 0 is unchanged: single tree probe with regex OR (`/A|B/i`) covers all alternatives cheaply. Layers 1+ are per-alternative OCR calls — Vision gets a real string each time.

### Split semantics

- **Top-level `|` only.** Character classes and escapes are respected:
  - `'A[|B]C'` → NOT split. Character class contains a pipe.
  - `'A\|B'` → NOT split. Pipe is escaped.
  - `'A|[B|C]'` → split at the top-level `|` only: `['A', '[B|C]']`.
- **Empty alternatives filtered.** `'|A|'` → `['A']`, not `['', 'A', '']`.
- **All-pipes degenerate.** `'||'` → falls back to the original string as a single alternative (`['||']`) so the caller still gets a probe.

### Concrete impact on launch-chain

Your round-4 report referenced this pre-v1.0.25 trace:

```
error: sdk: FAIL [TIMEOUT]:
extendedWaitUntil.visible({ fallback=[
  { text=/Log in to Insight|Device/i },
  { ocr_text="Log in to Insight|Device" }        # ← never on screen
] }) timed out after 45s
```

Post-v1.0.25 the same yaml lifts to:

```
{ fallback=[
  { text=/Log in to Insight|Device/i },          # tree: matches either
  { ocr_text="Log in to Insight" },              # OCR: real string 1
  { ocr_text="Device" }                          # OCR: real string 2
] }
```

Vision now has actual on-screen text to search. Your explicit-fallback workaround (f971b245) stays valid — same runtime shape. You can revert to bare-string form when you want the terseness back.

4 new parser tests locked (`parse_visible_bare_string_regex_or_splits_ocr_per_alternative`, `..._no_pipe_unchanged`, `..._three_alternatives`, `..._empty_alternatives_filtered`). 70 total parser tests green.

## D2 — Skipped diagnostic emitted to stderr per step

Pre-v1.0.25 the `RunStepReport::Skipped { reason }` string only surfaced in `--debug-output/step-N.json`. Under `stdio: inherit` consumers (your `spawnSync(SMIX_BIN, ..., { stdio: 'inherit' })` qa-sim runner) the diagnostic was invisible.

Fix — at the end of every step, if the outcome is `Skipped`, emit:

```
STEP N: <verb-summary> → SKIPPED: <reason>
```

to stderr. Non-Skipped outcomes stay quiet (Ok / ExpandedSubflow / errored) — no noise for the happy path.

Under your flow-2-through-12 (each with `runFlow when.notVisible: qa-bubble` short-circuiting), consumers now see:

```
STEP 3: runFlow qa-gate-passcode.yaml (conditional) → SKIPPED: runFlow when.notVisible visible=true ({ id="qa-bubble" }); skipped subflow qa-gate-passcode.yaml
```

Grep-friendly: `grep 'SKIPPED:' /tmp/batch.log` gives you every short-circuit reason across the batch.

## Wire compatibility

- D1 shape change only affects yaml parsed under `SMIX_AUTO_OCR_FALLBACK=1`. Env-off (or pre-v1.0.25) parses unchanged.
- D2 stderr emit only fires on Skipped outcomes. Silent for Ok / ExpandedSubflow / errored — no noise for the happy path.
- CLI / Rust wire types / other subsystems byte-identical to v1.0.24.

## About the residuals (per your round-4 report)

- **launch-chain race**: Ask 11 fix (D1) is the root cause; v1.0.25 unblocks the bare-string form. Your workaround yaml also works — pick whichever ergonomics you prefer.
- **L-merlin verify-tail race**: app-side per your diagnosis (panel close animation races openLink). No smix action.
- **M-push / N-sharing chip deeplinks**: app-side per your diagnosis (Universal Links entitlement not configured on sim build). No smix action.

The three-of-four app-side hit ratio on Phase E's design-goal shape ("chip fires app-side navigation the sim can't complete, assertion catches the divergence") is exactly what you built Phase E for. Working as intended.

## Ship gate

- 70 parser tests green (+4 D1 tests; Mutex-serialized env-touching helper reused from v1.0.23 D4).
- 119 workspace test-result-ok buckets green (unchanged bucket count).
- CLI smoke on `visible: 'Log in|Device'` under `SMIX_AUTO_OCR_FALLBACK=1` — dry-run parses cleanly to the 3-tier fallback as spec.

## v1.0.25 does NOT change

- v1.0.24 D1/D2/D3 preserved.
- v1.0.23 D1/D2/D3/D4 preserved (D4 shape change is additive; non-pipe strings still get 2-tier fallback).
- v1.0.22 D1/D2/D3 preserved.
- v1.0.21 iOS 26.5 UIAlertController promotion preserved.

## Retest checklist (v1.0.24 → v1.0.25)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.25

# 2. Cold rebuild — this cycle changes the CLI adapter runtime;
#    Swift runner unchanged (byte-identical tarball vs v1.0.24).
smix runner up <UDID> --bundle <BUNDLE>

# 3. Revert your Ask 11 workaround if you want the terseness back
#    (or keep the explicit form — both produce the same runtime shape now)
- extendedWaitUntil:
    visible: 'Log in to Insight|Device'         # ← works again under D4 env
    timeout: 45000
# Under SMIX_AUTO_OCR_FALLBACK=1 this parses to:
#   fallback: [
#     Text('/Log in to Insight|Device/i'),
#     OcrText('Log in to Insight'),
#     OcrText('Device'),
#   ]

# 4. Full batch — expect 9/12 with launch-chain moving to green
bun test:e2e --all -- --metro-log /tmp/metro.log

# 5. Look for D2 Skipped diagnostic in stderr
grep 'SKIPPED:' /tmp/batch.log
# → STEP 3: runFlow qa-gate-passcode.yaml (conditional) → SKIPPED: runFlow when.notVisible visible=true ({ id="qa-bubble" }); ...
```

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.25 feedback:
1. Does `visible: 'Log in to Insight|Device'` under `SMIX_AUTO_OCR_FALLBACK=1` now unblock launch-chain?
2. Do you see D2 Skipped lines in your qa-sim runner stderr under `stdio: inherit`?
3. Any edge cases in the `|`-split behavior (character classes, escapes, unusual regex constructs) that surface as false positives / negatives?

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.25-shipping.md
```

## Prior chain

- ... (v1.0.14 → v1.0.24; see prior shipping docs)
- `insight-v1.0.24-shipping.md`
- `smix-feedback-2026-07-13.md` — the round-4 report this doc responds to
- **this doc** — v1.0.25 D4 regex-OR split + Skipped-to-stderr
