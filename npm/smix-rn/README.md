# @goliapkg/smix

TypeScript types and host-side building blocks for
[smix](https://github.com/goliajp/smix) — AI-native UI automation for the
iOS Simulator and Android emulator.

**Live driving now runs through the napi addon.** This package ships the
same typed API surface as the Swift / Kotlin / Rust SDKs; `Smix.launchApp`,
`App.tap`, `App.fill`, and the rest drive a device by calling into
`@goliapkg/smix-node`. The bundled per-platform addon — so you don't wire
one up yourself — lands with the prebuild matrix in a later release; until
then, inject a `NodeDriver`. Three surfaces still throw
`SmixNotImplementedError` pending their transport: `App.screenshot` and
`App.openUrl` need a runner wire route, and `App.launchFresh` needs
host-side state clearing. To drive a device without wiring the addon
yourself today, use the [`smix` CLI](https://crates.io/crates/smix-cli) or
one of the Swift / Kotlin / Rust SDKs.

## What works today

- **Selector DSL** — the full selector language, encoding to the exact
  JSON wire shape the Rust core resolves.
- **Session lifecycle** — `Session.open` / `stillValid` / `relaunchApp` /
  `renewActivation` / `close` against a running smix runner, over HTTP.
- **`ExpectationFailure`** — parse and produce smix's AI-readable
  failure JSON.
- **`A11yNode`** — typed accessibility-tree model with `flatten` /
  `findById` / `rectCenter` helpers for working with `smix tree --json`
  output.

## Installation

```bash
bun add -D @goliapkg/smix
# OR
npm install -D @goliapkg/smix
```

Test-target only: keep it in `devDependencies`, never bundled into a
production RN / Expo app.

## Selector

```typescript
import { Selector, literal, regex } from '@goliapkg/smix'

Selector.id('btn-login')
Selector.text(literal('Sign In'))
Selector.text(regex('^Sub', 'i'))
Selector.label('Settings')
Selector.role('button', literal('Submit'))
Selector.localizedText({ en: 'Submit', ja: '送信' })

// Fluent modifier chaining (returns a new Selector)
Selector.id('btn').below(Selector.text(literal('Address'))).nth(0)
Selector.role('button').near(Selector.text(literal('Confirm')))
```

Wire JSON (untagged + flattened — byte-identical to Rust `smix-selector`):

```json
{"id": "btn-login", "below": {"text": "Address"}, "nth": 0}
```

## Session lifecycle

```typescript
import { HttpSimRuntime, Session } from '@goliapkg/smix'

const runtime = new HttpSimRuntime('http://127.0.0.1:22087')
const session = await Session.open(runtime, 'com.example.app')
await session.relaunchApp()
await session.close()
```

`HttpSimRuntime` also exposes `resolver` / `labelsResolver`, which
resolve a selector against a caller-supplied tree JSON via the runner's
`/select/resolve` routes.

## ExpectationFailure

```typescript
import { ExpectationFailure } from '@goliapkg/smix'

try {
  // a driving call, once the native transport lands
} catch (e) {
  if (e instanceof ExpectationFailure) {
    e.code            // 'ELEMENT_NOT_FOUND' | 'AMBIGUOUS' | 'TIMEOUT' | ...
    e.visibleElements // A11yNode[] — context for AI diagnosis
    e.suggestions     // string[]
    e.toJson()        // single-line JSON for agent consumption
  }
}
```

## License

Apache-2.0 OR MIT (dual, at your option).
