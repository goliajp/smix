# `--activate` header lifetime

> How `--activate` and `--bundle-id` behave per request, so consumers can reason about which XCUITest bundle is active for each call.

## TL;DR

- `--activate` and `--bundle-id` are **per-request headers**, not
  session-sticky state on the runner.
- Every `smix run` invocation resends the pair on every request to
  the runner via `App-Bundle-Id: <id>` + `App-Activate: true`
  headers.
- The runner-side handler resolves the target `XCUIApplication(bundleId)`
  per request via a `resolveApp()` closure; when `App-Activate: true`,
  it hops through `SmixRunnerServer.onMain { .activate() }` before
  invoking the operation.
- Absent headers → the runner falls back to the boot-time
  `XCUIApplication` bound in `test_runForever` setup.

## Lifetime rules

### At runner boot

`smix runner up <device>` launches `test_runForever` (XCUITest
test method). The setup phase constructs one `XCUIApplication`
bound to the `SMIX_RUNNER_TARGET_BUNDLE` env var.

### Per request

Each HTTP request to the runner may carry `App-Bundle-Id` +
`App-Activate` headers. The runner's `contextGuardedResponse`
wrapper parses them into a `@TaskLocal RequestContext` scoped to
that request's handler task.

The `resolveApp` closure reads the task-local:

```swift
let resolveApp: @Sendable () async -> XCUIApplication = {
  let ctx = SmixRunnerServer.currentContext
  if let b = ctx.bundleId, b != bundleId {
    let target = XCUIApplication(bundleIdentifier: b)
    if ctx.activate {
      await SmixRunnerServer.onMain { target.activate() }
    }
    return target
  }
  if ctx.activate {
    await SmixRunnerServer.onMain { app.activate() }
  }
  return app
}
```

Handlers call `let app = await resolveApp()` at the top of their
body. The target bundle can change per request without restarting
the runner.

### After the request completes

The task-local goes out of scope. Subsequent requests without
headers use the boot-time default.

## Common patterns

### Rebind mid-flow (e.g. system dialog dismiss)

To interact with SpringBoard between app steps:

```bash
smix run flow.yaml --bundle-id com.example.app --activate
# ... flow completes ...
smix run cleanup.yaml --bundle-id com.apple.springboard --activate
```

Each `smix run` invocation sends its own header pair; no runner
restart is needed.

### Concurrent flows against different bundles

Not supported in v1.0 — one runner serves one target at a time
because XCUITest's synthesize-event dispatch is process-global.
Use two separate `smix runner up <deviceA>` + `<deviceB>` instances
with different `runnerPort` values in the `.smix/` registry.

## Anti-patterns

### Assuming "sticky" state

**Wrong**:
```bash
smix run first.yaml --bundle-id com.mybundle
# runner "remembers" the bundle... no it doesn't
smix run second.yaml
# ← second.yaml uses BOOT-TIME bundle, not com.mybundle
```

**Right**:
```bash
smix run first.yaml --bundle-id com.mybundle
smix run second.yaml --bundle-id com.mybundle  # pass every time
```

### Assuming `--activate` costs zero

Each `--activate` request costs 50-100 ms on the runner (main-actor
hop + `XCUIApplication.activate` roundtrip). For flows with many
short steps, the overhead adds up. Prefer omitting `--activate`
when your app is guaranteed foreground already (e.g. after a fresh
`sim launch`).

## References

- Wire header schema: [wire-format.md](wire-format.md)
