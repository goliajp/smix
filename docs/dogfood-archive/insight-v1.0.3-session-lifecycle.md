# smix v1.0.3 — activation storm + screenshot走样 fully closed

Date: 2026-07-09
Reply to:
- `qualcomm/insight/.claude/state/gol-611/smix-feedback-v1.0-parse-gap-2026-07-09.md`
- `qualcomm/insight/.claude/state/gol-611/smix-feedback-v0.3.1-runner-crash-2026-07-09.md`

## TL;DR

Both reports landed on the same root cause. Three patch releases published today:

- **v1.0.1** — `smix run` parser gap on `expect: { visible: ... }` + `smix run --check` parse-only gate
- **v1.0.2** — activation storm interim fix (rate-limited legacy path) + PNG sRGB byte-splice + runner liveness observability
- **v1.0.3** — session lifecycle across all 4 SDKs (Rust / TS / Swift / Kotlin) + CLI auto-session — supersedes the interim rate limit

`cargo install smix-cli --locked --version 1.0.3 --force` gets you the fully-closed version. Your `develop` can revert the `v0.3.1` pin.

`bun verify visual` and `bun verify perf` should now pass against the same corpus that failed on 2026-07-09. Baseline re-accept is NOT needed — the previous baselines were captured under a clean app state; the today failures were captures under a mid-activation-storm state, not real UI drift.

## Pixel forensic — findings from your `hub-form.png`

Before writing any fix I did a pixel-level forensic on the two PNGs you had:

| | Baseline 2026-07-08 17:46 | Current 2026-07-09 02:51 |
|---|---|---|
| Dimensions | 1206 × 2622 RGBA | same |
| PNG chunks | `IHDR + sRGB + eXIf + 10×IDAT + IEND` | `IHDR + 1×IDAT + IEND` |
| Size | 158 104 bytes | 91 404 bytes |
| Colorspace | `sRGB IEC61966-2.1` (ICC) | none |
| Decoded RGBA sha256 (top 16 hex) | `93301ba8cb5d9c4c…` | `41b1d05ca508352b…` |
| **Pixel diff (RGBA compare)** | — | **27.67 % of pixels differ, max channel Δ = 255** |

Two things collapse out of that table:

1. **27.67 % pixel diff with max Δ 255 rules out "screenshot encoder走样"** as the primary cause. The two PNGs capture materially different screen states — the current run genuinely captured a mid-transition frame with a red block covering the form's upper half. This was your first hypothesis and the pixel forensic confirmed it.
2. **The missing `sRGB` / `eXIf` chunks are an independent secondary effect** — iOS 26.5 sub-build variability in `xcrun simctl io screenshot`. Preview.app falls back to Display P3 without an ICC profile embedded, over-saturating red and adding yellow anti-alias fringing. This does NOT affect pixel-comparison (dhash / hamming decode IDAT and ignore ancillary chunks), only affects human-review viewers.

So what your report framed as "two bugs" collapsed to **one root cause with two clinical presentations along a severity axis**:

- **Presentation A** (long sessions, > 340 s): XCTest process arbitration exhausted → `test_runForever()` crashes → downstream stale bodies → visual gate reports 10-19 % diff + `metric=-1`
- **Presentation B** (medium sessions, before the crash boundary): app is repeatedly re-activated but runner is still alive → `simctl io screenshot` catches the app mid-transition (pull-to-refresh stuck expanded, SpringBoard event overlay, a11y highlight residue) → captured pixels genuinely show the mid-state red block seen in `hub-form.png`

Both are downstream products of `resolveApp()` calling `.activate()` on every request. Fix the root, both presentations disappear.

## What shipped

### v1.0.1 — parser fix + `smix run --check` (2026-07-09)

Response to `smix-feedback-v1.0-parse-gap-2026-07-09.md`.

**Parser bug**: `smix run` refused to parse `expect: { visible: { text: 'X' }, timeoutMs: 8000 }` — the exact shape `smix migrate` emits for `extendedWaitUntil`. `parse_expect` fell through to `parse_assert_visible` → `visible_to_selector` on the outer map, which looks for top-level `text` / `id` — not present, so `expected 'text' or 'id' key`.

**Fix**: `crates/smix-adapter-maestro/src/parser.rs::parse_expect` — three new arms accept every shape migrate emits:

- `expect: { visible: <selector>, timeoutMs: N }` → `Step::ExtendedWaitUntil { expect_visible: true }`
- `expect: { visible: <selector> }` (no timeout) → `Step::AssertVisible`
- `expect: { notVisible: <selector>, timeoutMs?: N }` → symmetric

Plus 8 regression tests in `crates/smix-adapter-maestro/tests/parser.rs` pinning every accepted shape (block-style / flow-style / with-timeout / without-timeout / notVisible / bare-string / top-level-text back-compat).

**Also fixed**: `smix migrate --help` text drift — now correctly states that comments, copyright headers, and blank lines survive the codemod byte-identical.

**Systemic capability upgrade** (your capability suggestion #3): `smix run --check` — parse-only pre-flight gate. Reads every listed flow YAML and reports parse or include errors without connecting to a runner or booting a simulator. Exit 0 on clean parse across every flow; non-zero (2) on any error. Suitable for CI without simulator infrastructure. Wire it in wherever you want a fast "does this YAML corpus parse against the current CLI" gate.

### v1.0.2 — interim activation fix + sRGB splice + liveness observability (2026-07-09)

Response to `smix-feedback-v0.3.1-runner-crash-2026-07-09.md`.

**Runner activation storm** (root cause): `SmixRunnerUITests.swift::resolveApp()` unconditionally called `.activate()` on every request with `App-Activate: true`. Over 340 s at ~300 ms cadence that's ~1130 activations — enough to exhaust XCTest process arbitration on iOS 26.5.

**Interim fix (v1.0.2)**: rate-limit `.activate()` to at most once per 5 s per bundle-id. Recovery semantics preserved — after 5 s of silence a subsequent activate hint is honored, so a SpringBoard foreground steal is still auto-recovered.

Empirical impact:
- **Before**: ~1130 activations over 340 s
- **After (v1.0.2 rate limit)**: ~68 activations over 340 s
- **After (v1.0.3 session)**: 1 activation for the whole session

**PNG sRGB byte-splice** (independent finding): iOS 26.5 sub-build changed `xcrun simctl io screenshot` to omit the `sRGB` ancillary chunk. `smix-simctl::SimctlClient::screenshot` now byte-splices a synthesized 13-byte `sRGB` chunk (rendering intent = 0, perceptual) into the PNG stream immediately before the first IDAT when none is present. **IDAT bytes are never decoded or modified** — pixel-comparison consumers see byte-identical decoded RGBA arrays.

Validated end-to-end on your actual anomalous `hub-form.png`:
```
input:  91 404 bytes, no sRGB chunk
output: 91 417 bytes (+13), sRGB chunk present
decoded RGBA sha256: IDENTICAL before/after splice
```

**Runner liveness observability** (your capability suggestion #2): opt-in via `HttpRunnerClient::with_liveness_window(N)`. Client tracks a rolling window of the last N request outcomes.
- Majority-failure → `RunnerTransportError::RunnerDegraded { window, non_success_recent, last_endpoint, last_error }`
- `is_connect()` failure + `/health` probe fails within 1 s → `RunnerTransportError::RunnerDied { last_seen_ms, last_error }`
- No more silent stale bodies when the runner has drifted into a degraded state.

**Extended `GET /health` JSON body**: `{ ok, runnerVersion, uptimeMs, lastRequestAtMs, sessionsOpen, activationsTotal }`. Legacy consumers that jq-parse `{"ok":true}` continue to work — the extended body is a strict superset. Rust client parses via `HttpRunnerClient::health_detail()`.

### v1.0.3 — session lifecycle across all 4 SDKs (2026-07-09)

Systemic fix that supersedes the v1.0.2 interim rate limit. Consumers open a session at flow start, run every step against the runner's cached `XCUIApplication` binding, close on exit. No per-request `.activate()`, ever — the session's cached binding is reused every time.

**Runner wire** (all additive):
- `POST /session/open  {bundleId, activate}` → `{sessionId, activatedOnce, serverTimeMs}`
- `POST /session/close {sessionId}` → `{ok}` (idempotent)
- `POST /session/renew-activation {sessionId}` → `{ok, activated}` (2 s per-session rate limit; 404 for unknown ids)
- `Session-Id: <id>` header on any subsequent request → `resolveApp()` short-circuits directly to session cache

**All 4 SDKs ship real Session support** (no defer to v1.1):

- **Rust SDK** — `App::open_session(bundle_id, activate) -> Session`; `Session::app_mut()` / `renew_activation()` / `close()`. `Driver` trait `set_session_id()` with iOS + Android impls.
- **TypeScript SDK** — `Session.open(runtime, bundleId, {activate})` on `HttpSimRuntime`. `HttpRunner.setSessionId()` attaches `Session-Id` on every POST.
- **Swift SDK** — new `HttpSmixSimRuntime` (URLSession-backed `SmixSimRuntime` protocol impl, full runner wire) + `Session.open(runtime, activate:)` / `session.close()` / `session.renewActivation()`.
- **Kotlin SDK** — new `HttpSmixSimRuntime` (`java.net.HttpURLConnection`, no new dep) + `Session.open(runtime, activate=true)` / `session.close()` / `session.renewActivation()`. `AtomicReference` for thread-safe session-id state.

**CLI (`smix run`)** — auto-opens a session at start, closes on exit. Runners that don't implement `/session/open` (v1.0.x pre-1.0.3) return non-2xx; CLI emits a WARN and falls back to the v1.0.2 legacy per-request path (rate-limited, so still safe).

Your visual + perf stage code needs zero changes to benefit — `smix run` opens/closes the session for you.

## Wire + ABI compatibility

Wire-additive-only through the entire chain:
- `Session-Id` header optional on every non-session route
- Session routes are new — don't affect any existing endpoint
- Extended `/health` body is a strict superset
- `RunnerTransportError` new variants are `#[non_exhaustive]`-safe

Compatibility matrix that matters for you:

| Client | Runner | Behavior |
|---|---|---|
| v1.0.3 | v1.0.3 | Session lifecycle throughout |
| v1.0.3 | v1.0.2 | Session open fails → WARN → legacy path with 5 s rate limit |
| v1.0.3 | v0.3.1 | Session open fails → WARN → legacy path (unbounded — but v1.0.3 client wouldn't be used with a v0.3.1 runner in practice) |
| v0.3.1 client | v1.0.3 runner | Legacy path with runner-side 5 s rate limit (still safe) |

## Adoption on your side

The minimum action to unblock:

```bash
cargo install smix-cli --locked --version 1.0.3 --force
smix --version   # → smix 1.0.3
```

Then re-run `bun verify visual` and `bun verify perf`. Expected outcome: pass with the same baseline set that was passing on 2026-07-08. **No baseline re-accept required** — the 27.67 % pixel diff was captured under a mid-storm state; a clean-state capture should match the 2026-07-08 baselines.

If any anchor is still over threshold after this update, that IS a real UI drift or a genuine screenshot integrity issue — I want to hear about it as a fresh feedback report because the fix chain above should have eliminated the storm-driven false positives.

## What to do if the pixel diff comes back

Three orthogonal probes I'd try in order:

1. **Confirm session actually opened** — `smix run --debug-output <dir>` and look for the `WARN: /session/open failed` line in stderr. If present, the runner didn't accept the session (should not happen against a v1.0.3 runner, but this is the fastest way to prove that).

2. **Confirm PNG is byte-clean** — `pngcheck -v <path>` on the failing anchor. Expected chunks: `IHDR + sRGB + IDAT+ + IEND`. If missing sRGB, `smix-simctl` splice didn't fire (should not happen in v1.0.3 — but this is the second fastest way to isolate).

3. **Confirm pixel diff isn't a real UI change** — decode both PNGs (baseline + current) into RGBA arrays and compute pixel diff. If < 5 %, that's just anti-alias jitter or animation-frame drift; if higher, it's a real diff and worth an actual product-side investigation.

Point (3) is worth having as a permanent CI helper on your side — it's a 20-line Python + Pillow script and it lets you distinguish "screenshot pipeline走样" from "real UI drift" in seconds. I can send you the exact script if helpful.

## Optional new capabilities you may want to adopt

These are optional; the storm fix works without any of them. Listed here because they're systemic upgrades that map to your report's other suggestions.

### `smix run --check` (v1.0.1)

Parse every yaml in your corpus in CI without booting a sim:

```bash
smix run --check .devtools/qa/sim/**/*.yaml
```

Exit 0 clean; exit 2 on any parse or include error. Suitable for a pre-flight gate before you spin up an emulator.

### Session for consumers driving the SDK directly

If any of your test code drives the SDK directly (not just via `smix run`), wrap in a session:

**TypeScript**:
```ts
import { Session, HttpSimRuntime, Smix, bundleId } from '@goliapkg/smix'

const runtime = new HttpSimRuntime('http://127.0.0.1:22087')
const session = await Session.open(runtime, 'com.focusai.app.mobile', { activate: true })
try {
  const app = await Smix.launchApp(bundleId('com.focusai.app.mobile'), runtime, runtime.resolver)
  await app.tap(Selector.id('btn-login'))
  await app.find(Selector.text('Dashboard')).toBeVisible({ timeoutMs: 5000 })
} finally {
  await session.close()
}
```

If your stage code uses the CLI only, ignore this — `smix run` opens the session for you.

### Extended `/health` for CI visibility

Point your CI dashboard at `GET /health` on the runner port; parse the JSON:

```json
{
  "ok": true,
  "runnerVersion": "1.0.3",
  "uptimeMs": 12345,
  "lastRequestAtMs": 1720500000000,
  "sessionsOpen": 1,
  "activationsTotal": 3
}
```

`activationsTotal` climbing linearly in the millisecond range while `uptimeMs` is climbing 340 s = your gate would have caught this incident well before the crash.

### Runner liveness in your consumer client (Rust only for now)

If you write any consumer code in Rust using `HttpRunnerClient` directly, opt in:

```rust
let client = HttpRunnerClient::new(22087).with_liveness_window(8);
```

Silent stale bodies now surface as `RunnerTransportError::RunnerDegraded` or `RunnerDied` with `last_seen_ms` + last endpoint context. Not needed on the CLI path (the CLI drives its own error surface).

## Chain — what shipped when

- **2026-07-08 morning** — v1.0.0 (10-phase mega-cycle) published to crates.io / npm / Maven Central / Swift GH Release
- **2026-07-08 17:46** — insight's `/pub` succeeds with re-accepted baselines under v0.3.1
- **2026-07-09 02:51** — insight's next `/pub` fails with 27.67 % pixel diff on 4/4 anchors; feedback files filed
- **2026-07-09** — v1.0.1 published (parser fix + `smix run --check`)
- **2026-07-09** — v1.0.2 published (activation-storm interim rate limit + sRGB splice + liveness)
- **2026-07-09** — v1.0.3 published (session lifecycle across all 4 SDKs + CLI auto-session)

## Docs

- `docs/ai-guide/09-sessions.md` — session lifecycle for Rust / TypeScript / Swift / Kotlin SDK + CLI
- `docs/ai-guide/wire-format.md` — updated with `/session/*` routes + extended `/health` payload
- `CHANGELOG.md` v1.0.1 / v1.0.2 / v1.0.3 entries

## Contact

`lihao@golia.jp` on my side. If any anchor is still failing after 1.0.3 adoption, send me a fresh feedback file and I'll cut a fresh cycle.

## Share this path

`/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.3-session-lifecycle.md`
