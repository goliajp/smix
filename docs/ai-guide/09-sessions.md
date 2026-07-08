# 09 — Session lifecycle

Sessions solve the "activation storm" problem on long-running flows: without a session, every request that sets `App-Activate: true` triggers an `.activate()` call on the runner side, and hundreds of activations across a multi-minute gate can exhaust XCTest's process arbitration on iOS 26.5+ and crash `test_runForever()`.

With a session:

- The runner activates once at open time (or not at all — the client picks).
- Every subsequent request from the same client carries `Session-Id: <id>` and hits the session's cached `XCUIApplication` binding directly.
- No per-request activation regardless of what `App-Activate` says.
- The runner exposes an explicit `POST /session/renew-activation` escape hatch for drift recovery, rate-limited to at most one activation per 2 s per session.

Available since v1.0.3. The legacy per-request rebind path stays as a fallback (rate-limited to at most one activation per 5 s per bundle-id since v1.0.2), so v1.0.x consumers keep working against v1.0.3 runners and vice versa.

## When to use

- **`smix run`** — the CLI opens a session for you automatically at start, closes on exit. Zero yaml changes.
- **Rust SDK** — call `App::open_session(bundle_id, activate)` at the top of your flow, use `session.app()` for all subsequent calls, `session.close().await` at the end.
- **TypeScript SDK** — `Session.open(runner, bundleId, { activate: true })`, pair with `try / finally { await session.close() }`.
- **Swift SDK** — `HttpSmixSimRuntime` + `Session.open(runtime, activate:)` (both new in v1.0.3). Pair with `defer { Task { try? await session.close() } }`.
- **Kotlin SDK** — `HttpSmixSimRuntime` + `Session.open(runtime, activate = true)` (both new in v1.0.3). Pair with `try { ... } finally { session.close() }`.

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

## TypeScript SDK

```ts
import { Smix, Session, HttpSimRuntime, bundleId } from '@goliapkg/smix'

const runtime = new HttpSimRuntime('http://127.0.0.1:22087')
const session = await Session.open(runtime, 'com.example.app', { activate: true })
try {
  const app = await Smix.launchApp(bundleId('com.example.app'), runtime, runtime.resolver)
  await app.tap(Selector.id('btn-login'))
  await app.find(Selector.text('Dashboard')).toBeVisible({ timeoutMs: 5000 })
} finally {
  await session.close()
}
```

The `HttpSimRuntime`'s internal `HttpRunnerClient` picks up the `Session-Id` header via `setSessionId` inside `Session.open` — the wiring is invisible to callers.

## Swift SDK

```swift
import SmixSDK

let runtime = HttpSmixSimRuntime(
    baseURL: URL(string: "http://127.0.0.1:22087")!,
    bundleId: "com.example.app"
)
let session = try await Session.open(runtime, activate: true)
do {
    let app = try await Smix.launchApp(
        .bundleId("com.example.app"),
        runtime,
        runtime.selectorResolver
    )
    try await app.tap(.text("Sign In"))
    try await session.close()
} catch {
    try? await session.close()
    throw error
}
```

`HttpSmixSimRuntime` implements the full `SmixSimRuntime` protocol; consumers can pass it anywhere a `SmixSimRuntime` is accepted (the mock runtime is used only for unit tests).

## Kotlin SDK

```kotlin
import dev.smix.sdk.*

val runtime = HttpSmixSimRuntime(
    baseUrl = "http://127.0.0.1:28080",
    bundleId = "com.example.app"
)
val session = Session.open(runtime, activate = true)
try {
    val app = Smix.launchApp(AppTarget.BundleId("com.example.app"), runtime)
    app.tap(Selector.Id("btn-login"))
} finally {
    session.close()
}
```

The Kotlin `HttpSmixSimRuntime` uses `java.net.HttpURLConnection` — no additional HTTP-library dependency. Thread-safe on the session-id field via `AtomicReference`.

## CLI

`smix run` opens a session automatically. No new flags — the session is just there under the hood:

```bash
smix run visual.yaml --device ios-17 --activate --bundle-id com.example.app
```

Behind the scenes:

1. CLI POSTs `/session/open` with `{bundleId, activate}`, gets a session id.
2. Every subsequent request in the flow carries `Session-Id`.
3. On exit, CLI POSTs `/session/close`.

If the runner returns a non-2xx from `/session/open` (older v1.0.x runner without session support), the CLI logs a WARN and falls through to the legacy per-request rebind path. This is safe — the legacy path has been rate-limited to 1 activate / 5 s / bundle-id since v1.0.2, so it doesn't storm.

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
