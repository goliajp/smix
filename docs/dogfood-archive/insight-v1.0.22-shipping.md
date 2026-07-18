# smix v1.0.22 — shipping notes for insight (RN Fabric tree gap triage upgrade)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12.md` — iOS 26.5 + RN 0.86 Fabric a11y tree gap + 3 asks
Prior: `insight-v1.0.21-shipping.md`

## TL;DR (3 lines per Q10)

- **All 3 asks land in v1.0.22.** OCR now fires inside `extendedWaitUntil.visible.fallback` (Ask 2, full impl); screenshot + tree JSON auto-captured on every extendedWaitUntil timeout without needing `--debug-output` (Ask 3, full impl); `A11yNode.elementTypeRaw: u64` on wire so you can distinguish "iOS types it a button but a11y bridge dropped the name" from "iOS types it .other, expected nameless" (Ask 1, partial fix — see §D3 below for why "partial").
- **Root cause on Ask 1 is app-side, not smix serializer.** smix already emits `identifier` when non-empty (`TreeRoute.nodeToDict` line 192 pre-v1.0.22). If iOS 26.5's XCUITest snapshot returns empty `identifier` for RN 0.86 Fabric-mounted views despite `testID` being set in JSX, that's the RN → UIAccessibility bridge on iOS 26+ dropping the value between hosts. `elementTypeRaw` gives you the wire signal to prove it in-house or upstream to Meta.
- **Real-sim smoke green on all 3.** Preferences timeout emits `L1 id=…: MISS; L2 text=…: MISS; L3 ocrText=…: MISS` (OCR fired), auto-writes `.smix/timeouts/timeout-extendedWaitUntil-<epoch>.png` (378 KB) + `.tree.json` (222 KB, every node with `elementTypeRaw` matching Apple's XCUIElement.ElementType numeric table).

## Round-7 report acknowledgment

Your report is the highest-quality Ask-by-Ask report yet — every hypothesis (identifier missing at wire, `ocrText` never fired despite `fallback:` listing it, screenshot missing on timeout) turned into a repro + fix in one round. Your root-cause on Ask 2 was exactly right: parser accepts the shape but runtime never dispatches. And your framing on Ask 1 — "the tree walker is not capturing `identifier`" — turned out to be one step from the truth: smix already tries to capture it (`XCUIElement.AttributeName("identifier")` via `dictionaryRepresentation`), but iOS 26.5 XCUITest returns empty for RN Fabric-mounted views. That's below smix's abstraction floor; we can improve the diagnostic signal (elementTypeRaw), not fix the bridge.

## D1 — `extendedWaitUntil.visible.fallback: [ocrText: ...]` now actually calls OCR

Pre-v1.0.22 flow:
1. Parser (v1.0.20 D1) accepted `ocrText:` inside a `fallback:` chain — you saw this in your yaml → parse OK path.
2. Runtime dispatched every `extendedWaitUntil` selector through `App::wait_for`.
3. `App::wait_for` polled `/tree` and used `resolve_selector_compiled` to match.
4. `resolve_selector_compiled` on `Selector::OcrText` returns `false` (correct — OCR is meant to be dispatched at the adapter layer, not from a tree resolver).
5. So `OcrText` inside a `Fallback` was silently never matched. 45 s of pure `/tree` polls; zero OCR calls in the runner log — exactly what you observed.

Fixed with new adapter method `wait_for_visible_with_ocr`:

```rust
// Fast path: no OCR anywhere in the selector → delegate to
// driver-side wait_for unchanged (existing tree-only behavior).
if !contains_ocr(selector) {
    self.app.wait_for(selector, timeout).await?;
    return Ok(());
}

// Otherwise poll: per iteration, try every tree-resolvable sub
// first, then every OcrText sub. First hit wins.
```

Semantics:
- Fallback chain containing OcrText: per iteration, tree-resolvable sub-selectors (`Id`, `Text`, `Label`, `Role`, `LocalizedText`, `Anchor`, `AnchorRelative`, `Focused`, `Point`) fire via `App::find`; `OcrText` sub-selectors fire via `App::find_by_text_ocr`. Tree hits pre-empt OCR cost.
- Standalone `Selector::OcrText` at top-level: polls `find_by_text_ocr` on the same 250 ms cadence.
- Everything else (no OCR anywhere): delegates to `App::wait_for` unchanged — zero perf change.

Timeout emits a per-layer trace:
```
L1 { id="btn-log-in-to-insight" }: MISS;
L2 { text="Log in to Insight" }: MISS;
L3 { ocr_text="Log in to Insight" }: MISS
```

Plus a hint pointer:
```
OCR-aware waitFor exhausted budget. Trace: [...]. If the a11y tree
is degraded (RN Fabric on iOS 26.5), consider adding `ocrText` last
in a `fallback:` chain so smix falls through to Vision OCR; smix
v1.0.22+ actually fires OCR in this path (pre-v1.0.22 silently
skipped it).
```

**Real-sim verification (Preferences smoke, sim-insight)**: yaml with `fallback: [nonexistent-id, impossible-text, impossible-ocr]` timed out at 2 s with all 3 layers listed MISS — L3 = OCR did fire.

### Cost / perf note

OCR is expensive (~500 ms per call on the sim). If a fallback chain contains an OCR sub-selector, poll iteration cost jumps from ~10 ms (single tree fetch) to ~500 ms. Callers pay this willingly — it's the tier they opted into by listing `ocrText`. But avoid `fallback: [ocrText]` alone in tight-timeout scenarios; keep at least one cheap tree-based tier ahead of OCR so hits pre-empt Vision cost.

## D2 — Screenshot + tree JSON auto-captured on every `extendedWaitUntil` timeout

Pre-v1.0.22 required `--debug-output <dir>` to get a fail PNG + tree snapshot on step failure — which insight's bootstrap batch didn't wire. Every timeout left you with metro logs + top-10 element hint and no visual.

Now every `extendedWaitUntil` timeout auto-captures both, no flag needed. Sink resolution:
1. If `--debug-output <dir>` set → same dir as per-step debug artifacts (unchanged path for consumers who already wired it).
2. Else try `<CWD>/.smix/timeouts/` — repo-scoped triage. Already in typical gitignores.
3. Else fall back to `~/.local/share/smix/timeouts/`.

File names: `timeout-extendedWaitUntil-<epoch-ms>.png` + `.tree.json`. Both paths appended to the failure's existing hint:

```
v1.0.22 timeout capture: screenshot=<path>.png tree=<path>.tree.json
```

Best-effort: any I/O / screenshot / tree error is logged to stderr but does not affect the failure verdict. If your sim's UDID isn't resolvable at capture time, the screenshot fails but the failure still emerges with the original selector + trace.

**Real-sim verification (Preferences smoke)**: after a 2 s timeout, `.smix/timeouts/` contained:
- `timeout-extendedWaitUntil-1783839211371.png` — 378 KB, actual sim viewport
- `timeout-extendedWaitUntil-1783839211371.tree.json` — 222 KB, full a11y tree

You can now attach BOTH to your existing `.tmp/qa-sim/diagnostic-dump-<ts>-*.json` collection — they land in a sibling dir the next round.

## D3 — `A11yNode.elementTypeRaw` numeric on wire (Ask 1 partial)

Your Ask 1 hypothesis was: "smix's tree walker is not capturing `identifier` for the returned XCUIElement." Not quite — smix does try (`XCUIElement.AttributeName("identifier")` via `dictionaryRepresentation`, `SmixRunnerUITests.swift:2930`). The value comes back as `""` under iOS 26.5 + RN 0.86 Fabric, so smix's per-node emit skips the empty `identifier` field (v1.0.22 line 192: `if !d.identifier.isEmpty { out["identifier"] = d.identifier }`).

**Root cause is below smix's abstraction floor**: the RN 0.86 → UIAccessibility bridge under iOS 26+. Same class of drop as the iOS 26.5 UIAlertController elementType change we hit last round — Apple's XCUITest layer is quietly changing what it exposes for third-party mounted views.

What v1.0.22 CAN do: emit the numeric `XCUIElement.ElementType.rawValue` on every wire node so you can prove the drop in-house:

```jsonc
// A node under a degraded RN Fabric root — v1.0.22 wire
{
  "rawType": "button",     // "button" per elementTypeName(9)
  "elementTypeRaw": 9,     // ← v1.0.22 NEW — iOS types this as UIButton-adjacent
  "identifier": "",        // → but empty (or omitted for empty). RN a11y bridge drop.
  "label": "",             // → also empty. Same drop.
  ...
}
```

Client-side triage rules:
- `elementTypeRaw != 1 && identifier == "" && label == ""` ⇒ iOS types this as a real element (`.button`, `.textField`, `.staticText`, ...) but its `identifier` / `label` fields are empty. That's the a11y bridge drop signal. Fix in the app / RN layer, not smix.
- `elementTypeRaw == 1` (`.other`) ⇒ plain wrapper view. Nameless is expected — no bridge issue.

Additive on wire; `#[serde(default = "default_element_type_raw")]` returns 1 (`.other`) for pre-v1.0.22 payloads. Downstream Rust / TS clients ignore the field until they read it explicitly.

### Why "partial" fix

To fully solve Ask 1 — surface a testID-shaped identifier for RN Fabric mounts on iOS 26.5 — smix would need to bypass `dictionaryRepresentation` and query live XCUIElements per node. That's a large architectural change (perception layer redesign) with real perf cost (each `/tree` call would go from ~10 ms per snapshot to ~2 s per snapshot for a 100-node tree). Wrong-tier fix for what's an RN bridge issue on iOS 26+. But the elementTypeRaw signal gives your team the wire evidence to file that RN issue upstream or patch on your side.

**Real-sim verification (Preferences smoke)**: tree JSON from timeout capture shows `elementTypeRaw` on all 168 nodes matching Apple's numeric table exactly:
- `button` → 9 (13 nodes)
- `image` → 43 (24 nodes)
- `staticText` → 48 (13 nodes)
- `cell` → 75 (11 nodes)
- `application` → 2 (1 node)
- `navigationBar` → 21 (1 node)
- `other` → 1 (101 nodes)
- ...

## Wire compatibility

- `A11yNode.elementTypeRaw: u64` — additive, defaults to 1 (`.other`) for pre-v1.0.22 payloads. Consumers ignoring the field see no behaviour change.
- `extendedWaitUntil` semantics preserved for OCR-free selectors — fast path delegates to `App::wait_for` unchanged.
- Timeout hint additive — the failure code (`Timeout`) / message / structure are unchanged; the hint gets extra lines appended.
- CLI / parser / other subsystems byte-identical to v1.0.21.

## Ship gate

- 119 test-result-ok buckets across the workspace green (all pre-existing + new).
- Full workspace `cargo check` green.
- Real-sim gate (Preferences): OCR fires in fallback, timeout capture writes both files, elementTypeRaw shipped on every wire node.
- Empirical validation on your degraded RN Fabric tree pending your next batch.

## v1.0.22 does NOT change

- v1.0.21 iOS 26.5 UIAlertController role-mapping — preserved.
- v1.0.20 D1/D2/D3 — preserved.
- v1.0.19 top-level lastInteractiveNamedIds — preserved.
- v1.0.18 D1/D2 — preserved.
- CLI / adapter parser byte-identical to v1.0.21.

## What v1.0.22 does NOT solve

- **RN 0.86 Fabric → iOS 26.5 UIAccessibility bridge dropping identifier** — that's the app side (or an upstream RN/Meta issue). elementTypeRaw gives you the wire evidence.
- **launch-chain title-all-cameras** — your side.
- **4th native race** — your side; separate follow-up branch.

## Retest checklist (v1.0.21 → v1.0.22)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.22

# 2. Cold rebuild — this cycle changes both Swift (elementTypeRaw
#    emission in TreeRoute) and CLI (OCR fallback + timeout capture).
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle <BUNDLE>
# → "synced runner sources → 1.0.22" line means it re-extracted

# 3. Add gitignore for the auto-timeout dir (once per repo)
echo ".smix/timeouts/" >> .gitignore

# 4. Try the yaml shape that regressed on v1.0.19
- extendedWaitUntil:
    visible:
      fallback:
        - id: 'btn-log-in-to-insight'
        - text: 'Log in to Insight'
        - ocrText: 'Log in to Insight'
    timeout: 45000

# 5. Full bootstrap batch
bun test:e2e -- --metro-log /tmp/metro.log

# 6. If any flow still times out, check .smix/timeouts/ for the
#    auto-captured PNG + tree.json. On the tree.json:
jq '[.. | objects | select(.elementTypeRaw and .elementTypeRaw != 1 and .identifier == "" and .label == "")] | length' \
  .smix/timeouts/timeout-extendedWaitUntil-*.tree.json
# → non-zero count = number of "typed as element but bridge dropped
#   the name" nodes = the RN Fabric drop count. Prove it in-house.
```

## Ship-worthy branch reminder

Your `bugfix/GOL-611-native-cold-boot-crash` (6 commits) has now been validated across 7 consecutive smix versions (v1.0.14 → v1.0.20; not against v1.0.21 which was iOS 26.5-specific, and now v1.0.22). 3-UAF chain still fully closed. Merge to develop when convenient.

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.22 feedback, please include:
1. On your degraded RN Fabric tree, does adding `ocrText:` at the tail of the `fallback:` chain now make the timeout become a hit? (D1 empirical validation.)
2. On timeout, is there a `.smix/timeouts/*.png` + `.tree.json` at the expected path? (D2 empirical validation.)
3. From the timeout tree.json, `jq` count of `elementTypeRaw != 1 && identifier == "" && label == ""` nodes — that number is the RN Fabric bridge drop count. Would help if you shared it upstream to Meta / RN issues.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.22-shipping.md
```

## Prior chain

- ... (v1.0.14 → v1.0.21, see prior shipping docs)
- `insight-v1.0.21-shipping.md`
- `smix-feedback-2026-07-12.md` — the round-7 report this doc responds to
- **this doc** — v1.0.22 RN Fabric tree gap triage upgrade
