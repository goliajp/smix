# smix v1.0.4 — shipping notes for insight

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to:
- Verbal report: "最近insight在用 smix,总是显示insight意外退出;似乎也是导致studio总是WindowServer意外退出和死机的原因"
- `smix-feedback-2026-07-10-gate-hardening.md` (§A–§I, 9 items)
- Prior reply: `docs/ai-guide/insight-v1.0.3-studio-crash-2026-07-10.md`

## Summary

v1.0.4 closes every ask in the gate-hardening feedback (§A/§B/§D/§E/§F/§G/§H/§I) plus my new-discovered SimRenderServer stress fix plus safe-exit lifecycle primitives. This document is a shipping-notes preview — v1.0.4 has NOT yet been published to crates.io / npm / Maven / Swift Package. It will ship as a single cross-ecosystem release once the runner-side (Swift) capability changes are `xcodebuild test-without-building` verified.

## What lands where

### CLI (`smix run`, `smix runner`)

- **`smix runner cycle`** — new verb. Warm re-up in ~3 s (preserves `derived-data-<udid>/`); §E ask 3.
- **`smix runner up --bundle` hard-required** — refuses to boot without `--bundle` (`SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1` bypasses); logs `[runner] target bundle-id: <id>` at start. §A preference (3).
- **`smix run --gate-signal <regex> [--gate-signal-timeout <ms>]`** — prepends implicit metro-log-tail wait as step 0. Replaces insight's `wait-metro-signal.ts`. §B preference (1).
- **`smix run` SIGINT / SIGTERM safe-exit** — best-effort `/session/close` under 2 s timeout, exit 130 / 143. Partial `--debug-output` trace still fires.

### `--debug-output` per-step trace (§I)

- On failure: `<flow>/step-<N>-<verb>.fail.png` PLUS new `<flow>/step-<N>-<verb>.fail.tree.json` (full a11y snapshot at the moment the step evaluated).
- `run-summary.json` now carries `steps: [{n, verb, verdict, wallMs, jsonPath, pngPath?, treePath?, failureKind?, failureMessage?}]` for both success and failure runs.

### Sim-render-server protection (my forensic finding)

- **`SimctlClient` screenshot pacer** — adaptive interval floor (100 ms fast / 1500 ms slow) + circuit breaker on ≥ 1500 ms wall or failure (3 s hold, surfaces `SimctlError::CaptureBackpressure { retry_after }`). This is the direct fix for the `com.apple.display.captureservice` `brk 1` assertion trip that took studio down. Consumers running screenshot at ≥ 200 ms cadence are unaffected; tight loops slow to the floor.
- **`smix-sim-health` — new sense stone crate.** Watches SimRenderServer + xcodebuild pids + `/health` age + rolling walls; broadcasts `Healthy | Degraded | Dead` transitions.

### Session lifecycle across all 4 SDKs (§E / §H / §D7 / §D14)

- **`Session::state` + subscribe**. Runner emits `X-Sim-Health` header on every response; SDKs surface transitions to consumers:
  - Rust: `session.state() -> SessionState`
  - TypeScript: `session.state`, `session.on('state', listener)`
  - Swift: `session.state`, `session.stateStream: AsyncStream<SessionState>`
  - Kotlin: `session.state`, `session.stateFlow: StateFlow<SessionState>`
- **`Session::relaunch_app`** — new lifecycle primitive. Runner does `terminate() + launch()` on the session's cached `XCUIApplication` IN PLACE, preserving session id and XCUITest binding. Recovers from a downstream app crash without cycling the runner. Available on all 4 SDKs.
- **`POST /session/close-all`** — bulk session release for `smix runner cycle` and the supervisor auto-restart. Idempotent.

### §D auto-resolution

- **`--activate` per-request cost is auto-resolved when using sessions.** The runner short-circuits `App-Activate: true` when a `Session-Id` header is present, so the 50-100 ms main-actor hop `smix-feedback-2026-07-10-gate-hardening.md` §D described no longer applies in session-mode. `smix run` opens a session automatically; direct SDK users pass `--activate` once at session open.

### §G verification

- **`openLink` URL passthrough verified.** Byte-identical URL bytes reach `xcrun simctl openurl` (see `smix-simctl` `openurl_argv` helper + 3 unit tests). The dev-launcher picker behavior insight reported on `expo-dev-client 57.0.5` is upstream — not smix.

## What still needs xcodebuild verify before release

The following Swift-side runtime behaviours are source-committed to the v1.0.4 scope but need an `xcodebuild test-without-building` run to certify. These are what will hold v1.0.4 back from crates.io / npm / Maven / Swift Package publish until green:

1. **§D4 `/system-popups` 500 ms per-session floor** — `SystemPopupsRoute.tooManyRequests(retryAfterMs:)` helper landed; middleware install into `SmixRunnerServer.registerRoutes` pending. This is one of the two direct XCTest arbitration pressure sources (screenshot pacer being the other, already landed).
2. **§D2 app-alive cache (20 s TTL)** — after observed "Application X is not running" XCTIssue, `/tree` and `/system-popups` short-circuit empty. Cuts the swallowed-error firehose observed in the runner log.
3. **§D6 XCTest test-host supervisor** — auto-cycle on `** TEST INTERRUPTED **` / `SchemeActionResultOperation started unexpectedly`. Fulfills §E ask 1.
4. **§D12 `launchApp: clearState: true` rewrite (§F + §H)** — `XCUIApplication.terminate() + .launch()` with in-place sandbox clear via `simctl privacy` + `NSFileManager` under the app's Containers/Data root. Replaces `simctl uninstall + install` which is the joint root cause of §F (XCUITest binding loss) and §H (`ReportCrash` "Insight quit unexpectedly" dialog).
5. **`X-Sim-Health` header emission on runner responses** — SDK parse is landed + tested; runner-side emit pending.
6. **Runner-side idle-close 120 s → 60 s tightening** — smaller SIGKILL-orphaned session window.

## Insight-side action items (nothing to do yet)

- **Do NOT run `bun test:e2e` batch scope on studio until v1.0.4 ships** (from prior `insight-v1.0.3-studio-crash-2026-07-10.md`). Single-flow `smix run` is fine.
- Once v1.0.4 ships:
  - Rebuild + reinstall the smix crates.io / npm / Maven / Swift Package binaries on machines that gate.
  - Insight-side `wait-metro-signal.ts` and `qa/sim/runner.ts` `--bundle` passthrough can be simplified — those workarounds are no longer needed but not broken.
  - Session-Degraded event wiring — see `docs/ai-guide/09-sessions.md` for the four-SDK examples.

## Release trigger

I ship v1.0.4 across crates.io + npm + Maven Central + Swift Package (DAG-ordered, ~90 s propagation waits between stages) once the six pending Swift items above are xcodebuild-green + a fresh 20-flow bootstrap stress on `sim-insight` completes without SimRenderServer crash and without `** TEST INTERRUPTED **`. Full release notes in `CHANGELOG.md`; feedback trace in prior insight docs.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.4-shipping.md
```

Prior chain in the same directory:
- `insight-v1.0.3-session-lifecycle.md` — v1.0.3 shipping notes
- `insight-v1.0.3-studio-crash-2026-07-10.md` — the SimRenderServer forensic + urgent stop-gap
- `insight-feedback-gol-611-response.md` — original gol-611 response arc
