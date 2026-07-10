# Changelog

All notable changes to the `smix` workspace are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) at the wire, ABI, and CLI surface.

## [1.0.8] — 2026-07-11

Eliminate the "Insight quit unexpectedly" ReportCrash system dialog. Response to `smix-feedback-2026-07-11-blocking-crash-dialog.md` — escalated hard-requirement. RFC `.claude/rfcs/1.0.8-crash-dialog-elimination-and-a11y-cache.md`.

### Root cause revisited

v1.0.4 §D12 replaced `simctl uninstall + install` with an in-place clear (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`). Insight reported the dialog STILL fired. Diagnosis: even without the uninstall, `simctl terminate` sends SIGKILL to the target, which `com.apple.ReportCrash` on iOS 26.5 sim treats as a crash. The whole `simctl` termination pathway is what triggers the dialog — not just the uninstall.

The systemic answer: move termination + launch INSIDE the XCUITest runner process via `XCUIApplication.terminate()` / `.launch()` (cooperative via `testmanagerd`; does NOT signal ReportCrash). The sandbox wipe stays on the host via `SimctlClient::clear_app_sandbox` but ONLY after the cooperative terminate, so ReportCrash was never signalled.

### Runner-side (Swift)

- **`POST /session/terminate-app { sessionId }`** → cooperative `XCUIApplication.terminate()` on the session's cached binding. testmanagerd stop; no SIGKILL; no ReportCrash signal.
- **`POST /session/launch-app { sessionId }`** → cooperative `XCUIApplication.launch()`. Fresh instance sees whatever sandbox state the SDK left for it.
- Both are additive routes; v1.0.7 runners return 404 and consumers should either upgrade the runner or route through the legacy `Session::relaunch_app`.

### CLI + adapter (Rust)

- **New yaml verb `clearAppData`** — session-scoped in-place data clear. Bare verb, no args. Maps to `App::clear_app_data` which orchestrates the 3 steps host-side. Requires an open session (auto-populated by `smix run`).
- **`App::clear_app_data() → Result<wall_ms>`** on the Rust SDK. Grabs `session_id` + `bundle_id` from the driver + `udid` from `App::require_udid`; calls `runner.terminate_session_app` → `simctl.clear_app_sandbox` → `runner.launch_session_app`.
- **`Session::reset_app_data()`** — thin ergonomic wrapper on `App::clear_app_data`, for consumers who hold a `Session` handle directly.
- **`launchApp: clearState: true` NOT yet deprecated** in this cycle — legacy shape still runs the pre-v1.0.8 `LaunchFreshOp` sequence. Consumers migrating to `clearAppData` get the crash-dialog fix; consumers who keep the legacy shape stay unaffected until v1.0.9 flips the default.

### Wire additions

- `SessionAppLifecycleRequest` / `SessionAppLifecycleResponse` in `smix-runner-wire`.
- `HttpRunnerClient::terminate_session_app(req)` / `launch_session_app(req)` on the Rust client.

### Deferred to v1.0.9

- **Adaptive app-alive cache re-probe** (originally D4 of this RFC; parked because the crash-dialog fix is enough to unblock insight's gate and the a11y-cache work has its own testing surface).
- **Supervisor `RunnerCycled` reason with log context** (D5).
- **Deprecation of `launchApp: clearState: true`** — emit WARN + auto-expand to `clearAppData + launchApp: {}`. Deferred because the deprecation needs a full-corpus consumer migration and we want insight to migrate their subflows first on their own timeline.

### Wire + ABI compatibility

- Additive routes; v1.0.7 runners return 404 on the new endpoints.
- Additive `Step::ClearAppData` variant on the yaml Step enum; `#[non_exhaustive]` was already in play (via yaml deserialization), so consumers using pattern matching are unaffected.



Systemic observability + subprocess integrity. RFC `.claude/rfcs/1.0.7-observability-layer.md`. Response to `smix-feedback-2026-07-11-v1.0.5-followup.md` items A, B, D.3 — three feedback points share one root cause: smix is opaque about its own runtime.

### Subprocess integrity (RFC §D1 + D2)

- **`SimctlClient::clear_app_sandbox` uses `/bin/rm`** (not `"rm"`). `xcrun simctl spawn <UDID> <cmd>` uses `posix_spawn` inside the sim; PATH resolution is NOT run, so a bare command name fails `NSPOSIXErrorDomain code 2: No such file or directory` on iOS 17+ sims. This is the direct root cause of insight's v1.0.5 §B ENOENT failure on `launchApp: clearState: true` mid-flow. `current_locale` + `set_locale` similarly use `/usr/bin/defaults`.
- **`SimctlError::NonZeroExit` extended with `argv: Vec<String>` + `wall_ms: u64`**. Display impl now surfaces every arg simctl was asked to run — `xcrun simctl spawn <UDID> /bin/rm -rf /Users/.../Documents ... exited 2 (312ms): ...` — instead of just the subcommand name. Consumers reading the error know exactly what smix asked simctl to do.
- `SimctlError` marked `#[non_exhaustive]`; `SimctlError::non_zero_exit(sub, code, stderr)` helper for callers translating foreign errors.

### Observability surface (RFC §D3 + D4 + D5)

- **Ring buffer of recent `simctl` invocations** (capped 128; oldest evicted). Public accessor `smix_simctl::recent_subprocesses() -> Vec<SubprocessRecord>` — `argv`, `exit_code`, `wall_ms`, `stderr_head` (first 256 bytes), `timestamp`.
- **`POST /diagnostic/dump`** runner-side route — snapshot of `{ sessions, simHealth, supervisorPid, uptimeMs, recentSubprocesses }`.
- **`smix diagnostic dump [--json]`** CLI verb — calls `/diagnostic/dump` on the runner, merges with the client-side ring, pretty-prints a runtime post-mortem view. `--json` for CI consumption. Legacy runners (v1.0.6-) return 404; CLI degrades gracefully to client-side ring only.
- `HttpRunnerClient::diagnostic_dump()` Rust client method.

### Streaming discipline (RFC §D6)

- **`smix runner supervise` flushes stdout after every `RunnerCycled` JSON event**. Fixes insight §D.3 — supervisor events reach the consumer's parser even when the outer flow crashes fast right after a cycle.

### Cold-rebuild progress banner (RFC §D7)

- **`smix runner up` prints an explicit cold vs warm banner**. Detects warm by checking `.smix/runner/derived-data-<UDID>/` presence + populated. Cold path prints `COLD REBUILD expected up to 10 minutes` and emits a `xcodebuild still working (Ns elapsed)` heartbeat every 30 s. Warm path prints `warm rebuild ~3 s expected`. Fixes insight §A (their `spawnSync` timeout=300s tripped during cold recompile after version bump; they bumped to 600 s but had no visible progress signal).

### Related regression fix

- `smix-sdk/tests/launch_fresh_plan.rs` was pre-v1.0.4; asserted `Uninstall+Install` on the default clear_state path. v1.0.4 §D12 flipped the default to in-place (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`); tests updated to match shipping behaviour. Force-reinstall path exercised via `plan_launch_fresh_calls_v2(true)`.

### Wire + ABI compatibility

- All wire additions additive. `POST /diagnostic/dump` on runners < v1.0.7 returns 404; CLI degrades gracefully.
- `SimctlError` is `#[non_exhaustive]`; construction sites updated to fill new fields via `non_zero_exit` helper.



Sidecar supervise + symmetric down-cascade + rust 1.97 baseline. Follow-up to v1.0.5 folding the supervisor's spawn-and-teardown into the runner lifecycle so consumers who want automatic `TEST INTERRUPTED` recovery just add `--supervise` to their existing `smix runner up`. RFC `.claude/rfcs/1.0.6-supervise-sidecar-and-runner-down-cascade.md`.

### CLI (Rust)

- **`smix runner up --supervise`** — after `/health` returns 200, spawn a detached `smix runner supervise` process, redirect stdout/stderr to `.smix/runner/supervise-<UDID>.log`, and record its pid in `state.json` under a new `supervisorPid` field. Sidecar runs in its own process group so a ctrl-C on the CLI doesn't tear it down.
- **`smix runner down` cascades supervisor teardown.** Before the xcodebuild SIGINT, `down` reads `state.json` and if a `supervisorPid` is present + still matches a `smix runner supervise` process, sends SIGTERM (5 s), escalates to SIGKILL if needed. `down` invoked from inside the supervisor itself (re-entrant case, during auto-cycle) skips the self-kill.
- **`smix runner cycle` preserves the sidecar flag.** If the pre-cycle `state.json` records a supervisor, the post-cycle `up` re-attaches one. Consumers who ran `up --supervise` get supervision back automatically after a cycle.

### Runner state schema (backward-compatible)

- `state.json` gains optional `supervisorPid: u32` field via `#[serde(default)]`. State files written by v1.0.5 or earlier deserialize without change.

### Workspace hygiene

- `rust-version = "1.97"` in the workspace `Cargo.toml`. Baseline bump for the `if let` chain stabilizations + std ergonomics. Consumers on `cargo install` see no change (prebuilt binary); consumers building from source now need rustc 1.97+.

### Documentation

- CHANGELOG format going forward groups entries under `### CLI (Rust)`, `### Runner-side (Swift)`, `### SDK — all four`, `### Documentation`, `### Deferred`. First entry using the new pattern; retroactive edit of v1.0.4/v1.0.5 not required.

### Deferred (v1.0.7+)

- **Opportunistic 1.97 idiom cleanups.** RFC §D3 flagged a handful of nested `if let` sites that collapse under 1.97's chain stabilizations. Not a functional change; queued as a hygiene sweep for a slow release cycle.

### Wire + ABI compatibility

- No wire additions.
- No SDK ABI additions.
- CLI additions are opt-in via `--supervise`; the classic path is unchanged.



Session persistence across XCTest lifecycle, host-side XCTest supervisor daemon, runner idle-close sweep, and the release smoke gate script. RFC `.claude/rfcs/1.0.5-supervisor-and-persistence.md`. Closes the three v1.0.4 deferrals + the "shipped on build-green only" gap.

### Added — session persistence (RFC §D1)

- **`POST /session/list`** → `{sessions: [{sessionId, bundleId, openedAtMs, lastActivatedAtMs}]}`. Rust: `HttpRunnerClient::list_sessions()`. CLI: `smix runner list-sessions` (pretty-printed table).
- **`Session::still_valid()` on all 4 SDKs** — probes `/session/list` and returns `true` iff the runner still knows this session id. Consumers wire it after a `Session::state` transition to `Cycling` or `Dead` to decide whether to keep using the session (§D1 preserves them across cycles) or reopen.
- **Runner-side persistence** — session table serializes to `~/Documents/smix-sessions.json` inside the sim on every mutation via `Data.write(.atomic)` (atomic-rename write). Boot rehydrates whatever's there, rebuilding each `XCUIApplication(bundleIdentifier:)` fresh (no `.activate()` call — the client's next request drives that). `smix runner cycle` preserves the file, so consumer `Session-Id` survives the cycle transparently.

### Added — supervisor daemon (RFC §D2)

- **`smix runner supervise [--runner-project <path>]`** — foreground process that tails `.smix/runner/runner-<UDID>.log`, matches interrupt patterns (`** TEST INTERRUPTED **`, `SchemeActionResultOperation started unexpectedly`), and auto-invokes `runner::cycle()` on hit. Backoff: 60 s per-cycle cooldown. Circuit breaker: 5 cycles in 10 minutes → exit non-zero so a monitoring layer can escalate. Emits `{"event":"RunnerCycled","reasonMatched":"...","atMs":N}` JSON on stdout per cycle. Fulfills feedback §E ask 1.

### Added — idle-close sweep (RFC §D3)

- **Runner-side session idle-close** — `SessionEntry` gains `lastAccessedAt`; `resolveApp()` refreshes it on every `Session-Id` hit. Detached `Task.detached` in `test_runForever` reaps sessions whose `lastAccessedAt` is older than 60 s every 15 s. Half-orphaned client sessions (SIGKILL wipes client without close) vanish within 60-75 s instead of accumulating until runner restart. Emits a stderr line on non-zero reap for operator visibility.

### Added — release smoke gate + ship script (RFC §D4)

- **`scripts/release/smoke-v1.smoke.sh` + `.smoke.yaml`** — real-sim gate exercising every net-new v1.0.4/v1.0.5 code path: pacer floor (`takeScreenshot × 10`), `--debug-output` `fail.tree.json` emit on a deliberate `assertVisible` fail, `runner cycle` + `/session/list` persistence, supervisor 5 s alive check. Requires jq + a booted sim.
- **`scripts/release/ship.sh <version> [--i-know-what-im-doing]`** — DAG-ordered 4-ecosystem publisher, refuses to run unless the smoke gate has passed in the last hour. Bypass flag is an audit-visible knob, not a silent default.

### Wire + ABI compatibility

- All additions are additive (routes, response fields, CLI verbs).
- v1.0.5 clients work against v1.0.4 runners (missing `/session/list` → 404; SDK `Session::still_valid()` propagates the error and consumers treat as invalid).
- v1.0.4 clients keep working against v1.0.5 runners.



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
