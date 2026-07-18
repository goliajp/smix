# smix v0.3.0 published — 4 capability primitives live

Date: 2026-07-08
Prior chain: `insight-v0.2.1-published.md` → `insight-v0.2.5-published.md` (batch + migrate + concurrency) → **this doc**.
Superseded: prior v0.2.x adoption docs. If you're catching up from v0.2.0, apply v0.2.1 → v0.2.5 → v0.3.0 in order (each is byte-compat with the previous).

Feedback iron rule: see `.claude/dogfood/README.md` in smix repo. Every capability below was designed as a *primitive* consumers compose over, not a targeted patch. Feedback on any of them lands in `.claude/dogfood/` for the next round.

---

## 0. What actually shipped

| Ecosystem | Coordinate | Version | URL |
|---|---|---|---|
| **crates.io** | 22 crates (adds smix-metro-log / smix-fixture / smix-annotate to the 19 v0.2.5 ones) | 0.3.0 | https://crates.io/crates/smix-cli |
| **npm** | `@goliapkg/smix` | 0.3.0 | https://www.npmjs.com/package/@goliapkg/smix |
| **Maven Central** | `jp.golia.smix:smix-sdk` | 0.3.0 | https://central.sonatype.com/artifact/jp.golia.smix/smix-sdk (HTTP 200 on repo1.maven.org) |
| **Swift Package (GitHub Release)** | `SmixCoreFFI.xcframework.zip` | swift-v0.3.0 | https://github.com/goliajp/smix/releases/tag/swift-v0.3.0 |

---

## 1. What's new — 4 capability primitives

Each primitive maps to an insight-roadmap section (or the user-filed screenshot RFC). Each is documented in CHANGELOG v0.3.0 with the full design intent.

### 1.1 Metro log signal bridging (insight-roadmap §B) — the single most productive addition

smix now observes the app-under-test's metro/expo log stream and exposes signal-await as first-class yaml verbs. This decouples flow assertions from what the UI looks like at any instant — the app declares "I'm done with X" via a log line, smix waits for that line.

**Config** (`.smix/config.json`, optional):

```json
{
  "metroLog": {
    "url": "ws://127.0.0.1:8081/logs",
    "allowlist": [
      "^skipping register: must use physical device",
      "^bundle scheme is file - unable to"
    ]
  }
}
```

Or override per invocation: `smix run --metro-log-url ws://127.0.0.1:8081/logs`.

Fallback: `--metro-log-url file:///path/to/metro.log` for on-disk tail (poll-based, 100ms tick, seeks to end).

**yaml verbs**:

```yaml
# Presence — signal appears in a window
- expect:
    signal:
      regex: "env=qa-mode"
      timeoutMs: 8000
      window:
        sinceStep: 3               # optional; default = SinceRun (from runner boot)

# Ordering — signals in exact sequence
- expect:
    signals:
      - regex: "^launchOverrideConsumed"
      - regex: "^autoLoginValidated"
      - regex: "^readyForInteractive"
    order: strict                  # or "any" (default)
    timeoutMs: 30000

# Log-hygiene assertion — no non-allowlisted warn/error entries
- expectLogClean                   # shorthand
- expect:
    logClean: true                 # long form
```

**CLI flags** (append implicit steps to a flow at invocation):

```bash
smix run flow.yaml --await-signal 'env=qa-mode' --metro-log-url ws://127.0.0.1:8081/logs
smix run flow.yaml --expect-log-clean --metro-log-url ws://127.0.0.1:8081/logs
```

Powered by new stone crate `smix-metro-log` (ring buffer + subscribers + await surface). Runtime attaches via `Adapter::with_metro_tail`. Absent config → verbs error with an actionable hint (`configure .smix/config.json metroLog block or pass --metro-log-url`).

### 1.2 Fixture chip protocol (insight-roadmap §A)

Replace the 3-6 line "open qa-bubble → wait → tap chip → wait for signal" boilerplate in every scope yaml with a single line.

**Config**: point to a JSON registry via CLI flag `--fixture-registry /path/to/registry.json` (or `.smix/config.json` `fixturesRegistry`).

**Registry** (JSON):

```json
{
  "version": 1,
  "fixtures": {
    "prime-search-history": {
      "testID": "qa-chip-prime-search-history",
      "signal": {
        "regex": "\\[fixture\\] prime-search-history: seeded (\\d+) rows",
        "level": "log"
      },
      "timeoutMs": 8000
    },
    "enter-qa-mode": {
      "testID": "qa-chip-enter-qa-mode",
      "signal": { "regex": "env=qa-mode" },
      "timeoutMs": 5000
    }
  }
}
```

**yaml**:

```yaml
- fixture: prime-search-history        # short form

- fixture:                              # long form with per-invocation timeout override
    id: prime-search-history
    timeoutMs: 12000
```

**Runtime sequence** (idempotent — closes overlay even on failure):

1. Tap `id: qa-bubble-toggle` (open QA overlay)
2. Tap `id: <fixture.testID>`
3. `await_signal(fixture.signal.regex, timeoutMs)` via metro tail (from §1.1)
4. Tap `id: qa-bubble-toggle` (close overlay)

Powered by new stone crate `smix-fixture`. Registry loader validates shape at parse time. Missing registry → actionable DriverError hint. Depends on §1.1 metro tail — the runtime returns an error if the fixture verb runs without a metro log subscriber attached.

TypeScript module reader for `registry.ts` deferred to v0.3.5 (JSON is the v0.3.0 surface). Insight can generate a JSON registry from its existing `.devtools/qa/fixtures/registry.ts` with a one-shot codegen.

### 1.3 Standard subflow catalogue (insight-roadmap §C)

Two shipped yaml files usable as `runFlow: std/<name>.yaml` — smix resolves `std/` against the shipped runner directory.

```yaml
- runFlow: std/dismiss-open-in.yaml    # iOS 26 SpringBoard "Open in <app>" dismiss (idempotent)
- runFlow: std/ensure-locale.yaml      # sim locale contract check against env
```

**Resolver cascade** (highest priority first):

1. `<cwd>/std/<name>.yaml` — consumer override wins
2. `$SMIX_STD_SUBFLOWS/<name>.yaml`
3. `~/.local/share/smix/std/<name>.yaml` — install-local.sh target
4. `<smix source tree>/crates/smix-cli/std/<name>.yaml` — dev fallback

Insight can delete forked local copies of dismiss-open-in.yaml and reference `std/` directly.

Bundled with `bash scripts/install-local.sh` in the smix repo, or ship in your Docker image as needed.

### 1.4 Annotated screenshots (user-filed RFC)

Standalone CLI + library that composes circle / arrow / text / box / line primitives onto a PNG with configurable compression.

**CLI**:

```bash
smix annotate input.png output.png \
    --annotate "circle,at:200_150,color:red,radius:60" \
    --annotate "arrow,from:50_50,to:350_250,color:blue,stroke:5" \
    --annotate "box,at:100_100,width:200,height:100,color:yellow" \
    --annotate "text,at:50_50,content:test-a,color:green,size:24" \
    --font /System/Library/Fonts/Supplemental/Arial.ttf \
    --compression balanced
```

Mini-DSL: `kind,key:value,key:value` — positions encoded as `X_Y` (underscore or pipe separator) to avoid clash with the outer `,`.

**Kinds** (5): `circle` / `arrow` / `text` / `box` / `line`
**Compression presets**: `fast` (no oxipng) / `balanced` (oxipng level 2, default) / `aggressive` (oxipng level 6 + zopfli)
**Colors**: 12 named + `#RRGGBB` + `#RRGGBBAA` + `rgb(...)` + `rgba(...)` + 5 semantic (`expected` / `actual` / `hint` / `error` / `success`)

**Library** (Rust):

```rust
use smix_annotate::{Annotator, Annotation, Color, Position, Compression};

let png = std::fs::read("in.png")?;
let out = Annotator::new(&png)?
    .add(Annotation::circle(Position::pixel(100, 100))
        .color(Color::EXPECTED)
        .radius(40))
    .add(Annotation::text(Position::pixel(50, 50), "step 1")
        .color(Color::HINT)
        .size(28))
    .font(std::fs::read("/path/to/font.ttf")?)
    .compression(Compression::AGGRESSIVE)
    .render()?;
std::fs::write("out.png", out)?;
```

**Not yet in v0.3.0** (v0.3.5 slot): bundled fonts (Inter + Noto Sans SC), yaml verb `takeScreenshot: { annotate: [...] }`, `--debug-output` fail-step auto-annotate with the failing selector circled.

Powered by new stone crate `smix-annotate` (image + imageproc + ab_glyph + oxipng deps). 14 unit tests including pixel-level primitive verification.

---

## 2. Install v0.3.0

```bash
cargo install smix-cli --locked --version 0.3.0 --force
# or
cd path/to/smix && git checkout smix-v0.3.0 && bash scripts/install-local.sh
smix --version    # → smix 0.3.0
```

`bash scripts/install-local.sh` also copies the std subflow catalogue to `~/.local/share/smix/std/`. `cargo install` doesn't (no post-install hooks in cargo); if you use `cargo install`, either copy `std/*.yaml` manually or set `$SMIX_STD_SUBFLOWS`.

### npm SDK (TypeScript native tests)

```bash
cd /Users/doracawl/workspace/qualcomm/insight
bun add @goliapkg/smix@0.3.0
```

### Sanity check

```bash
smix --version | grep -q '0.3.0'        && echo "✅ smix 0.3.0"
smix run --help | grep -q 'expect.signal\|await-signal' && echo "✅ signal await surfaced"
smix run --help | grep -q 'fixture-registry' && echo "✅ fixture registry flag surfaced"
smix annotate --help | grep -q '\-\-annotate' && echo "✅ annotate CLI surfaced"
smix migrate --help >/dev/null 2>&1     && echo "✅ migrate (v0.2.5) still present"
```

All 5 `✅` → v0.3.0 install carries every capability.

---

## 3. Apply the primitives to insight

### 3.1 Path A migration — the big picture

The 12 `.devtools/qa/sim/flows/*/*.yaml` scope flows currently duplicate the fixture-tap sequence (~3-6 lines each). Path A migration:

1. **Write** `.devtools/qa/fixtures/registry.json` (codegen from `registry.ts` — see §5 below)
2. **Replace** each scope's `open qa-bubble → wait → tap chip → wait signal` block with `- fixture: <id>` (1 line each)
3. **Retire** `logGate` in `.devtools/qa/sim/runner.ts` — the metro tail + `expectLogClean` verb replaces it
4. **Replace** temporal-contract log assertions with `expect.signals` (in strict order for event ledgers)

Estimated yaml reduction: ~250 lines across the qa corpus.

### 3.2 CI hook — smix run + metro-log-url

Add the `--metro-log-url` flag to your existing `bun verify`-style test runners:

```typescript
// .devtools/verify/smix-env.ts
export function makeSmixArgs(scope: string, opts: Options): string[] {
  return [
    'run',
    `.devtools/qa/sim/flows/${scope}/main.yaml`,
    '--device', opts.simUdid,
    '--metro-log-url', 'ws://127.0.0.1:8081/logs',
    '--fixture-registry', '.devtools/qa/fixtures/registry.json',
    '--no-launch',
    '--debug-output', opts.debugDir,
    '--format', 'json',
  ]
}
```

CI env vars still flow via `--env` from the v0.2.0 gol-611 surface — no change there.

### 3.3 Composable delivery gate assertion

The temporal contract event ledger becomes a single yaml step:

```yaml
- expect:
    signals:
      - regex: "^launchOverrideConsumed\\('force-update'\\)"
      - regex: "^autoLoginValidated"
      - regex: "^readyForInteractive"
    order: strict
    timeoutMs: 45000
```

This is byte-for-byte equivalent to hand-writing a state-machine gate in TS. Any ordering violation raises an ExpectationFailure with the exact `ms_since_start` at which the pattern first appeared — perfect for post-fail diagnosis.

---

## 4. New CLI flag summary (all in `smix run`)

| Flag | v | Purpose |
|---|---|---|
| `--metro-log-url <URL>` | 0.3.0 | Start a metro log subscriber. `ws://` or `file://` |
| `--await-signal <regex>` | 0.3.0 | Append implicit `expect.signal { regex }` to flow |
| `--expect-log-clean` | 0.3.0 | Append implicit `expectLogClean` step |
| `--fixture-registry <path>` | 0.3.0 | Enable `- fixture:` yaml verb |
| `--activate` | 0.2.1 | Send `App-Activate: true` header on every request (unchanged) |
| `--bundle-id <id>` | 0.2.0 | Target bundle for XCUIApplication rebind (unchanged) |
| `--env KEY=VAL` | 0.2.0 | Env var for yaml `${NAME}` interpolation (unchanged) |
| `--debug-output <dir>` | 0.2.0 | Per-step JSON + fail screenshot artifacts (unchanged) |
| `--format json` | 0.2.0 | Structured stdout report (unchanged) |
| `--fail-fast` | 0.2.5 | Abort batch on first failure (unchanged, batch semantics) |

---

## 5. Fixture registry codegen (recommended one-shot)

Convert `.devtools/qa/fixtures/registry.ts` to JSON. Given a shape like:

```typescript
export const FIXTURES: Record<FixtureId, FixtureDecl> = {
  'prime-search-history': {
    testID: 'qa-chip-prime-search-history',
    signal: { level: 'log', regex: /\[fixture\] prime-search-history: seeded (\d+) rows/ },
    timeoutMs: 8000,
  },
  // ...
}
```

The one-shot Node script:

```javascript
// .devtools/qa/fixtures/gen-json.mjs
import { FIXTURES } from './registry.ts'
import { writeFileSync } from 'fs'

const json = {
  version: 1,
  fixtures: Object.fromEntries(
    Object.entries(FIXTURES).map(([id, d]) => [id, {
      testID: d.testID,
      signal: { regex: d.signal.regex.source, level: d.signal.level },
      timeoutMs: d.timeoutMs,
    }])
  ),
}
writeFileSync('.devtools/qa/fixtures/registry.json', JSON.stringify(json, null, 2))
```

Run once whenever `registry.ts` changes. When v0.3.5 lands a TS reader, this codegen retires.

---

## 6. Rollback

Insight-side:
```bash
# revert the yaml swap
cd /Users/doracawl/workspace/qualcomm/insight
git checkout -- .devtools/qa/sim/flows/*/main.yaml
rm .devtools/qa/fixtures/registry.json
# CI env stays same — just doesn't pass --fixture-registry / --metro-log-url
```

smix binary rollback:
```bash
cargo install smix-cli --locked --version 0.2.5 --force
```

Both sides rollback independently — smix binary can stay at 0.3.0 while insight stays on maestro-style verbs (v0.3.0 is byte-compat with 0.2.5 yaml).

---

## 7. Reporting back

Feedback protocol per `.claude/dogfood/README.md` iron rule:

- **Where**: `.claude/state/gol-611/smix-feedback-v0.3.0-<topic>.md` on your side (any structured path works — smix side reads whatever you point at). Note the specific capability primitive the feedback applies to (§B / §A / §C / annotations).
- **How** to write it — reporter identifies the surface shape; smix side re-abstracts to "which capability layer is missing what" before designing a fix. Point defects at the primitive, not at a single yaml line.
- **What happens on smix side**: rolls into next dogfood round → `.claude/dogfood/<date>-insight-<topic>.md` detailed record → response doc in `docs/ai-guide/` → v0.3.x shipped fix.

Include when reporting:

1. `smix --version`
2. Which primitive (§B signal / §A fixture / §C std subflow / §D annotate)
3. Exact `smix run` command + stderr/stdout
4. yaml step that triggered the issue
5. `.smix/config.json` `metroLog` / `fixturesRegistry` blocks (redact if needed)
6. Metro state (`curl -sf http://127.0.0.1:8081/status`)

---

## 8. Next milestone: v0.3.5

Deferred non-goals from v0.3.0 (see CHANGELOG for full list):

- **§B**: yaml `takeScreenshot: { annotate: [...] }` verb + `--debug-output` fail-step auto-annotate with the failing selector circled
- **§A**: TypeScript module reader for fixture registry (`.devtools/qa/fixtures/registry.ts` load without codegen)
- **§D**: Bundled fonts (Inter + Noto Sans SC subset for CJK) — currently `--font <path>` required for text annotations
- **§B**: Metro log allowlist multi-source merging (currently one `.smix/config.json`)

Target date: +2-3 weeks. Ping smix side if any of these gate insight's adoption; priority reshuffle is fine.

Beyond v0.3.5, per insight-roadmap:

- **v0.4.0** — §E part 2 (session recording) + §F (semantic a11y-tree diff for visual gate) + §G (selector auto-suggest at authoring time)
- **v0.5.0** — §H (native control channel) + §J (flow coverage report) + §K (deterministic time / animations mode)

---

## 9. Quick reference (updated 2026-07-08)

| resource | location |
|---|---|
| smix repo | `github.com/goliajp/smix` @ `smix-v0.3.0` |
| Binary (this machine) | `~/.local/bin/smix` (`smix --version` → `smix 0.3.0`) |
| Shipped runner | `~/.local/share/smix/runner/SmixRunner.xcodeproj` |
| Shipped std subflows | `~/.local/share/smix/std/dismiss-open-in.yaml` + `ensure-locale.yaml` |
| crates.io CLI | `cargo install smix-cli --version 0.3.0` |
| npm SDK | `bun add @goliapkg/smix@0.3.0` |
| Maven Central | `jp.golia.smix:smix-sdk:0.3.0` |
| Swift Package tag | `swift-v0.3.0` |
| Integration guide (base) | `smix/docs/ai-guide/insight-integration-guide.md` |
| CHANGELOG | `smix/CHANGELOG.md` (v0.3.0 section) |
| Verify probes | `smix/scripts/gol-611-verify.sh` (v0.2.1 §6 rebind still runs) |
| Roadmap | `smix/docs/ai-guide/insight-roadmap.md` |
| Dogfood log | `smix/.claude/dogfood/README.md` (iron rule + round index) |
| Annotation RFC | `smix/docs/ai-guide/rfc-v0.3.0-annotated-screenshots.md` (design + open questions) |
| Metro log crate | `smix/crates/smix-metro-log/src/lib.rs` |
| Fixture crate | `smix/crates/smix-fixture/src/lib.rs` |
| Annotate crate | `smix/crates/smix-annotate/src/lib.rs` |

If any URL / coordinate is stale (Maven Central slow-propagating, crates.io index lag), that resolves within ~30 min of the publish timestamp above.
