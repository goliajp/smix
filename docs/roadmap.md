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
- **v2.0.0 — pending (first formal public release)** — Not yet published. Pre-fold ship-readiness was reached and every gate verified; then (2026-07-23) the whole forward roadmap was folded in, so v2.0.0 now ships only once the folded work (see "Folded into v2.0.0" below) is done. Wire- and ABI-breaking. smix's capability surface now exceeds maestro, with iOS + Android parity as a ship gate. The release carries the fenced AI-assertion tier (`assertCondition` / `extractWithAI` — screenshot to a local `claude` CLI, back as a structured verdict, firewalled off the resolver path), the full external-agent MCP driving surface, true animation-idle (frame-diff, not fixed sleep), and a code/docs hygiene pass (comment de-noising across 26+ crates, a machine-checked `hygiene-scan` gate, and this script-generated `llms.txt` / `llms-full.txt` index). It lands two structural pieces:
  - **Six breaking changes** (each with a `smix migrate` codemod path): (1) sessions mandatory — the implicit no-session rebind path is gone, `Session-Id` required; (2) wire schema-version negotiation at `/health` — the runner and client agree a schema instead of assuming v1, which also lets v2 add routes; (3) `SMIX_*` env switches folded into `.smix/config.yaml switches:` (env still honored with a named deprecation warn); (4) the dead `Modifier` (singular) SDK type removed (`Modifiers` was always the real selector-modifier model); (5) `smix-recorder-ir` → `smix-authoring-ir` (stone-crate rename = semver break); (6) `VERB_TABLE` frozen as the single source of truth, with a parser-dispatch ⊆ table test gate. Alongside them, `SimctlError` → `DeviceControlError` (the old name made every Android failure call itself an iOS tool).
  - **SDK re-architecture — one wire client, reached through FFI.** The three SDKs had each shipped against a fictional wire (13 driver routes no runner served — they mock-tested themselves into agreement). v2 collapses this to a single wire client (the Rust client), reached through the `smix-ffi` boundary by the Swift and Kotlin SDKs; the TypeScript SDK's fictional routes are deleted (host-side/driver methods now throw an explicit `not-implemented (lands napi)` rather than 404-ing), keeping only its genuinely-served resolver routes. A `route-conformance` gate holds the line at zero unserved routes.

---

## Folded into v2.0.0 (decision 2026-07-23)

The entire forward roadmap below — the v2.1 additive charter **and** the previously
speculative horizon — is folded into the v2.0.0 release. **v2.0.0 does not publish
until all of it is done.** These were separate, later, and in three cases explicitly
out-of-scope; the decision to do them first and ship once is recorded in
`docs/v2.md` (决策日志 2026-07-23) and drives the v2.8+ phase cold plans.

**Status (2026-07-24): the fold is complete.** All five folded minors landed —
v2.8 faster-and-wider, v2.9 napi (TS SDK drives sims), v2.10 cross-platform
recorder, v2.11 LLM-in-the-loop authoring, v2.12 federation — each with its
checkpoints green (see the `docs/v2.md` decision log and `docs/plan-history/`).
The item descriptions below stand as written; what shipped is recorded per
minor in `CHANGELOG.md`. Two things remain before publish and are the user's
call: wiring the napi `@goliapkg/smix-node` npm publish into `ship.sh` (the TS
driving code is done but its addon is not yet on npm), and the ship
authorization itself.

Two constraints survive the fold and are not the user's to waive silently, so they
are called out where an item brushes them:

- **§9#1 simulator-only.** No item introduces a real-device path. The cross-platform
  recorder's web axis is a Playwright *bridge*, not a physical device.
- **§9#2 single-provider, local `claude` CLI.** LLM-in-the-loop authoring is described
  below as "backed by the Claude API"; folded in, it defaults to the **local `claude`
  CLI** (single-provider, no network key management) to honor §9#2. Revisiting §9#2 to
  allow the Claude API directly is a separate invariant decision, flagged in the log.

### Runtime performance

- **Non-xcodebuild XCUITest restart.** `smix runner cycle` spawns a fresh `xcodebuild test-without-building` — ~3 s warm, ~15 s cold. Bounce the FlyingFox server + rebind XCUIApplication within the same test-host process, ~500 ms, no xcodebuild spawn.
- **Screenshot pipeline hoisting.** Move the sRGB chunk splice + adaptive pacer into a small C-backed helper (or async pool) so `simctl io screenshot` overhead falls below 100 ms at the p95.

### Coverage and tooling

- **Multi-sim orchestration.** `smix run --parallel <N>` sharding N flows across M sims (each with its own runner + supervisor pair). Preserves the current single-sim contract as N=1.
- **Recording tier maturity.** Elevate `smix authoring capture-tree` / `record` / `diff-tree` from "adjacent surface" to primary authoring interface for AI-driven flow synthesis.
- **Android runtime feature parity.** UiAutomator paths for the iOS-specific v1.0.4 features (rate-limit pacer, app-alive cache).
- **Occlusion-aware hit verdict.** The EXT1 #4 deferral — snapshot has no z-order, `isHittable` was rejected twice. Scoped as "find out if z-order is obtainable" before anything is built.

### Testing and CI

- **Real-sim stress harness.** 20-flow bootstrap corpus on nightly CI against `sim-smoke`. Replaces the ad-hoc smoke gate with a graded stress-and-smoke pipeline.
- **Bench regression detection.** `smix bench` runs the perf-gate corpus + compares against the last committed baseline. Fails CI when the regression exceeds 5%. Prerequisite for the two perf items above.

### SDK completeness

- **TypeScript driving via napi.** A cross-triple prebuilt `.node` binding so the TS SDK drives through the same Rust wire client as Swift/Kotlin, retiring the `not-implemented (lands napi)` stubs. Closes the four-SDK parity guarantee.

### Authoring and orchestration

- **Cross-platform recorder** that speaks XCTest + UIAutomator + web (Playwright bridge) uniformly.
- **LLM-in-the-loop authoring** where the smix runtime observes flow execution and proposes improvements — via the local `claude` CLI per §9#2 (see the callout above).
- **Distributed run federation.** N smix runners on N machines coordinated by a central scheduler, each running its own sim/emulator, results merged for CI.

### Documentation

- **AI-authoring guide.** `docs/ai-guide/authoring.md` — how to write flows against the Locator API from an LLM. Recipe-driven.
- **Session state playbook.** Each `SessionState` transition → recommended consumer response (pause / retry / cycle / bail).

Everything above remains additive-wire + additive-ABI relative to the pre-fold v2.0
surface; a `cargo-semver-checks` pass per checkpoint holds that line so the fold does
not silently turn v2 into a moving target.

What v2 will NOT drop, carried as a standing guarantee:
- CLI verb names shipped in v1.0.
- The core sense / act / decide three-layer contract.
- The four-SDK parity guarantee (TS reaches driving parity when the napi axis lands — now in-scope).
- The `Session::relaunch_app` / `Session::state` surface.

---

## Beyond v2

Open. The former speculative horizon is now inside v2.0.0; the next horizon is set after
v2 ships.

---

## Cadence philosophy

- **Patch = tomorrow's problem.** Ship as soon as green.
- **Minor = next month.** Batch enough charter items to justify the marketing cycle.
- **Major = next season.** Give the ecosystem a genuine deprecation runway; never break in a patch.

Every version — patch, minor, major — ships across all four ecosystems simultaneously. crates.io + npm + Maven Central + Swift Package tag in the same publish window. No fractured "1.0.5 on crates.io / 1.0.4 on npm" states.
