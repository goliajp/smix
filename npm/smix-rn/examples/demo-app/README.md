# @goliapkg/smix demo-app

> Minimal node-based demo for `@goliapkg/smix` — shows the full SDK surface
> exercised end-to-end against an in-memory simulator backend. Mirrors
> what a real RN/Expo XCUITest target would look like.

## Why this exists

Prove `@goliapkg/smix` is usable end-to-end without booting a real
simulator. The demo uses `MockSimRuntime` + `MockSelectorResolver` to
stage a deterministic UI tree, then writes a realistic e2e test
(login → fill → tap → assert welcome) that flows through the same
Locator / App / Selector / ExpectationFailure paths that production tests
would use.

In a real iOS XCUITest + Android instrumentation deployment, the demo's
`runtime` and `resolver` would be swapped for `HttpSimRuntime` +
`HttpSimRuntime.resolver` (already implemented in `@goliapkg/smix`). The
rest of the test code is byte-identical.

## Running

```bash
cd npm/smix-rn/examples/demo-app
# Demo assumes parent package is built; run from npm/smix-rn first if needed.
bun login-flow.ts
# OR
npx tsx login-flow.ts
```

Expected output:

```
login flow PASS (4 steps, mock sim)
   1. tap "Sign In" -> 1 tap dispatched
   2. fill username -> "alice"
   3. fill password -> "secret"
   4. assert welcome.visible -> PASS
```

## What this is NOT

- This is NOT a production RN app. It's a node script that exercises
  `@goliapkg/smix` in isolation.
- This is NOT instrumentation testing. For real-sim verification you
  would swap in `HttpSimRuntime` and run against a booted simulator or
  emulator.

## Metrics

This demo is hand-written. Rough numbers:

- **LoC**: ~80 (login-flow.ts) — vs equivalent Detox flow ~150 LoC
- **Setup time**: 0 (no metro/expo/sim boot needed for shape verification)
- **Failure diagnosis**: `ExpectationFailure.toJson()` emits AI-readable
  JSON with `code` / `selector` / `visibleElements` — single line, no
  stack-trace parsing needed.
