# Changelog

All notable changes to the `smix` workspace are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) at the wire, ABI, and CLI surface.

## [1.0.4] — 2026-07-11

Studio protection + full-scope insight feedback response. Motivation: a downstream `insight` gate loop running against a v1.0.3 runner triggered `SimRenderServer` `brk 1` assertion inside the `com.apple.display.captureservice` dispatch queue, cascading into shutdown_stall and forced macOS restarts. Forensic evidence + response plan in `docs/ai-guide/insight-v1.0.3-studio-crash-2026-07-10.md` (gitignored). This release closes every ask in `insight/.claude/state/gol-611/smix-feedback-2026-07-10-gate-hardening.md` (§A–§I) plus the SimRenderServer stress fix, plus lifecycle-safe-exit primitives.

### Added — sense layer (RFC 1.0.4 §D1)

- **`smix-sim-health` — new stone crate.** Watches SimRenderServer + xcodebuild pids + `/health` age + rolling screenshot wall times. State machine `Healthy | Degraded | Dead`; transitions broadcast on a `tokio::sync::broadcast` channel. Business-unaware; SDK-facing state is exposed via `Session::state` (below), driver-side auto-cycle policies live per driver.
- **`HttpRunnerClient::with_sim_health(monitor)`** — `/health` outcomes feed `SimHealthMonitor::record_health_ok`/`record_health_fail`. `HttpRunnerClient::sim_health()` accessor.

### Added — act layer (RFC 1.0.4 §D3)

- **`smix-simctl` screenshot pacer.** Adaptive interval floor: 100 ms in the fast path (recent wall < 800 ms), 1500 ms in the slow path (recent wall ≥ 800 ms). Circuit breaker: any recent wall ≥ 1500 ms or any failure trips a 3 s hold that surfaces the new typed error `SimctlError::CaptureBackpressure { retry_after }`. Consumers whose gates already screenshot at ≥ 200 ms cadence are unaffected; tight loops slow to the pacer floor. This is the direct fix for the `SimRenderServer` `brk 1` triggering pattern on iOS 26.5.2 (25F84).
- **`SimctlClient::with_screenshot_pacer(cfg)`** builder + **`SimctlClient::with_sim_health(monitor)`** builder — wire the pacer's observations back to the sim-health monitor for global state classification.

### Added — CLI (feedback §A / §B / §E ask 3 / D8, D9, D5)

- **`smix runner cycle`** — new verb. Reads the current runner state, tears down (SIGINT + wait, preserves per-udid `derived-data-<udid>/`), brings up on the same device + port + bundle. Warm re-up in ~3 s vs cold ~15 s. Errors if no `state.json` exists (`runner up` for a cold start). Fulfills feedback §E ask 3.
- **`smix runner up` bundle validation** — refuses to boot without `--bundle`, prints a clear error + example. `SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1` bypasses the guard (opts back into the legacy Preferences default with an explicit warning). With `--bundle` set, logs `[runner] target bundle-id: <id>` at boot. Fulfills feedback §A preference (3).
- **`smix run --gate-signal <regex>` + `--gate-signal-timeout <ms>`** — prepends an implicit `expect.signal { regex, timeoutMs }` step at the START of the flow (index 0), blocking until the regex is observed in the metro log tail. Requires `--metro-log-url` also set. Symmetric to the existing `--await-signal` end-of-flow gate. Default timeout 60 s; zero disables. Replaces insight's `wait-metro-signal.ts` node-side helper. Fulfills feedback §B preference (1).

### Added — debug output (feedback §I / D11)

- **`--debug-output <dir>/step-<N>-<verb>.tree.json`** — on step failure, alongside the fail PNG the adapter now writes a full a11y-tree snapshot captured at the moment the step's expectation was evaluated. Turns "screenshot shows the text but assertVisible failed" mysteries into "here's exactly what the runner saw."
- **`run-summary.json` per-step trace** — the summary now carries `steps: [{n, verb, verdict, wallMs, jsonPath, pngPath?, treePath?, failureKind?, failureMessage?}]`. Populated for both success and failure runs (partial trace on failure preserved via a snapshot taken before the `?`-return early-exit).

### Added — session lifecycle (RFC 1.0.4 §D5 / D14 / D7)

- **`POST /session/close-all`** — closes every open session on the runner. Idempotent (`{ok, closed:N}`). Rust: `HttpRunnerClient::close_all_sessions()`.
- **`POST /session/relaunch-app {sessionId}`** — runner does `terminate() + launch()` on the session's cached `XCUIApplication` binding IN PLACE, preserving the session id and XCUITest binding. Returns `{ok, wallMs}`. Recovers from a downstream app crash without cycling the runner. Rust: `HttpRunnerClient::relaunch_session_app(&req)`; SDK: `Session::relaunch_app()` (Rust), `session.relaunchApp()` (TS / Swift / Kotlin).
- **`Session::state` + state stream/flow/event across all 4 SDKs (RFC 1.0.4 §D7).** The runner emits `X-Sim-Health: healthy|degraded|cycling|dead` on every response; SDKs parse it and surface transitions to consumers:
  - Rust — `Session::state() -> SessionState`.
  - TypeScript — `session.state` + `session.on('state', listener)`.
  - Swift — `session.state` + `session.stateStream: AsyncStream<SessionState>`.
  - Kotlin — `session.state` + `session.stateFlow: StateFlow<SessionState>`.

### Added — extended health (RFC 1.0.4 §D1)

- **Extended `GET /health` body** now includes `simRenderServer: {alive, pid}` and `xcodebuildTestHost: {alive, pid, restartCount}`. Legacy clients that only read `{ok:true}` continue to work.

### Added — safe-exit cascade (RFC 1.0.4 §D15 / lifecycle)

- **`smix run` SIGINT / SIGTERM handling.** `tokio::signal::ctrl_c()` and SIGTERM race against the flow execution; on signal the CLI aborts the in-flight flow, runs a best-effort `/session/close` under a 2 s timeout, prints `interrupted (SIGINT|SIGTERM) — running session-close cascade` on stderr, and exits with POSIX-conventional 130 (SIGINT) / 143 (SIGTERM). The Rust adapter's `--debug-output` partial-trace file still fires on interrupt so the last-attempted step is captured. Solves the "ctrl-C leaves a session hanging until runner idle-close fires" complaint.

### Fixed — `openLink` URL preservation (feedback §G / D13)

- **`SimctlClient::open_url` argv preservation** — verified byte-identical URL passthrough (`openurl_argv` test helper + 3 unit tests covering percent-encoded schemes, query params with `&`/`#`, unicode). The dev-launcher picker behavior insight reported on `expo-dev-client 57.0.5` is upstream (not smix); the finding lives on expo-dev-client's side and is documented for the record.

### Documented — feedback §D auto-resolution

- **`--activate` per-request cost** is auto-resolved for consumers who upgrade to v1.0.3 sessions (via `smix run` auto-session or explicit `Session.open`). The runner short-circuits `App-Activate: true` when a `Session-Id` header is present, so the "50-100 ms per request main-actor hop" feedback §D described no longer applies for session-mode flows. No code change needed; documented here so consumers know to prefer `--activate` inside a session rather than passing it per-request.

### Wire + ABI compatibility

- All additions are additive (routes, response fields, enum variants, SDK types).
- v1.0.4 clients work against v1.0.3 runners (missing routes → 404 → fall through; missing headers → `Session::state` stays `Healthy`).
- v1.0.3 clients work against v1.0.4 runners (extra fields / headers ignored).

### Verified builds

- Rust workspace (26 crates): fresh `cargo check --workspace --jobs 1` clean 3m06s.
- Swift Package: `swift build` clean; `xcodebuild build-for-testing -project SmixRunner.xcodeproj -scheme SmixRunner -destination 'generic/platform=iOS Simulator'` — `** TEST BUILD SUCCEEDED **`.
- Kotlin: `./gradlew :sdk:build` — BUILD SUCCESSFUL in 28s.
- TypeScript: `tsc --noEmit` clean.

### Deferred to v1.0.5 (independent charters)

- **§E ask 2 — session-persistence across XCTest lifecycle.** Needs a separate design for state serialization.
- **§D6 host-side XCTest supervisor** — auto-cycle-on-`TEST INTERRUPTED`. v1.0.4 provides the manual escape hatch (`smix runner cycle` verb) plus the programmatic detection surface (`Session::state` transitions via `X-Sim-Health` + `AppAliveCache` markDead from parsed XCTIssues); a fully-automatic supervisor daemon is v1.0.5 material.
- **Runner-side idle-close 120 s → 60 s tightening** — deferred; the client-side `smix run` SIGINT / SIGTERM cascade (§D15) already covers the primary orphaned-session case.



Session lifecycle at the runner boundary. Building on v1.0.2's rate-limited activation, v1.0.3 lets consumers open a session at the start of a flow, run the entire flow against a cached `XCUIApplication` binding, and close on exit — no per-request activation. This is the systemic fix that supersedes the interim rate-limit; the legacy per-request path stays as a fallback.

### Added

- **Session routes on the iOS runner** — `POST /session/open {bundleId, activate}` returns `{sessionId, activatedOnce, serverTimeMs}`; `POST /session/close {sessionId}` (idempotent) returns `{ok}`; `POST /session/renew-activation {sessionId}` returns `{ok, activated}` subject to a 2 s per-session rate limit. Wire types available on `smix-runner-wire` since v1.0.2; runner-side handlers land in v1.0.3.
- **`Session-Id` header** on every runner request. When present, `resolveApp()` short-circuits to the session's cached binding — no per-request activation regardless of `App-Activate`.
- **Rust SDK `Session`** — `App::open_session(bundle_id, activate) -> Session`. Consumer flow: `let session = app.open_session("com.example.app", true).await?; session.app().tap(...).await?; session.close().await?;`. `Session::renew_activation()` for consumer-driven drift recovery.
- **TypeScript SDK `Session`** — `Session.open(runner, "com.example.app", { activate: true })` on any `HttpRunnerClient`-shaped runtime. Consumers pair with `try / finally { await session.close() }`.
- **Swift SDK `HttpSmixSimRuntime` + `Session`** — URLSession-backed `SmixSimRuntime` implementation speaking the SmixRunnerCore wire directly, with session-aware header attachment. `Session.open(runtime, activate: true)` acquires a session; `session.close()` releases. Every request from the runtime while the session is open carries `Session-Id`.
- **Kotlin SDK `HttpSmixSimRuntime` + `Session`** — java.net.HttpURLConnection-backed runtime (no additional dependencies beyond the existing kotlinx-serialization-json), same wire contract. `Session.open(runtime, activate = true)` / `session.close()`. Thread-safe on the session-id field via `AtomicReference`.
- **`smix run` opens a session automatically** — every CLI invocation opens a session at start, closes on exit. Runners that don't implement `/session/open` (v1.0.x pre-1.0.3) return non-2xx; the CLI emits a WARN and falls through to the legacy per-request path (rate-limited since v1.0.2, so still safe).

### Wire + ABI compatibility

- All new routes are additive
- All new SDK types are additive (`Session`, `SessionOpenRequest`, etc.)
- v1.0.x clients keep working against v1.0.3 runners (Session-Id header optional)
- v1.0.3 clients work against v1.0.2 runners with a WARN + legacy fallback

## [1.0.2] — 2026-07-09

### Fixed

- **Runner activation storm** — the XCUITest-side `resolveApp()` no longer calls `.activate()` on every request when `App-Activate: true` is set. Instead, `.activate()` runs at most once per bundle-id per 5 s. Long-running gates (visual / perf regression, ~340 s of continuous requests against the runner) previously accumulated ~1000+ activate calls, exhausting XCTest process arbitration on iOS 26.5+ and crashing `test_runForever()` mid-run. Recovery semantics preserved: after 5 s of silence a subsequent activate hint is honored, so a foreground steal by SpringBoard is auto-recovered within the same window.
- **Simulator screenshot PNG colorspace metadata** — `xcrun simctl io <udid> screenshot` on iOS 26.5 sub-builds started omitting the `sRGB` ancillary chunk from its PNG output. macOS Preview.app and other viewers fall back to Display P3 in the absence of an embedded ICC profile, over-saturating red and adding yellow anti-alias fringing on text. `SimctlClient::screenshot` now byte-splices a synthesized `sRGB` chunk (rendering intent = 0, perceptual) into the PNG stream immediately before the first IDAT when none is present. IDAT bytes are never decoded or modified — pixel-comparison consumers (dhash, hamming) see byte-identical decoded pixel arrays.

### Added

- **Runner liveness observability** (Rust client) — `HttpRunnerClient::with_liveness_window(N)` opts in to rolling-window request outcome tracking. If a majority of the last N requests failed, subsequent calls surface `RunnerTransportError::RunnerDegraded { window, non_success_recent, last_endpoint, last_error }` instead of returning silent stale bodies. Any transport-level `is_connect()` error additionally probes `/health` with a 1 s timeout; if the runner is unreachable, subsequent calls surface `RunnerTransportError::RunnerDied { last_seen_ms, last_error }`.
- **Extended `GET /health` body** — the runner-side JSON response now includes `runnerVersion`, `uptimeMs`, `lastRequestAtMs`, `sessionsOpen`, and `activationsTotal`. Legacy clients that jq-parse `{"ok":true}` continue to work — the extended body is a superset. The Rust client's `HttpRunnerClient::health_detail()` parses the new fields.
- **Wire types for session lifecycle** — `smix-runner-wire` exports `SessionOpenRequest / SessionOpenResponse / SessionCloseRequest / SessionCloseResponse / SessionRenewActivationRequest / SessionRenewActivationResponse`. The Rust client (`HttpRunnerClient::open_session`, `close_session`, `renew_session_activation`) can drive these when a runner implements the endpoints; the corresponding runner-side routes are queued for v1.0.3.

## [1.0.1] — 2026-07-09

### Fixed

- **Parser** — `smix run` now accepts the `expect: { visible: <selector>, timeoutMs?: N }` and `expect: { notVisible: <selector>, timeoutMs?: N }` shapes emitted by `smix migrate` for `extendedWaitUntil`. The `expect: { visible: ... }` shorthand (no timeout, equivalent to `assertVisible`) is likewise accepted. Previously the parser only recognized the top-level `expect: { text | id: ... }` maestro-alias form, so codemodded corpora failed at run time with `expected 'text' or 'id' key`. Regression tests in `smix-adapter-maestro/tests/parser.rs` pin every accepted shape.
- **`smix migrate --help`** — help text corrected to state that comments, copyright headers, and blank lines survive the codemod byte-identical (matches 1.0.0's actual behavior).

### Added

- **`smix run --check`** — parse-only pre-flight. Reads every listed flow YAML and reports parse or include errors without connecting to a runner or booting a simulator. Exit 0 on clean parse across every flow; non-zero (2) on any error. Suitable for CI without simulator infrastructure.

## [1.0.0] — 2026-07-08

First public release.

### Added

- **CLI** — `smix` binary with subcommands `run`, `sim`, `runner`, `migrate`, `annotate`, `authoring`, `tree`, `find`, `tap`, `fill`, `clear`, `scroll`, `screenshot`, `describe`, `doctor`.
- **Rust SDK** — `smix-sdk` crate exposing the `App`, `Selector`, `KeyName`, and `Runtime` types plus a fluent builder for connection configuration.
- **TypeScript SDK** — `@goliapkg/smix` on npm; Playwright-shape API surface mirrored to the Rust SDK.
- **Swift SDK** — Swift Package published as a GitHub Release; provides a prebuilt `SmixCoreFFI.xcframework`.
- **Kotlin SDK** — `jp.golia.smix:smix-sdk` on Maven Central; UiAutomator-backed runner for the Android Emulator.
- **YAML runtime** — Maestro-compatible YAML syntax accepted directly (both maestro-canonical `tapOn` and smix-canonical `tap` forms).
- **Codemod** — `smix migrate` rewrites YAML from maestro-canonical to smix-canonical while preserving comments, copyright headers, and blank lines byte-identical.
- **Fixture registry** — `--fixture-registry <file.ts|file.json>` enables the `- fixture: <id>` YAML verb.
- **Metro log signals** — `expect.signal`, `expect.signals`, `expectLogClean`, and the `--metro-log-url ws:// | file:// | -` transport with configurable allowlists.
- **Annotation** — bundled Inter Regular and Noto Sans SC fonts; the `takeScreenshot` verb accepts `annotate:` clauses composing `circle`, `line`, `arrow`, `text`, and `box` primitives; `smix annotate` standalone CLI.
- **Auto-annotate on failure** — `--debug-output` fail-step PNGs receive an automatic red circle, step label, and summary; opt out with `--no-fail-annotate`.
- **JUnit output** — `smix run --format junit --output report.xml` writes a JUnit-XML testsuite consumable by common CI pipelines.
- **Authoring tier** — `smix authoring suggest`, `capture-tree`, `diff-tree`, and `record` for authoring flows against a live simulator or emulator.
- **Standard subflows** — bundled `std/wipe-app-state.yaml`, `std/wait-metro-bundle.yaml`, `std/quit-qa-mode.yaml`, `std/dismiss-open-in.yaml`, and `std/ensure-locale.yaml`.
- **MCP server** — `smix mcp` subcommand exposes the SDK surface to Claude Code and other MCP-aware clients.

### Stability commitments

- Wire format frozen — any breaking wire change is a v2.0 release.
- ABI frozen for the ten core "stone" crates (`smix-error`, `smix-selector`, `smix-screen`, `smix-runner-wire`, `smix-input`, `smix-verbs`, `smix-metro-log`, `smix-fixture`, `smix-annotate`, `smix-migrate`) — additive changes only within v1.x.
- All CLI flags shipped in v1.0 remain accepted for the v1.x lifetime.
- The YAML verb table (`smix-verbs`) is the single source of truth; removing a verb is a major-version change.

See [`docs/ai-guide/wire-format.md`](./docs/ai-guide/wire-format.md) and [`docs/ai-guide/abi-stability.md`](./docs/ai-guide/abi-stability.md) for the full contracts.
