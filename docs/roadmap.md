# smix — Roadmap

Version cadence, charter previews, and the deprecation runway. Every entry links to the RFC that supersedes this preview once the version enters implementation.

Semver policy is unchanged from v1.0.0:

- **Patch** (`1.0.x`) — additive-only wire + additive ABI + bug fixes + capability closure.
- **Minor** (`1.x.0`) — additive wire + additive ABI + new capability charters.
- **Major** (`x.0.0`) — wire and/or ABI break; deprecation runway required.

Wire and ABI stability guarantees in [`docs/ai-guide/wire-format.md`](./ai-guide/wire-format.md) and [`docs/ai-guide/abi-stability.md`](./ai-guide/abi-stability.md).

---

## Shipped

- **v1.0.0 — 2026-07-08** — Initial public release. iOS/Android/Rust/TS/Swift/Kotlin surface; Maestro-compatible YAML runtime.
- **v1.0.1 — 2026-07-09** — Parser fix for `expect: { visible: ... }` shorthand; `smix run --check` parse-only gate.
- **v1.0.2 — 2026-07-09** — Runner activation storm fix; PNG sRGB metadata splice; liveness observability.
- **v1.0.3 — 2026-07-09** — Session lifecycle across all 4 SDKs; runner idempotent close.
- **v1.0.4 — 2026-07-11** — Studio-protection release. `smix-sim-health` sense stone + screenshot pacer + `/system-popups` 500ms floor + app-alive cache. `Session::state` + `Session::relaunch_app` across 4 SDKs. `launchApp: clearState: true` rewrite (§F+§H). CLI `runner cycle` + `--gate-signal` + safe-exit cascade. Closed all 9 §A-§I of the 2026-07-10 gate-hardening feedback. RFC `.claude/rfcs/1.0.4-sim-health-and-backpressure.md`.

---

## Next patch — v1.0.5 (target 2 weeks from v1.0.4)

RFC: [`.claude/rfcs/1.0.5-supervisor-and-persistence.md`](../.claude/rfcs/1.0.5-supervisor-and-persistence.md)

- **§E ask 2 — Session persistence across XCTest lifecycle.** Session table persists to `.smix/runner/sessions-<UDID>.json` + reloads on test-host boot. `POST /session/list` + `Session::still_valid()` on all 4 SDKs.
- **§D6 — Host-side XCTest supervisor daemon.** `smix runner supervise` or `smix runner up --supervise`; auto-cycles on `** TEST INTERRUPTED **` / `SchemeActionResultOperation` log matches.
- **Runner idle-close 120s → 60s.** Half-orphaned sessions vanish within 1 minute instead of 2.
- **Real-sim smoke gate.** `scripts/release/smoke-v1.smoke.sh` — hard gate on the ship script; every net-new v1.0.4/v1.0.5 code path is exercised before publish.

Cadence: no RC — the smoke gate replaces it.

---

## Later patches — v1.0.6 onward (feedback-driven)

Not planned in advance. Each accepted patch charter is triggered by:

- A named insight feedback item that survives the "does this fold into an existing charter?" check.
- A downstream consumer regression discovered post-v1.0.5.
- A CVE or upstream Apple/JetBrains toolchain change that shifts behaviour under smix's feet.

Patch releases stay additive-wire + additive-ABI. Anything requiring wire or ABI break goes to v1.1 or v2 accordingly.

---

## Next minor — v1.1 (target Q3 2026, still additive)

Charter previews (not yet in an RFC — will get one when planning starts):

### Runtime performance

- **Non-xcodebuild XCUITest restart.** Today, `smix runner cycle` spawns a fresh `xcodebuild test-without-building` — ~3 s warm, ~15 s cold. A leaner "bounce the FlyingFox server + rebind XCUIApplication within the same test-host process" path avoids the xcodebuild spawn entirely and takes ~500 ms.
- **Screenshot pipeline hoisting.** Move the sRGB chunk splice + adaptive pacer into a small C-backed helper (or async pool) so `simctl io screenshot` overhead falls below 100 ms at the p95.

### Coverage and tooling

- **Multi-sim orchestration.** `smix run --parallel <N>` sharding N flows across M sims (each with its own runner + supervisor pair). Preserves the current single-sim contract as N=1.
- **Recording tier maturity.** Elevate `smix authoring capture-tree` / `record` / `diff-tree` from "adjacent surface" to primary authoring interface for AI-driven flow synthesis. Ties into Anthropic's Claude API for LLM-driven flow generation.
- **Android runtime feature parity.** UiAutomator paths for the iOS-specific v1.0.4 features (rate-limit pacer, app-alive cache) — Android has different failure modes but the sense/act layer contract is portable.

### Testing and CI

- **Real-sim stress harness.** 20-flow bootstrap corpus that runs on nightly CI against `sim-smoke`. Replaces the ad-hoc smoke gate with a graded stress-and-smoke pipeline.
- **Bench regression detection.** `smix bench` new subcommand that runs the perf-gate corpus + compares against the last committed baseline. Fails CI when the regression exceeds 5%.

### Documentation

- **AI-authoring guide.** A comprehensive `docs/ai-guide/authoring.md` covering how to write flows against the Locator API from an LLM (Claude, GPT, whichever). Recipe-driven; not a formal API doc.
- **Session state playbook.** Each `SessionState` transition → recommended consumer response (pause / retry / cycle / bail). Turns v1.0.4's raw signal into an operator runbook.

Wire and ABI additions in v1.1 remain additive — anything that would break v1.0.x consumers slides to v2.

---

## Next major — v2.0 (target Q4 2026 to Q1 2027, semver break)

Only entries here are wire-breaking or ABI-breaking. Every entry has a deprecation runway that must ship in a v1.x patch before v2 lands.

- **Sessions become mandatory.** Retire the legacy per-request rebind path; remove `App-Activate: true` header semantics; require `Session-Id`. Consumer runway: v1.x deprecation warning printed by the CLI when it detects a request without a session.
- **Wire schema version negotiation.** Runner + client exchange a version header at `/health`; unrecognized combinations are refused with a diagnostic instead of the current permissive "extra fields ignored". Locks the wire contract explicitly.
- **`SMIX_*` escape hatch removal.** `SMIX_LAUNCH_FRESH_FORCE_REINSTALL`, `SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE`, `SMIX_DEV_LOCK` — bake the safe defaults; remove the opt-out env vars. Runway: v1.x prints a WARN when any of them is set.
- **Selector model consolidation.** Merge the `Modifier` and `Modifiers` types (accumulated dupes across 6-7 minor releases); collapse the two `open_url` shapes; remove the deprecated positional args accepted since v0.3.
- **`smix-recorder-ir` → `smix-authoring-ir`** rename. Aligns the crate name with what the tier actually is (authoring, not just recording). Runway: v1.x re-export shim.
- **YAML verb table freeze v2.** Any verb marked `@deprecated` in the v1.x table gets removed. Runway: `smix migrate` bumps + a codemod that rewrites deprecated shapes to v2 shapes.

v2 will NOT drop:
- CLI verb names shipped in v1.0.
- The core sense/act/decide three-layer contract.
- The four-SDK parity guarantee.
- The `Session::relaunch_app` / `Session::state` surface (v1.0.4 additions).

---

## Beyond v2 — speculative

- Cross-platform recorder that speaks XCTest + UIAutomator + web (Playwright bridge) uniformly.
- LLM-in-the-loop authoring where the smix runtime observes flow execution and proposes improvements (backed by Claude API).
- Distributed run federation: N smix runners on N machines coordinated by a central scheduler, each running its own sim/emulator, results merged for CI.

None of these are committed; they're the horizon we're aiming toward and inform how we design the v1.1 → v2 additive-vs-breaking boundary.

---

## Cadence philosophy

- **Patch = tomorrow's problem.** Ship as soon as green.
- **Minor = next month.** Batch enough charter items to justify the marketing cycle.
- **Major = next season.** Give the ecosystem a genuine deprecation runway; never break in a patch.

Every version — patch, minor, major — ships across all four ecosystems simultaneously. crates.io + npm + Maven Central + Swift Package tag in the same publish window. No fractured "1.0.5 on crates.io / 1.0.4 on npm" states.
