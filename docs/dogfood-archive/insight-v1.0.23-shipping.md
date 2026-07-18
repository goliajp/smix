# smix v1.0.23 — shipping notes for insight (round-2 4 asks all landed)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12-round-2.md` — v1.0.22 empirical + 4 new asks
Prior: `insight-v1.0.22-shipping.md`

## TL;DR (3 lines per Q10)

- **All 4 round-2 asks land in v1.0.23.** D1 tapOn implicit poll with OCR (`SMIX_TAP_OCR_POLL_MS` default 3000 ms). D2 scrollUntilVisible fires OCR between swipes. D3 `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` headers on /tree. D4 `SMIX_AUTO_OCR_FALLBACK=1` bare-string auto-lift.
- **Congrats on the v1.0.22 empirical win.** 3/3 pass on OCR fallback yaml vs 0/3 on v1.0.21 same yaml is the strongest signal of round-1's payoff. `.smix/timeouts/*.png` 4→1 turn triage is exactly what D2 was designed for.
- **Real-sim gate on new bits**: D3 headers confirmed on Preferences smoke (`X-Tree-Snapshot-Refresh-Count: 2`, `X-Tree-Snapshot-Wall-Ms: 5921`). D4 env behavior locked by 4 parser tests + real-CLI smoke on 3 env configs (off / on / explicit-off).

## Round-2 report acknowledgment

Your round-2 report was again the highest-signal report we've had. The empirical framing — batch-by-batch pass/fail table, per-yaml OCR trace ("L1 MISS ... L2 MISS ... L3 HIT ~200 ms"), same auto-captured .png cited by absolute path — puts every claim on the ground.

Two clarifications:

**1. Ask 4 clarification (tapOn OCR wire).** Your report said "zero `/screenshot` or `/ocr` calls in the runner log during the 5-8 s tap window". This was a grep miss — smix's OCR endpoint is `POST /find-text-by-ocr` (v5.19 c1a), not `/ocr`. The recursion inside `run_tap` fallback DOES dispatch `Selector::OcrText` sub-selectors through the OCR branch (`runtime.rs:2708`), and the `L3 { ocr_text="Skip" } MISS: optional tapOn ocrText: OCR found no match for "Skip"` you see in the failure trace IS proof OCR fired. Good news: means it always was firing per your yaml, just missing at that specific moment.

**But your real diagnosis is right for a different reason**: `tapOn` is one-shot semantically. If OCR misses because the tap moment races the app's post-transition mount (Vision snapshots BEFORE target text appears), the tap fails immediately. That's the actual gap — no poll to close the race. v1.0.23 D1 closes it.

**2. Ask 6 clarification (freshness signal shape).** Your proposal wrapped body under `root` + added `snapshotAgeMs` sibling. Wire-body wrap would break every existing consumer parsing the top-level as an A11yNode. HTTP response headers are additive — pre-v1.0.23 consumers see zero change; new consumers add a single header read. Same signal, safer delivery.

## D1 — tapOn implicit poll window when fallback contains OcrText

Pre-v1.0.23 `run_tap` fallback was one-shot per attempt. On iOS 26.5 + RN 0.86 Fabric, the tap moment often races the app's post-transition mount:

```
t=0ms      user tap fires
t=0ms      run_tap: L1 id: MISS (identifier bridge drop)
t=0ms      run_tap: L2 text: MISS (label bridge drop)
t=350ms    run_tap: L3 ocrText: OCR call fires
t=550ms    Vision returns "no match" — but the target JUST rendered at t=520ms
t=550ms    tap fails
t=800ms    if we'd polled once more, the target would be there
```

Fix — new adapter poll loop in `run_tap` Fallback branch:

```rust
if fallback contains any OcrText:
    poll_budget = SMIX_TAP_OCR_POLL_MS (default 3000 ms)
    loop until first hit or budget:
        for each sub in fallback:
            if match: return Ok
        sleep 250 ms
else:
    single pass (pre-v1.0.23 semantics preserved — zero perf change)
```

Consumer control: bump the budget if your post-transition mount is slower than 3 s:
```bash
SMIX_TAP_OCR_POLL_MS=5000 smix run bootstrap-flow.yaml
```

Failure hint now names the poll budget explicitly so consumers see WHY the wait happened.

Fast path preserved: `tapOn: {id: btn-…}` or `tapOn: fallback: [id, text]` (no OCR) is unchanged — single pass, no poll, no perf change.

## D2 — scrollUntilVisible fires OCR between scroll strokes

Pre-v1.0.23 `Step::ScrollUntilVisible` → `driver.scroll` → tree resolver only. Off-screen elements in RN 0.86 Fabric LazyColumn/LazyRow drop from the a11y tree on iOS 26.5, so the tree probe was doomed from the start. Insight round-2 §Ask 5.

Fix — new adapter path `scroll_until_visible_with_ocr`. Activates when the selector contains any `OcrText`:

```rust
for _ in 0..=30 swipes / 20 s wall:
    if check_selector_visible(selector) via tree + OCR: return Ok
    if deadline exceeded: return timeout with actionable hint
    app.scroll_screen(direction)
```

Shared helper `check_selector_visible` between D1 tapOn poll and D2 scroll poll — one implementation of "probe this selector once via tree + OCR". No duplicated logic.

## D3 — X-Tree-Snapshot-Refresh-Count + X-Tree-Snapshot-Wall-Ms headers

Additive HTTP response headers on `/tree` — wire body unchanged, pre-v1.0.23 consumers see zero change.

**`X-Tree-Snapshot-Refresh-Count`** — monotonic UInt64. Cumulative /tree successful serves since runner boot. Consumer subtracting between calls detects stalls:

```bash
# Batch tail: are we still refreshing?
before=$(curl -si /tree | grep -i refresh-count | tr -dc '0-9')
sleep 5
after=$(curl -si /tree | grep -i refresh-count | tr -dc '0-9')
# after - before ≈ your polling rate × 5 s. If close to 0, XCUITest
# is stalled and returning cached snapshots.
```

**`X-Tree-Snapshot-Wall-Ms`** — how long THIS `snapshotHandler` invocation took end-to-end. Trending upward across a batch = XCUITest bogging down under sustained JS reload pressure.

Real-sim smoke shows the shape (Preferences, first-post-launch call): `X-Tree-Snapshot-Refresh-Count: 2, X-Tree-Snapshot-Wall-Ms: 5921`. First call is expensive because XCUITest is warming its accessibility cache; subsequent calls settle to ~200-500 ms wall on this sim. Track that number — if it climbs to seconds under `--all`, that's the drift signal.

**Not a JSON body change**: your original proposal wrapped body under `root` + added sibling fields. Would break every consumer that parses the top-level as an A11yNode. Headers give the same signal without wire disruption.

For `?refresh=1` on-demand refresh: XCUITest's `.snapshot()` is already fresh-per-call (the cache is UIKit-internal, not smix's). Adding a query param that forces re-fetch would be no-op today. If `X-Tree-Snapshot-Wall-Ms` grows uncontrolled and refresh count is monotonic, the drift is UIKit-side — not something smix can force through a query flag. But the headers give you the observation surface to file that upstream.

## D4 — `SMIX_AUTO_OCR_FALLBACK=1` bare-string auto-lift

```yaml
# terse (works if tree exposes text)
- extendedWaitUntil:
    visible: 'Log in to Insight'
    timeout: 30000

# safe (works under RN Fabric degradation) but 3× the tokens
- extendedWaitUntil:
    visible:
      fallback:
        - text: 'Log in to Insight'
        - ocrText: 'Log in to Insight'
    timeout: 30000
```

v1.0.23 opt-in: `SMIX_AUTO_OCR_FALLBACK=1 smix run flow.yaml` makes the first form parse EQUIVALENT to the second at parse time. Zero yaml edit; ~40% fewer lines across your 12 flows.

Accepted truthy values: `1`, `true`, `TRUE`, `yes`. Anything else (including unset) leaves bare strings as `Selector::Text` — pre-v1.0.23 semantics preserved.

**Env read at PARSE time (not RUN time)** — you can't have "sometimes this yaml parses to Text, sometimes to Fallback" depending on runtime state, which would violate the parser's determinism contract. Set the env before invoking `smix run`; it applies to every bare-string in that invocation.

Verified via 4 parser tests: `parse_visible_bare_string_default_stays_text`, `..._with_env_lifts_to_fallback`, `..._with_env_true`, `..._with_env_zero_stays_text`. Env-touching tests are Mutex-serialized because Cargo runs tests in parallel by default.

## Real-world impact on your open flake

- **Skip flake in force-update** (round-2 report): D1 tapOn poll closes the tap-mount race. Your `waitForAnimationToEnd 500` + `extendedWaitUntil` gate workaround stays valid; D1 makes the extra gate optional.
- **Chip off-screen in M/N** (round-2 report): D2 scrollUntilVisible OCR sees pixels — LazyColumn/LazyRow drop is invisible to Vision.
- **`--all` batch snapshot drift** (round-2 report): D3 headers give you the numeric drift signal. Watch `X-Tree-Snapshot-Wall-Ms` across scopes; if it trends up from ~250 ms to seconds, the drift is real and observable — file upstream.
- **12 flows spelling out fallback** (round-2 report): D4 with env-opt-in reduces yaml ~40%.

## Wire compatibility

- `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` headers additive — pre-v1.0.23 consumers see zero change.
- `SMIX_AUTO_OCR_FALLBACK` off ⇒ bare-string `visible: 'X'` parses to `Selector::Text` — pre-v1.0.23 semantics preserved.
- tapOn / scrollUntilVisible without any OCR in the selector: fast path preserved — no polling, no perf change.
- CLI / other subsystems byte-identical to v1.0.22.

## What v1.0.23 does NOT change

- v1.0.22 D1/D2/D3 preserved.
- v1.0.21 iOS 26.5 UIAlertController promotion preserved.
- v1.0.20 D1/D2/D3 preserved.

## What v1.0.23 does NOT solve

- **RN 0.86 Fabric → UIAccessibility bridge on iOS 26+ dropping identifier** — still app-side (or upstream to Meta). `A11yNode.elementTypeRaw` (v1.0.22 D3) is the wire evidence.
- **launch-chain title-all-cameras** — still your side (QA staging role assignment).
- **4th native race** — still your side; separate follow-up branch.

## Retest checklist (v1.0.22 → v1.0.23)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.23

# 2. Cold rebuild — this cycle changes both Swift (D3 header emission
#    in SmixRunnerServer + TreeRoute) and CLI (D1/D2 adapter loops +
#    D4 parser env-var read). Version-mismatch gate refuses boot.
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle <BUNDLE>
# → "synced runner sources → 1.0.23" line means it re-extracted

# 3. NEW — try the tapOn shape that was one-shot before
- tapOn:
    fallback:
      - id: 'btn-skip-force-update'
      - text: 'Skip'
      - ocrText: 'Skip'
# v1.0.23 polls this chain for up to 3000ms. Should hit L3 OCR
# even if the app is mid-transition when the tap fires.

# 4. NEW — try the scrollUntilVisible shape that was tree-only
- scrollUntilVisible:
    element:
      fallback:
        - id: 'btn-panel-fixture-push-simulate-event'
        - ocrText: 'Simulate deeplink'
    direction: DOWN
    timeout: 15000

# 5. NEW — try the env-opt-in for terser yaml
SMIX_AUTO_OCR_FALLBACK=1 smix run flow.yaml
# Every bare `visible: 'X'` parses as fallback: [text: X, ocrText: X].

# 6. NEW — batch drift signal
# During your --all sweep, tail X-Tree-Snapshot-Wall-Ms:
watch -n 5 'curl -si -m 8 http://localhost:22087/tree \
  -H "Session-Id: $YOUR_SID" | grep -Ei "X-Tree-Snapshot"'

# 7. Full bootstrap batch (unchanged)
bun test:e2e -- --metro-log /tmp/metro.log
```

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.23 feedback:
1. Does D1 tapOn OCR poll close the Skip flake in force-update?
2. Does D2 scrollUntilVisible OCR reach the deeplink chip in M/N?
3. What are typical `X-Tree-Snapshot-Wall-Ms` values across your `--all` sweep? (Baseline vs mid-batch.)
4. Do bare-string flows survive under `SMIX_AUTO_OCR_FALLBACK=1`?

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.23-shipping.md
```

## Prior chain

- ... (v1.0.14 → v1.0.22; see prior shipping docs)
- `insight-v1.0.22-shipping.md`
- `smix-feedback-2026-07-12-round-2.md` — the round-2 report this doc responds to
- **this doc** — v1.0.23 tapOn/scroll OCR + snapshot headers + auto-OCR opt-in
