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
- **v1.0.4 — 2026-07-11** — Studio-protection release. `smix-sim-health` sense stone + screenshot pacer + `/system-popups` 500ms floor + app-alive cache. `Session::state` + `Session::relaunch_app` across 4 SDKs. `launchApp: clearState: true` rewrite (§F+§H). CLI `runner cycle` + `--gate-signal` + safe-exit cascade. Closed the full 9-point gate-hardening ask (§A–§I). RFC `.claude/rfcs/1.0.4-sim-health-and-backpressure.md`.
- **v1.0.5 → v1.0.27 — 2026-07-11 to 2026-07-13** — Feedback-driven patch arc, all additive-wire + additive-ABI. In order of the problem each solved: session persistence + host-side supervisor daemon (`smix runner supervise`); a systemic CLI-vs-runner source-sync fix (the CLI now ships the Swift runner sources and re-syncs on `runner up`, ending six releases of silently-frozen runner code); a runtime observability layer (`smix diagnostic dump`, subprocess ring, lifecycle counters); crash-dialog elimination via cooperative in-runner terminate/launch; and the iOS 26.5 + RN 0.86 Fabric tree-degradation triage stack — OCR-in-verbs, auto-capture on timeout, live on-screen visibility confirmation, per-key user-defaults deletion. Per-release detail lives in `CHANGELOG.md`; the roadmap does not replay it.
- **v2.0.0 — 2026-07 (first formal public release)** — Wire- and ABI-breaking. smix's capability surface now exceeds maestro, with iOS + Android parity as a ship gate. The release carries the fenced AI-assertion tier (`assertCondition` / `extractWithAI` — screenshot to a local `claude` CLI, back as a structured verdict, firewalled off the resolver path), the full external-agent MCP driving surface, true animation-idle (frame-diff, not fixed sleep), and a code/docs hygiene pass (comment de-noising across 26+ crates, a machine-checked `hygiene-scan` gate, and this script-generated `llms.txt` / `llms-full.txt` index). It lands two structural pieces:
  - **Six breaking changes** (each with a `smix migrate` codemod path): (1) sessions mandatory — the implicit no-session rebind path is gone, `Session-Id` required; (2) wire schema-version negotiation at `/health` — the runner and client agree a schema instead of assuming v1, which also lets v2 add routes; (3) `SMIX_*` env switches folded into `.smix/config.yaml switches:` (env still honored with a named deprecation warn); (4) the dead `Modifier` (singular) SDK type removed (`Modifiers` was always the real selector-modifier model); (5) `smix-recorder-ir` → `smix-authoring-ir` (stone-crate rename = semver break); (6) `VERB_TABLE` frozen as the single source of truth, with a parser-dispatch ⊆ table test gate. Alongside them, `SimctlError` → `DeviceControlError` (the old name made every Android failure call itself an iOS tool).
  - **SDK re-architecture — one wire client, reached through FFI.** The three SDKs had each shipped against a fictional wire (13 driver routes no runner served — they mock-tested themselves into agreement). v2 collapses this to a single wire client (the Rust client), reached through the `smix-ffi` boundary by the Swift and Kotlin SDKs; the TypeScript SDK's fictional routes are deleted (host-side/driver methods now throw an explicit `not-implemented (lands napi)` rather than 404-ing), keeping only its genuinely-served resolver routes. A `route-conformance` gate holds the line at zero unserved routes.

---

## Next minor — v2.1 (post-v2, additive)

Charter previews carried forward from the pre-v2 v1.1 horizon — additive-wire + additive-ABI candidates, not yet in an RFC:

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

Wire and ABI additions in v2.1 remain additive — anything that would break v2.0.x consumers slides to the next major (v3).

What v2.x will NOT drop, carried as a standing guarantee:
- CLI verb names shipped in v1.0 / v2.0.
- The core sense / act / decide three-layer contract.
- The four-SDK parity guarantee (TS reaches driving parity when the napi axis lands).
- The `Session::relaunch_app` / `Session::state` surface.

---

## Beyond v2 — speculative

- **TypeScript driving via napi.** Close the one remaining SDK gap: a cross-triple prebuilt `.node` binding so the TS SDK drives through the same Rust wire client as Swift/Kotlin, retiring the `not-implemented (lands napi)` stubs. Independent distribution-engineering deliverable.
- Cross-platform recorder that speaks XCTest + UIAutomator + web (Playwright bridge) uniformly.
- LLM-in-the-loop authoring where the smix runtime observes flow execution and proposes improvements (backed by the Claude API).
- Distributed run federation: N smix runners on N machines coordinated by a central scheduler, each running its own sim/emulator, results merged for CI.

None of these are committed; they're the horizon we're aiming toward and inform how we design the v2.x additive-vs-breaking boundary.

---

## Cadence philosophy

- **Patch = tomorrow's problem.** Ship as soon as green.
- **Minor = next month.** Batch enough charter items to justify the marketing cycle.
- **Major = next season.** Give the ecosystem a genuine deprecation runway; never break in a patch.

Every version — patch, minor, major — ships across all four ecosystems simultaneously. crates.io + npm + Maven Central + Swift Package tag in the same publish window. No fractured "1.0.5 on crates.io / 1.0.4 on npm" states.
