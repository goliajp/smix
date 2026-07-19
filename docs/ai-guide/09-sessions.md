# 09 — Session lifecycle

Sessions solve the "activation storm" problem on long-running flows: without a session, every request that sets `App-Activate: true` triggers an `.activate()` call on the runner side, and hundreds of activations across a multi-minute gate can exhaust XCTest's process arbitration on iOS 26.5+ and crash `test_runForever()`.

With a session:

- The runner activates once at open time (or not at all — the client picks).
- Every subsequent request from the same client carries `Session-Id: <id>` and hits the session's cached `XCUIApplication` binding directly.
- No per-request activation regardless of what `App-Activate` says.
- The runner exposes an explicit `POST /session/renew-activation` escape hatch for drift recovery, rate-limited to at most one activation per 2 s per session.

Sessions are mandatory on iOS in v2 — the legacy per-request rebind path is gone. Driving without a session is a named error (`no session id on the client`), never a silent fall-back to the path the session model exists to remove. Android has no session concept and drives sessionless.

## When to use

- **`smix run`** — the CLI opens a session for you automatically at start, closes on exit. Zero yaml changes.
- **Rust SDK** — call `App::open_session(bundle_id, activate)` at the top of your flow, use `session.app()` for all subsequent calls, `session.close().await` at the end.
- **TypeScript SDK** — `Session.open(runner, bundleId, { activate: true })`, pair with `try / finally { await session.close() }`.
- **Swift SDK** — `Session.open(driver, bundleId:)` on a `SmixDriver`. Pair with `defer { Task { try? await session.close() } }`.
- **Kotlin SDK** — no public session handle; `Smix.launchApp` opens one and owns it. Pair with `try { ... } finally { app.terminate() }`.

## Rust SDK

```rust
use smix_sdk::{App, text};

let app = App::connect_to_runner(22087).await?
    .with_bundle_id("com.example.app");

let mut session = app.open_session("com.example.app", true).await?;
// Every call below carries `Session-Id` — no activation storm.
session.app_mut().tap(&text("Sign In")).await?;
session.app_mut().fill(&text("Email"), "user@example.com").await?;
session.app_mut().assert_visible(&text("Dashboard")).await?;

// Explicit drift recovery (optional; rate-limited to 1 / 2 s).
if some_drift_condition {
    let _activated = session.renew_activation().await?;
}

// Release. Sends `POST /session/close`, clears the client-side header.
let _app = session.close().await?;
```

If you skip `close()`, `Drop` clears the client-side `Session-Id` header on a best-effort basis. It cannot `await` a network call, so the runner-side entry is only cleaned up on runner restart. Prefer explicit `close()`.

## Android

The Kotlin runner serves the same `/session/*` surface (open / close /
close-all / list / launch-app / terminate-app / relaunch-app /
renew-activation), backed by an in-memory table and `am` commands.
`smix run` still drives Android sessionlessly — the session surface is
there for the SDKs that open one explicitly.

## TypeScript SDK

Session lifecycle is fully wired in the TypeScript package; driving
(`Smix.launchApp` and the `App` act/sense methods) is not — those throw
`SmixNotImplementedError` until the native transport lands. What works:

```ts
import { Session, HttpSimRuntime } from '@goliapkg/smix'

const runtime = new HttpSimRuntime('http://127.0.0.1:22087')
const session = await Session.open(runtime, 'com.example.app', { activate: true })
try {
  await session.relaunchApp()
} finally {
  await session.close()
}
```

The `HttpSimRuntime` picks up the `Session-Id` header via `setSessionId` inside `Session.open` — the wiring is invisible to callers.

## Swift SDK

```swift
import SmixSDK

// Smix.launchApp opens the session for you; open one directly only
// when you want the handle before launching.
let driver = SmixDriver(port: Smix.defaultRunnerPort)
let session = try await Session.open(driver, bundleId: "com.example.app")
do {
    let app = try await Smix.launchApp(.bundleId("com.example.app"))
    try await app.tap(.text("Sign In"))
    try await session.close()
} catch {
    try? await session.close()
    throw error
}
```

`SmixDriver` comes from the UniFFI-generated bindings and carries every sense / act call over the runner's HTTP surface. The Swift SDK holds no simulator I/O of its own.

## Kotlin SDK

```kotlin
import dev.smix.sdk.*

// Kotlin has no public session handle: Driver's implementation and
// App's session field are both `internal`. Smix.launchApp opens a
// session and owns it for the App's lifetime.
try {
    val app = Smix.launchApp(AppTarget.BundleId("com.example.app"))
    app.tap(Selector.Id("btn-login"))
} finally {
    app.terminate()
}
```

Kotlin's driver wraps the same UniFFI-generated core the Swift SDK uses, so both languages speak one Rust wire client rather than a per-language HTTP client. It is `internal` on purpose — `Smix.launchApp` is the supported entry.

## Session state + relaunch-app

Sessions expose a `state` classification driven by the runner's `X-Sim-Health` response header, and a `relaunch_app()` primitive for in-place app-crash recovery.

### State

`SessionState` values:

- `healthy` — all watched signals inside envelope
- `degraded` — screenshot slow-path, `/health` age between stale and dead thresholds, or `/system-popups` throttled
- `cycling` — runner supervisor is mid auto-restart; wait for `healthy` before retrying failed calls
- `dead` — SimRenderServer or xcodebuild is gone; bail out

Consumers subscribe:

```rust
// Rust
match session.state() {
    SessionState::Healthy => { /* proceed */ }
    SessionState::Degraded => { /* pause the gate loop */ }
    SessionState::Cycling => { /* wait for healthy */ }
    SessionState::Dead => { /* abort */ }
}
```

```ts
// TypeScript
session.on('state', (state) => {
  if (state === 'degraded') pauseGate()
  if (state === 'dead') abortGate()
  if (state === 'cycling') waitFor('healthy')
})
```

State observation exists in Rust (`session.state()`) and TypeScript
(`session.on('state', …)`, fed by `HttpSimRuntime.attachSessionState`).
The `SmixDriver`-backed Swift and Kotlin sessions carry no state
stream — poll `GET /health`, or watch the supervisor, for the same
signal there.


### Relaunch app

When the target app crashes but the runner is still healthy, `relaunch_app()` does an in-place `terminate() + launch()` on the session's cached binding — session id + XCUITest binding preserved, no runner cycle needed.

```rust
// Rust
let wall_ms = session.relaunch_app().await?;
```

```ts
// TypeScript
const wallMs = await session.relaunchApp()
```

```swift
// Swift
let wallMs = try await session.relaunchApp()
```

```kotlin
// Kotlin
val wallMs = session.relaunchApp()
```

## CLI

`smix run` opens a session automatically. No new flags — the session is just there under the hood:

```bash
smix run visual.yaml --device ios-17 --activate --bundle-id com.example.app
```

Behind the scenes:

1. CLI POSTs `/session/open` with `{bundleId, activate}`, gets a session id.
2. Every subsequent request in the flow carries `Session-Id`.
3. On exit, CLI POSTs `/session/close`.

If `/session/open` returns a non-2xx — a runner too old to open one — `smix run` prints the reason and exits 6. There is nothing to fall through to: v2 drives iOS through a session or not at all. Re-extract the runner this CLI ships with (`smix runner install --force`) and retry.

## Wire

- `POST /session/open  {bundleId, activate}` → `200 {sessionId, activatedOnce, serverTimeMs}`
- `POST /session/close {sessionId}` → `200 {ok}` (idempotent)
- `POST /session/renew-activation {sessionId}` → `200 {ok, activated}` (activated=false when rate-limited); `404 not_found` when the session id is unknown.

Client header on subsequent requests: `Session-Id: <sessionId>`.

## Semantics

- **One session per bundle-id per client.** Opening a second session for the same bundle from the same client just gives you a new id — the runner tracks both bindings independently but they alias the same `XCUIApplication`.
- **Session is per-connection, not per-flow.** If you close and reopen your `HttpRunnerClient`, you need a fresh session; the runner has no way to associate two connections.
- **Runner restart drops all sessions.** A subsequent request with a stale `Session-Id` falls through to the legacy per-request rebind path (rate-limited).
- **Renew is rate-limited.** At most one `.activate()` per session per 2 s. If you call `renew_activation()` inside that window, `activated` comes back `false`; the session is still healthy.
