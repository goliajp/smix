# @goliapkg/smix

Playwright-style iOS Simulator + Android emulator UI automation SDK for
TypeScript / React Native / Expo / Detox-compatible test runners. Brings
smix's Rust-core selector resolver to TS via HTTP client (production) or
in-process mock (unit tests), packaged as an ESM Node 20+ module.

## Installation

```bash
bun add -D @goliapkg/smix
# OR
npm install -D @goliapkg/smix
```

For test target / Detox-equivalent consumer. This package should only be
listed in `devDependencies` — never bundled into a production RN/Expo app
via metro/expo tree-shaking.

## Quick start (MockSimRuntime — unit tests)

```typescript
import { Smix, Selector, literal, MockSimRuntime, MockSelectorResolver, bundleId } from '@goliapkg/smix'

const runtime = new MockSimRuntime({ snapshotResult: /* A11yNode tree */ })
const resolver = new MockSelectorResolver()
resolver.registerHit('{"id":"btn-login"}', 'btn-login')

const app = await Smix.launchApp(bundleId('com.example.MyApp'), runtime, resolver.resolve)
await app.tap(Selector.id('btn-login'))
await app.fill(Selector.id('input-username'), 'alice')

const welcome = app.find(Selector.text(literal('Welcome back')))
await welcome.toBeVisible({ timeoutMs: 5_000 })
```

## Production: HTTP runtime

```typescript
import { Smix, Selector, bundleId, HttpSimRuntime } from '@goliapkg/smix'

// Connects to smix-runner-server running on host (see swift-bridge/SmixRunnerCore)
const runtime = new HttpSimRuntime('http://localhost:14101')

const app = await Smix.launchApp(bundleId('com.example.MyApp'), runtime, runtime.resolver, runtime.labelsResolver)
await app.tap(Selector.id('btn-login').below(Selector.text(literal('Sign In'))).nth(0))
await app.find(Selector.role('button')).toHaveCount(3, { timeoutMs: 5_000 })
```

HTTP endpoints documented in [`src/HttpRunner.ts`](src/HttpRunner.ts) +
[Swift server route handler](../../swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift).

## API surface

### Selector

```typescript
// Base discriminators (untagged JSON wire shape — matches Rust smix-selector)
Selector.id('btn-login')
Selector.text(literal('Sign In'))
Selector.text(regex('^Sub', 'i'))
Selector.label('Settings')
Selector.role('button')
Selector.role('button', literal('Submit'))
Selector.focused()
Selector.anchor({ below: Selector.text(literal('Total')) }, { nth: 0 })
Selector.localizedText({ en: 'Submit', ja: '送信' })

// Fluent modifier chaining (returns new Selector with Modifiers updated)
Selector.id('btn').below(Selector.text(literal('Address')))
Selector.label('Edit').nth(0)
Selector.text(literal('Item')).above(Selector.text(literal('Footer'))).first()
Selector.role('button').near(Selector.text(literal('Confirm'))).ancestor(Selector.role('dialog'))
```

Wire JSON (untagged + flatten — byte-identical to Rust smix-selector):

```json
{"id": "btn-login", "below": {"text": "Address"}, "nth": 0}
```

### App (act)

```typescript
await app.tap(selector: Selector, opts?: { timeoutMs?: number })
await app.fill(selector: Selector, text: string)
await app.pressKey(key: KeyName)  // 'return' | 'delete' | 'space' | 'tab' | 'escape' | 'enter'
await app.swipe(direction: SwipeDirection)  // 'up' | 'down' | 'left' | 'right'
await app.tapAtCoord(nx: number, ny: number)  // 0..1, throws if out-of-range
await app.terminate()
await app.relaunch()
await app.launchFresh({ clearState?: boolean, clearKeychain?: boolean, appPath?: string })
```

### App (sense)

```typescript
const png: Uint8Array = await app.screenshot()
const tree: A11yNode = await app.tree()
const popups: A11yNode[] = await app.systemPopups()
await app.openUrl('myapp://deep/link')
```

### Locator (assertions)

```typescript
const loc = app.find(Selector.text(literal('Welcome')))
await loc.toBeVisible({ timeoutMs: 5_000 })    // polls 250ms
await loc.toContainText('alice', { timeoutMs: 5_000 })  // substring
await loc.toHaveLabel('Sign In', { timeoutMs: 5_000 })  // strict equal
await loc.toHaveCount(3, { timeoutMs: 5_000 })  // exact match count
```

### ExpectationFailure (AI-readable JSON)

```typescript
try {
  await app.tap(Selector.id('btn-missing'))
} catch (e) {
  if (e instanceof ExpectationFailure) {
    e.code              // 'notFound' | 'ambiguous' | 'notInteractable' | 'timeout' | 'wrongState' | 'unknown'
    e.message           // human-readable
    e.selectorJson      // original Selector encoded as JSON
    e.visibleElements   // A11yNode[] — first 20 nodes from current tree
    e.suggestions       // string[] — ['check accessibilityIdentifier...', ...]
    e.toJson()          // single-line sorted-keys JSON for AI agent consumption
  }
}
```

## Demos

Run any of the realistic e2e flows in [`examples/demo-app/`](examples/demo-app):

```bash
cd examples/demo-app
bun login-flow.ts             # login + welcome assertion
bun form-validation-flow.ts   # 3-cycle form validation + inline errors
bun list-scroll-flow.ts       # scrollable list + toHaveCount semantics
bun multi-screen-nav-flow.ts  # nav + tapAtCoord + openUrl + relaunch
```

See [`examples/demo-app/COVERAGE.md`](examples/demo-app/COVERAGE.md) for the
SDK surface coverage matrix across demos.

## Conformance

Cross-binary harness verifies SDK output byte-identical to Rust:

```bash
bash ../../scripts/sdk/run-cross-binary-harness.sh
# Summary: 24 / 24 fixtures byte-identical (Rust + Swift + TS)
```

## Test-target-only

`@goliapkg/smix` is intended for `devDependencies` only — never bundled
into a production RN/Expo app. `metro`/`expo`'s tree-shaking guarantees
the production bundle excludes it. The `HttpSimRuntime` connects to a
smix-runner-server running on the host (via `127.0.0.1:port` loopback),
not in-process.

## Architecture

- **Rust stone core** (`crates/smix-{selector,selector-resolver,screen,error}`):
  selector resolution + AI-readable failure types, shared across all
  language SDKs.
- **HTTP runtime** (`src/HttpRunner.ts`): wraps fetch to smix-runner-server
  on the host. Production-default.
- **Mock runtime** (`src/SimRuntime.ts MockSimRuntime`): in-memory tree +
  event log. Used by vitest unit tests + demo flows.
- **SDK facade** (`src/{Smix,App,Selector,Modifiers,Locator,ExpectationFailure}.ts`):
  Playwright-style ergonomic surface, class-based fluent chaining.

## License

Apache-2.0 OR MIT (dual, at your option).
