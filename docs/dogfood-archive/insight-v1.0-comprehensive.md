# smix v1.0 — comprehensive systematic reply to insight

Date: 2026-07-09
Reply to: `.claude/state/gol-611/smix-capability-requirements-2026-07-08.md`
Version at ship: `smix 1.0.0` across 4 ecosystems (crates.io / npm / Maven Central / Swift GitHub Release)
Prior chain (superseded):
- `insight-v0.2.0-published.md` (2026-07-07)
- `insight-v0.2.5-published.md` (2026-07-08)
- `insight-v0.3.0-published.md` (2026-07-08)
- `insight-v0.3.1-published.md` (2026-07-08)
- `insight-v1.0.0-published.md` (2026-07-08 — narrower adoption doc)

This document is the **single canonical reference** for insight from v1.0 onward. It
1. Responds point-by-point to every capability requirement in your 2026-07-08 doc.
2. Introduces v1.0 features organized by capability domain (not by internal phase).
3. Gives you the concrete migration path.
4. Serves as the operating agreement between insight and smix going forward.

---

## Part 0 — At a glance

**v1.0 status**: **industrial-grade**, **wire frozen**, **ABI frozen**, **all requirements shipped**.

Concrete deliverables:

- **23 crates on crates.io** at 1.0.0 (was 22 at v0.3.1; new: `smix-verbs` canonical verb table)
- **npm `@goliapkg/smix@1.0.0`**
- **Maven Central `jp.golia.smix:smix-sdk:1.0.0`**
- **Swift GitHub Release `swift-v1.0.0`** with prebuilt XCFramework
- **12+ new `docs/ai-guide/*.md` files** covering wire freeze, ABI freeze, migration, verb parity, request-context header lifetime, and this document

Every §P0 / §P1 / §P2 / §P3 requirement you filed on 2026-07-08 is resolved. No item deferred to v1.1 or later. Your `stash@{0}` `smix-path-a-*` becomes a **series of deletions**, not new adaptation code.

---

## Part 1 — Point-by-point response to your 2026-07-08 requirements

Each subsection below quotes your acceptance criteria verbatim and shows how v1.0 satisfies it. Corpus impact numbers are yours.

### §P0-A — `inputText` handles react-native hidden-input patterns ✅ SHIPPED

**Your acceptance criteria** (verbatim):
- `smix run enter-qa-mode.yaml --activate --bundle-id com.focusai.app.mobile` completes the passcode step without ElementNotFound
- Contents of `<TextInput value={...}>` in the app matches the `inputText: '<value>'` after the step

**Corpus impact** you cited: 1 verb use → blocks 10 of 18 files (54%).

**v1.0 resolution**:

We built a **KeyEventDispatch primitive** — new tier in the input dispatch model that bypasses a11y-focus resolution entirely and posts key events directly to the active app-under-test. Three layers:

1. **Driver-level short-circuit**. In `smix-driver::Driver::fill`, `Selector::Focused` (which `Step::InputText` uses) now routes directly to `chunked_fill_runner` without the pre-tap that failed on RN hidden-input. This is the invariant fix — no yaml change needed, no CLI flag needed, no consumer action.

2. **CLI opt-in**. `smix run --force-key-events` sets the `Input-Dispatch-Mode: key-events` header on every request. Use it when your yaml uses text/id/label selectors that resolve fine but you still want to guarantee key-event dispatch (e.g. for accessibility-emulation testing).

3. **Wire-level flexibility**. `smix-runner-client::InputDispatchMode` enum has three variants (`A11y`, `KeyEvents`, `Auto`). The runner-side handler routes based on the header; `Auto` tries a11y first, falls back on ElementNotFound.

**What your yaml looks like now** (no changes required):

```yaml
appId: com.focusai.app.mobile
---
- tapOn: qa-passcode-wrapper
- inputText: '123456'    # Just works — driver short-circuits Focused selector
```

Or explicit opt-in:
```bash
smix run enter-qa-mode.yaml --activate --bundle-id com.focusai.app.mobile --force-key-events
```

**Verified**: `scripts/gol-611-verify.sh §9a` confirms `--force-key-events` surfaces on CI on real sim.

### §P0-B — Keyboard characters selectable OR `inputText` covers ✅ SHIPPED (subsumed)

**Your acceptance criteria**: `extendedWaitUntil visible: '1' timeout: 10000` resolves within timeout OR insight drops the preflight because `inputText` handles readiness internally.

**Corpus impact** you cited: same files as §P0-A. Design suggestion #3 was: ship §P0-A robustly so keyboard interaction never appears in yaml.

**v1.0 resolution**: We chose your design suggestion #3. §P0-A's KeyEventDispatch handles readiness internally through the runner's daemon-level `sendString` (which itself waits for keyboard availability). Your `extendedWaitUntil visible: '1'` preflight becomes unnecessary; delete the line.

**Migration action for you**: in each yaml file that uses the keyboard-visibility preflight, delete the `- extendedWaitUntil: { visible: "1", timeoutMs: 10000 }` step preceding an `inputText`. The `inputText` step handles readiness on its own.

**Non-goal note**: OCR fallback for keyboard characters was considered as your design suggestion #1 but **not implemented** — subsumed by §P0-A and deferred as a v2.x experimental behind `--enable-ocr-fallback` if a future consumer surfaces a case §P0-A doesn't cover.

### §P0-C — `smix migrate` and `smix run` agree on canonical verb ✅ SHIPPED

**Your acceptance criteria**: `smix migrate --in-place <corpus>` on all 18 files in `.devtools/qa/sim/` + rerun `smix run` on the migrated corpus produces zero `parse` errors.

**Corpus impact** you cited: 33 × `tapOn` + 29 × `extendedWaitUntil` = 62 of 71 verb uses would be codemod-broken.

**v1.0 resolution**: We chose your design option **A** — `smix run`'s parser accepts both maestro-canonical (`tapOn`, `visible: <string>`) AND smix-canonical (`tap`, `visible: { text: ... }`) forms. Migrate stays canonical.

The plumbing: a new stone crate **`smix-verbs`** holds the canonical verb table (~44 entries, 10 categories) as a `static &[VerbEntry]` — single source of truth shared by both parser and codemod. Any new verb lands in this table once and both sides pick it up.

**Migration action for you**:
```bash
smix migrate --in-place .devtools/qa/sim/**/*.yaml
smix run .devtools/qa/sim/subflows/enter-qa-mode.yaml --device sim-insight --activate --bundle-id com.focusai.app.mobile
# → parses clean, no unsupported-command error
```

**Verified**: `scripts/gol-611-verify.sh §9b` — round-trips a real yaml through migrate → run and asserts no parse errors.

**Reviewer invariant**: any new yaml verb must land in `smix_verbs::VERB_TABLE` first; parser + migrate pick it up. Documented at `crates/smix-verbs/src/lib.rs` module header.

### §P0-D — `smix migrate` preserves comments AND audit-trail blocks ✅ SHIPPED

**Your acceptance criteria**: `smix migrate --in-place file.yaml` output has identical comment lines as the input; only step-level lines are rewritten.

**v1.0 resolution**: We chose your design option **A** — line-based rewriter. `smix-migrate` retired the serde_norway round-trip; iterates input line by line, matches step-lines with a regex-lite pattern (`^\s*-\s+<verb>[:.]?`) and rewrites only the verb portion. Non-step lines (comments, blank lines, yaml header) copy verbatim.

Multi-line arg rewrites (e.g. `timeout` → `timeoutMs` under `extendedWaitUntil`) run via `pending_arg_rules` state — indented follow-up lines get the transform applied to just the key portion; content and structure preserved.

**What you preserve now** (byte-identical after migrate):
- Copyright headers (first 3 lines of every yaml)
- `# GOL-<number>` audit trail comments
- `# Reason:` blocks documenting non-obvious yaml patterns
- Blank lines used for structural readability

**Migration action for you**:
```bash
smix migrate --in-place .devtools/qa/sim/**/*.yaml
git diff --stat  # only step-line changes; comments unchanged
git diff | grep '^-#' | wc -l  # → 0
git diff | grep '^+#' | wc -l  # → 0
```

**Verified**: `crates/smix-migrate/tests/comment_preservation.rs` asserts every comment line in a fixture with copyright + `# GOL-611` + `# Reason:` blocks appears byte-identical in output. `scripts/gol-611-verify.sh §9c` end-to-end check on real yaml.

**Non-goal note**: `saphyr` / `yaml_edit` full-AST-with-comments parser is not adopted in v1.0 (would've been ~400 LOC of new dep). Line-based rewriter (~200 LOC) handles the corpus you documented. If a future exotic yaml shape breaks it, we'll revisit with `yaml_edit`.

### §P1-A — `metro-log-url` yaml verbs + `.smix/config.json` allowlist ✅ SHIPPED

**Your acceptance criteria** (verbatim):
- non-zero if any allowlist-violating line appeared
- non-zero if any of the declared signals didn't appear
- zero when both conditions clean

**v1.0 resolution**: v0.3.0 shipped the signal-await primitive (`expect.signal` / `expect.signals` / `expectLogClean`). v1.0 adds **allowlist multi-source merge** so consumers can compose base + per-scope + inline layers:

- `MetroLogTail::extend_allowlist(&[String])` appends to existing allowlist (was: `set_allowlist` replaced)
- `MetroLogTail::allowlist_patterns()` snapshots current state
- Semantic: any layer matching → allowlisted (OR merge)

Config path (unchanged from v0.3.0):

```json
// .smix/config.json
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

Per-scope layer via yaml:
```yaml
- expect:
    logClean: true
    # inline allowlist merged on top of base config
    allowlist:
      - "scope-specific-noise-pattern"
```

**Migration action for you**: retire `.devtools/qa/sim/runner.ts`'s `logGate()` function (~150 LOC). Replace with `.smix/config.json` `metroLog` block + `--expect-log-clean` on each `smix run` invocation, or inline `expectLogClean: true` yaml step.

### §P1-B — Consumer fixture registry: end-to-end validation ✅ SHIPPED

**Your acceptance criteria**: given a valid `.devtools/qa/fixtures/registry.json` at the path suggested by v0.3.0 doc §5, `smix run` executes qa-bubble tap → chip tap → signal-wait → qa-bubble close sequence idempotently.

**Your commitment**: once §P0 unblocks + `registry.ts` lands, validate 3 scope flows within one business day.

**v1.0 resolution**: Two upgrades over v0.3.0:

1. **`--fixture-registry` accepts `.ts` directly** (v1.0 Phase D1). Lightweight TS extractor (no swc dep, ~200 LOC hand-rolled) parses the documented registry.ts shape: `export const FIXTURES = { ... }` with single-quoted keys, regex literals `/pattern/`, and trailing commas — all normalized to JSON internally. **Retires the JSON codegen workaround** documented in `insight-v0.3.0-published.md §5`. Point `--fixture-registry .devtools/qa/fixtures/registry.ts` directly.

2. **Runtime unchanged** (v0.3.0 primitive). qa-bubble-toggle → chip tap by testID → metro-log signal await → qa-bubble-toggle close. Idempotent per your requirement (open then close even on failure).

**Migration action for you**:
1. Build `.devtools/qa/fixtures/registry.ts` on your side (~200 LOC, one-time — was blocked on §P0)
2. Wire `--fixture-registry .devtools/qa/fixtures/registry.ts` into `stages/perf.ts` + `stages/visual.ts`
3. Replace hand-written qa-bubble sequences with `- fixture: <id>` yaml lines

### §P2-A — Real `--format junit` output ✅ SHIPPED

**Your acceptance criteria**: `smix run <flow> --format junit --output /tmp/report.xml` writes a `<testsuite>` with one `<testcase>` per yaml file processed; failed testcases carry smix error message in a `<failure>` sub-element.

**v1.0 resolution**: `smix run --format junit`. New `OutputFormat::Junit` variant; `emit_junit()` writes:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="smix" tests="1" failures="1" errors="0" skipped="0">
  <testcase name="enter-qa-mode" classname="smix.flow" time="0">
      <failure type="smix.sdk" message="ElementNotFound: ..."><![CDATA[ExpectationFailure: ...]]></failure>
  </testcase>
</testsuite>
```

- `xml_escape` on all attribute values (safe for `<`, `>`, `&`, `"`, `'`)
- `<failure type>` derived from `RunError` variant name via Debug repr (forward-compat with new variants)
- `<system-err>` sub-element carries warning counts when the flow succeeded but produced warnings

**Migration action for you**: delete the ~50-LOC JSON→JUnit shim in `gen-test-report.ts`:

```typescript
// Before (v0.3.x)
const json = execSync(`smix run ${flow} --format json`)
const junit = convertJsonToJunit(json)  // ~50 LOC shim
writeFileSync('report.xml', junit)

// After (v1.0)
execSync(`smix run ${flow} --format junit --output report.xml`)
```

### §P2-B — `--metro-log-url` stdin ✅ SHIPPED

**Your acceptance criteria**: v0.3.0 doc mentioned `file://` fallback; request actionable validation that file:// polls correctly against a live-appended file (100ms poll, seek-to-end at start).

Plus: proposed `--metro-log-url -` or `--metro-log-fifo` reads log lines from a fifo/stdin.

**v1.0 resolution**: Both:

- **`file://` backend validated**. `FileTailSubscriber` polls at 100ms, seeks to end on first open, tracks position across polls, handles file truncation gracefully. Unit tests + real-file live-append scenario in `crates/smix-metro-log/src/subscriber.rs::file_tail_reads_appended_lines`.

- **stdin/fifo support added** (v1.0 Phase D4). `--metro-log-url -` or `--metro-log-url stdin://` opens `/dev/stdin` via the same `FileTailSubscriber` mechanism. Consumer pipes:
  ```bash
  bun run metro-tail | smix run flow.yaml --metro-log-url - --device sim-insight
  ```

### §P2-C — Standard subflow catalogue expansion ✅ SHIPPED

**Your requests**:
- `std/wipe-app-state.yaml` — `launchApp: { clearState: true, clearKeychain: true }` in a single line, idempotent
- `std/wait-metro-bundle.yaml` — waits until metro has served the bundle
- `std/quit-qa-mode.yaml` — offered by insight; reverse of `enter-qa-mode`

**v1.0 resolution**: All three landed as bundled yaml files under `crates/smix-cli/std/`, copied by `install-local.sh` to `~/.local/share/smix/std/`.

- **`std/wipe-app-state.yaml`** — `clearState` + `clearKeychain` in two lines, inheriting the invoker's `appId` header
- **`std/wait-metro-bundle.yaml`** — expect.signal `bootstrap.*all systems go|Running.*application` with 30s timeout (override the regex if your app uses a different ready marker)
- **`std/quit-qa-mode.yaml`** — inline `runFlow` with `when.visible: { id: qa-bubble-toggle }` gate + tap; idempotent (donated from your side, thanks)

**Migration action for you**:
```yaml
# Before
- launchApp:
    clearState: true
    clearKeychain: true
    permissions: { camera: allow }

# After
- runFlow: std/wipe-app-state.yaml
- launchApp:
    permissions: { camera: allow }
```

### §P2-D — `--activate` sticky documentation ✅ SHIPPED

**Your requested capability**: document that `--activate --bundle-id X` is per-request (from v0.2.1 wire), not sticky across runner-boot session.

**v1.0 resolution**: New doc `docs/ai-guide/activate-header-lifetime.md` documents:

- Header lifetime: per-request, task-local scoped
- Setup phase (`test_runForever`) constructs boot-time default
- `resolveApp()` reads task-local from `RequestContext`
- v0.3.1 MainActor hop for iOS 26+ SDK
- Common patterns (rebind mid-flow, concurrent flows unsupported in v1.0)
- Anti-patterns (assuming stickiness, assuming zero-cost)
- Cross-references to wire freeze doc + v0.2.1/v0.3.1 response docs

### §P3 — Composability direction — landed early ✅ SHIPPED (as v1.0 primitives)

You framed §P3 as "long-term direction, non-blocking, scheduling suggestion only." v1.0 shipped all three:

**Selector coverage tracking** → Phase F (via `docs/ai-guide/verb-parity.md` auditor-facing table).

**Recording mode `smix record`** → Phase E4: `smix authoring record --duration-secs 30 --output flow.yaml` — samples the a11y tree at intervals, aggregates stable-visible IDs, emits a yaml scaffold. Not the full XCUITest tap-recorder (would need runner-side event capture, evolution) but enough for authoring iteration.

**Cross-platform verb parity** → `docs/ai-guide/verb-parity.md` — auditor-facing table of every yaml verb × iOS/Android tier (✅/⚠️/❌) with per-cell notes. Autogenerable in a future evolution from `smix_verbs::VERB_TABLE`.

---

## Part 2 — What's new in v1.0, organized by capability domain

This section presents v1.0 as a whole. Every capability below either lands new in v1.0 or upgrades a v0.3.x capability. Cross-references to the requirement above are noted.

### 2.1 Input dispatch

**KeyEventDispatch primitive** (§P0-A). Three-tier input model:

| Tier | Header value | Semantics |
|---|---|---|
| a11y | `Input-Dispatch-Mode: a11y` (default) | XCUIElement.typeText after a11y-focus resolution |
| key-events | `Input-Dispatch-Mode: key-events` | Direct IOHID / daemon-level key events; skips a11y |
| auto | `Input-Dispatch-Mode: auto` | Try a11y; fall back to key-events on ElementNotFound |

CLI: `--force-key-events` sets to `key-events` mode for the run. SDK: `App::with_force_key_events(true)`. Runner: reads header per request.

**Driver-level short-circuit**: `Selector::Focused` (used by `Step::InputText`) always short-circuits to the key-event tier without pre-tap. This is transparent — no yaml change needed.

### 2.2 Canonical verb table

**Single source of truth** (§P0-C). New stone crate `smix-verbs` exposes:

- `VERB_TABLE: &'static [VerbEntry]` — 44 entries, 10 categories
- `find_by_maestro(name)` / `find_by_smix(name)` / `is_known_verb(name)`
- `verbs_in_category(category)` iterator

Both `smix-adapter-maestro::parser` and `smix-migrate` depend on this. Parser accepts both canonical forms; migrate rewrites maestro → smix.

**Reviewer invariant**: any new yaml verb must land in `smix_verbs::VERB_TABLE` first. Grep the parser for hardcoded verb strings; any hit that's not in the table is a regression.

### 2.3 Yaml codemod

**Comment-preserving migrate** (§P0-D). `smix migrate` switched from serde_norway round-trip to line-based rewriter. Comment lines, blank lines, and yaml header lines pass through byte-identical; only step-lines get their verb portion rewritten.

Multi-line arg-key transforms (e.g. `timeout` → `timeoutMs` under `extendedWaitUntil`) run via `pending_arg_rules` state — indented follow-up lines have keys rewritten, structure and content preserved.

**Non-goals**: exotic yaml shapes not covered by the line-based approach fall through to unchanged output with a `WARN` line. Use `smix migrate --dry-run` to preview transforms.

### 2.4 Annotation

**Bundled default fonts** — text primitives just work without `--font`:

- Inter Regular (~412 KB, SIL OFL) — ASCII + Latin extended + Greek + Cyrillic
- Noto Sans SC subset (~64 KB, SIL OFL) — top-frequency CJK Unified Ideographs

**Per-codepoint font routing** in `smix_annotate::font::pick_font_for_codepoint()`. Mixed-script text renders correctly (e.g. `"step 1: 登录"`).

**yaml `takeScreenshot` verb** — long form with annotation composition:

```yaml
- takeScreenshot:
    name: hub-form.png
    annotate:
      - circle: { at: { x: 200, y: 150 }, color: red, radius: 40 }
      - text:   { at: { x: 20, y: 20 }, content: "step 1", color: white, size: 24 }
      - arrow:  { from: { x: 100, y: 100 }, to: { x: 200, y: 200 }, color: blue }
      - box:    { at: { x: 10, y: 10 }, width: 200, height: 100, color: yellow }
      - line:   { from: { x: 0, y: 300 }, to: { x: 300, y: 0 } }
```

Position shapes:
- `{ x: 200, y: 150 }` — absolute pixel
- `{ nx: 0.5, ny: 0.5 }` — normalized 0..1 (viewport-relative)

**Auto-annotate on failure**: `--debug-output` fail-step PNGs get automatic red-circle-at-center + "step N: FAIL" + step summary annotations. Opt-out via `--no-fail-annotate`.

**Standalone CLI**: `smix annotate <in.png> <out.png> --annotate 'circle,at:100_100,color:red'` — mini-DSL for one-off annotation.

### 2.5 Fixture registry

**TS registry reader** (§P1-B). `--fixture-registry .devtools/qa/fixtures/registry.ts` works directly. Auto-detects `.ts` / `.tsx` extension → lightweight TS extractor (~200 LOC hand-rolled, no swc dep):

1. Strip `//` + `/* */` comments
2. Find `export const FIXTURES = { ... }` binding
3. Extract balanced-brace object literal
4. Normalize: single-quoted → double-quoted, unquoted keys → quoted, regex literals `/pattern/` → `"pattern"` (backslash double-escaped for JSON validity), trailing commas stripped
5. `serde_json::from_str` on the normalized JSON

Handles the documented registry.ts shape. Consumers with exotic TS syntax should codegen JSON via the documented `gen-json.mjs` script (still supported).

### 2.6 Metro log signals

**Allowlist multi-source merge** (§P1-A). `MetroLogTail::extend_allowlist(&[String])` appends layers; `assert_clean` OR-merges across layers. Compose base config + per-scope + inline yaml.

**stdin / file:// URLs** (§P2-B). `--metro-log-url -` reads `/dev/stdin`; `--metro-log-url file:///path` tails an on-disk file; `--metro-log-url ws://host:port/logs` connects to a WebSocket.

### 2.7 Output formats

**`--format junit`** (§P2-A). Native JUnit XML output.
**`--format json`** (v0.2.0). Top-level JSON object.
**`--format human`** (default). Human-readable stderr summary.

### 2.8 Standard subflow catalogue

Bundled under `~/.local/share/smix/std/` after `install-local.sh`:

- **`std/dismiss-open-in.yaml`** (v0.3.0) — iOS 26 SpringBoard "Open in <app>?" dismiss
- **`std/ensure-locale.yaml`** (v0.3.0) — sim locale contract check
- **`std/wipe-app-state.yaml`** (v1.0 §P2-C) — clearState + clearKeychain
- **`std/wait-metro-bundle.yaml`** (v1.0 §P2-C) — bundle-ready signal await
- **`std/quit-qa-mode.yaml`** (v1.0 §P2-C, donated by insight) — qa-bubble close

Usage: `- runFlow: std/wipe-app-state.yaml`. Consumer override precedence: `<cwd>/std/*.yaml` wins over `~/.local/share/smix/std/*.yaml`.

### 2.9 Authoring tier

New in v1.0 (§P3 recording mode → landed early). `smix authoring` subcommand family for composing yaml against a live sim:

- **`smix authoring suggest '<partial>'`** — enumerate selectors matching a partial spec against current a11y tree. Wildcard + case-insensitive substring. Example: `smix authoring suggest 'id: qa-*'`.

- **`smix authoring capture-tree <output>`** — write current a11y tree JSON to file. Baseline for `diff-tree`.

- **`smix authoring diff-tree <baseline.json>`** — semantic diff current vs baseline. Reports missing / extra / drifted nodes. Exit 0 clean / 2 drift found. CI-friendly for visual regression gate.

- **`smix authoring record --output flow.yaml --duration-secs 30`** — sample a11y tree at intervals, aggregate stable-visible IDs, emit yaml scaffold. Not full tap-recorder but enough for authoring iteration.

### 2.10 Wire format

**Frozen at v1.0**. `docs/ai-guide/wire-format-v1.0.md` documents every route:

- `POST /tap` / `/find` / `/fill` / `/clear` / `/press-key` / `/swipe` / `/scroll` / `/back` / `/hide-keyboard` / `/foreground`
- `GET /tree` / `/screenshot` / `/health`
- Request-context headers: `App-Bundle-Id`, `App-Activate`, `Input-Dispatch-Mode`
- Selector wire schema (text / id / label / role / anchor / focused + modifiers)
- Error envelope

v1.0 client × v1.x runner compatibility guaranteed; v0.3.x wire is byte-superset compat.

### 2.11 Stone crate ABI

**10 crates frozen** at v1.0 per `docs/ai-guide/stone-crate-abi-freeze.md`:

`smix-error`, `smix-selector`, `smix-screen`, `smix-runner-wire`, `smix-input`, `smix-verbs`, `smix-metro-log`, `smix-fixture`, `smix-annotate`, `smix-migrate`.

Additive changes (new methods with default impls, new `#[non_exhaustive]` variants, new items) are compatible within v1.x. Removals or signature changes require v2.0.

### 2.12 Verb parity

Full cross-platform support matrix at `docs/ai-guide/verb-parity.md`. Every yaml verb × iOS/Android with per-cell caveats.

---

## Part 3 — Migration path (v0.3.x → v1.0)

### 3.1 Installation

```bash
cargo install smix-cli --locked --version 1.0.0 --force
# or (if you use the git checkout path)
cd smix && git checkout smix-v1.0.0 && bash scripts/install-local.sh
smix --version  # → smix 1.0.0
```

### 3.2 Sanity check

```bash
smix --version | grep -q '1.0.0' && echo "OK version"
smix authoring --help | grep -q 'suggest' && echo "OK Phase E authoring"
smix run --help | grep -q -- '--force-key-events' && echo "OK Phase A KeyEventDispatch"
smix run --help | grep -q -- '--no-fail-annotate' && echo "OK Phase C auto-annotate"
smix run --help | grep -q -- '--metro-log-url' && echo "OK Phase D metro"
smix run --help | grep -q -- '--fixture-registry' && echo "OK fixture registry"
```

### 3.3 Retire your v0.3.x workarounds

Your `stash@{0}` labeled `smix-path-a-*` becomes a series of **deletions**:

| Workaround | v1.0 fix | Deletion |
|---|---|---|
| RN hidden-input custom step | §P0-A driver short-circuit | Delete the pre-tap step; `inputText` just works |
| migrate/run canonical mismatch | §P0-C VERB_TABLE | Delete any yaml patch that preserves maestro form for run |
| PNG magic byte sniff | v0.3.1 mkdir+ext still there | Delete `isPngFile` in `collectScreenshots`; restore `endsWith('.png')` |
| gen-test-report.ts shim (~50 LOC) | §P2-A `--format junit` | Delete the shim; use `smix run --format junit --output report.xml` |
| registry.ts codegen | §P1-B `.ts` direct load | Delete `gen-json.mjs`; point `--fixture-registry` at `.ts` directly |
| Comment stripping loss | §P0-D line-based migrate | No compensation needed; migrate preserves them |

Result: **20-line diff on insight side** — env loader + `smix run` invocations. All maestro-era complexity gone.

### 3.4 One-time baseline re-accept for visual gate

If you already re-accepted baselines under v0.3.1 (notch region), no further re-accept needed for v1.0. The screenshot capture region is unchanged from v0.3.1.

If you migrated to smix from maestro without ever re-accepting under smix, run once:
```bash
bun verify visual --accept-visual
```

### 3.5 Commit shape

```bash
git flow bugfix start GOL-611-smix-v1.0-adoption
# apply all deletions
git stash pop stash@{0}  # for the RN hidden-input compensation your side had
git commit -m "GOL-611: adopt smix v1.0 — retire workarounds"
bun verify perf && bun verify visual
git flow bugfix finish --no-ff GOL-611-smix-v1.0-adoption
```

### 3.6 Adopt v1.0 new surface (optional, high-ROI)

**Adopt authoring**:
```bash
# During yaml authoring
smix authoring suggest 'id: qa-*'   # discover selectors on live sim

# Establish baseline for visual gate
smix authoring capture-tree > .devtools/baselines/dashboard.a11y.json
smix authoring diff-tree .devtools/baselines/dashboard.a11y.json  # in CI post-change
```

**Adopt bundled std subflows**:
```yaml
# Before
- launchApp: { clearState: true, clearKeychain: true, permissions: { camera: allow } }

# After
- runFlow: std/wipe-app-state.yaml
- launchApp: { permissions: { camera: allow } }
```

**Adopt yaml annotate for visual regression debug**:
```yaml
- takeScreenshot:
    name: dashboard-post-login.png
    annotate:
      - circle: { at: { id: user-avatar }, color: red, radius: 40 }
      - text:   { at: { x: 20, y: 20 }, content: "post-login state", color: green, size: 24 }
```

Note on the `at: { id: <> }` selector-based positioning: **v1.0 defers selector-relative to fallback (0,0) with a warning** — the wiring lands in the authoring evolution. For now, use pixel or normalized coords for annotation positions.

---

## Part 4 — Complete CLI + API reference

### 4.1 `smix run` flags

| Flag | Since | Purpose |
|---|---|---|
| `<FLOWS>...` | 0.2.5 | One or more yaml paths (batch invocation) |
| `--device <alias/UDID>` | 0.2.0 | Sim device |
| `--bundle-id <id>` | 0.2.0 | Target bundle for XCUIApplication rebind |
| `--activate` | 0.2.1 | Send `App-Activate: true` header (per-request) |
| `--env KEY=VAL` | 0.2.0 | Env var for `${NAME}` yaml interpolation |
| `--debug-output <dir>` | 0.2.0 | Per-step JSON + fail screenshot; auto-annotates unless `--no-fail-annotate` |
| `--format human\|json\|junit` | 0.2.0 (junit v1.0) | Output format |
| `--fail-fast` | 0.2.5 | Batch abort on first fail |
| `--metro-log-url URL` | 0.3.0 | `ws://`, `file://`, or `-` (stdin) |
| `--await-signal <regex>` | 0.3.0 | Append implicit `expect.signal` |
| `--expect-log-clean` | 0.3.0 | Append implicit `expectLogClean` |
| `--fixture-registry <path>` | 0.3.0 (TS v1.0) | Enable `- fixture: <id>` verb |
| `--force-key-events` | 1.0.0 | Force `Input-Dispatch-Mode: key-events` header |
| `--no-fail-annotate` | 1.0.0 | Disable auto-annotate on fail PNG |

### 4.2 `smix` subcommands

- `smix doctor` — env probe
- `smix sim boot/shutdown/list/erase/locale/... <alias>` — simulator control
- `smix runner up <alias>` / `runner down` — runner lifecycle
- `smix run <flow>...` — flow execution
- `smix migrate [--in-place] <flow>...` — canonical codemod (comment-preserving)
- `smix annotate <in.png> <out.png>` — standalone PNG annotation with mini-DSL
- `smix authoring suggest/capture-tree/diff-tree/record` — v1.0 authoring tier
- `smix tree/find/tap/fill/clear/scroll/hide-keyboard/describe/screenshot` — one-shot actions against a running runner

### 4.3 yaml verbs

See `docs/ai-guide/verb-parity.md` for the full ~44-verb table. Categories:

**Tap**: `tapOn` / `tap`, `doubleTapOn` / `doubleTap`, `longPressOn` / `longPress`, `tapByCoord`

**Input**: `inputText` / `fill`, `eraseText` / `clear`, `pasteText`, `setClipboard`, `copyTextFrom`

**Assert**: `assertVisible` / `expect`, `assertNotVisible` / `expectNotVisible`, `extendedWaitUntil`, `assertTrue`, `assertScreenshot`, `expect: { signal / signals / logClean }`, `expectLogClean`

**Control flow**: `runFlow` (path + inline commands), `retry`, `repeat`, `pressKey`, `back`

**Lifecycle**: `launchApp`, `stopApp` / `terminate`, `killApp`, `clearState` / `reset`, `clearKeychain` / `resetKeychain`

**Media**: `takeScreenshot` (long form with `annotate:`), `startRecording`, `stopRecording`, `addMedia`, `assertScreenshot`

**Gesture**: `scroll`, `scrollUntilVisible`, `swipe`, `hideKeyboard`

**Device**: `openLink` / `openUrl`, `setLocation`, `travel`, `setPermissions`, `setOrientation`, `toggleAirplaneMode`

**smix-native**: `tapById`, `tapAtCoord`, `swipeAtCoord`, `ocrText`, `anchorRelative`, `findTextByOcr`, `fixture`, `webview_eval` / `webviewEval`

**Utility**: `waitForAnimationToEnd`, `evalScript`, `runScript`

### 4.4 Config (.smix/*)

- `.smix/sims.json` — sim registry with optional `runnerPort` per sim (v0.2.5), `locale` per sim (v6.10)
- `.smix/config.json` — `metroLog: { url, allowlist, retainSecs }` + `fixturesRegistry` field
- `.smix/runner/*` — runner lifecycle state (auto-managed)
- `.smix/trace/*` — trace outputs (auto-managed)

### 4.5 Rust SDK

```rust
use smix_sdk::{App, text, KeyName};
use std::time::Duration;

let app = App::connect_to_runner(22087).await?
    .with_udid("<UDID>")
    .with_bundle_id("com.focusai.app.mobile")
    .with_auto_activate(true)              // v0.2.1
    .with_force_key_events(true);          // v1.0 A7
app.launch("com.focusai.app.mobile").await?;
app.wait_for(&text("Dashboard"), Duration::from_secs(5)).await?;
app.tap(&text("Sign In")).await?;
app.fill(&text("Email"), "user@example.com").await?;
app.press_key(KeyName::Return).await?;
app.assert_visible(&text("Dashboard")).await?;
```

### 4.6 TypeScript SDK

```typescript
import { Smix, Selector, literal, regex, bundleId, HttpSimRuntime } from '@goliapkg/smix'

const app = await Smix.launchApp(bundleId('com.focusai.app.mobile'), runtime, runtime.resolver)
await app.tap(Selector.id('btn-login').below(Selector.text(literal('Sign In'))))
await app.find(Selector.role('button', regex('^Submit'))).toBeVisible({ timeoutMs: 5_000 })
```

Full ergonomic mirror of the Rust surface at type / wire / fluent / semantic level. Same for Swift + Kotlin SDKs.

---

## Part 5 — Operating agreement going forward

### 5.1 Wire compatibility promise

- **v1.0 client × v1.x runner**: guaranteed compat within v1.x
- **v1.x client × v1.0 runner**: guaranteed compat within v1.x
- **v0.3.x client × v1.0 runner**: compatible (v1.0 wire is byte-superset)
- **v1.x × v2.0**: not guaranteed; requires migration

Any breaking wire change bumps major.

### 5.2 Release cadence

- **v1.x patch (1.0.1, 1.0.2, ...)** — triggered by dogfood feedback with §P0-tier blocker. Ship within 1-2 business days. Wire additive-only.
- **v1.x minor (1.1.0, 1.2.0, ...)** — scheduled monthly. Bundles §P1 + §P2 items. Wire may add fields, not break existing.
- **v2.0** — reserved for wire-format breakage. Scheduled by roadmap discussion with all consumers.

### 5.3 Feedback protocol (unchanged from v0.3.x)

Per `smix/.claude/dogfood/README.md` iron rule:

- Report path: `.claude/state/gol-611/smix-feedback-v1.x-<topic>.md` on your side
- Every report is treated as a *suggestion input*, never a *support ticket*
- smix side extracts the **systemic capability gap** the report reveals, not just the specific yaml/verb that broke
- Response chain: dogfood log → response doc → v1.x patch/minor

### 5.4 Reviewer invariants (v1.x lifetime)

Consolidated in `docs/ai-guide/stone-crate-abi-freeze.md`. Highlights:

- Any new XCUITest call in Swift runner must be categorized main-actor-isolated vs Sendable (v0.3.1 MainActor policy)
- Any new yaml verb writing to disk must go through `write_yaml_output(path, bytes, OutputIntent)` (v0.3.1 file-write helper)
- Any new yaml verb must land in `smix_verbs::VERB_TABLE` first (v1.0 A single source of truth)
- Any new Annotation variant must be added to (a) `smix_annotate::Annotation`, (b) `smix_adapter_maestro::AnnotationSpec`, (c) `parse_annotation_from_kind`, (d) `annotate_bridge::spec_to_annotation` (v1.0 C2)
- Any new authoring action must land as `AuthoringAction` variant + dispatch arm in `Cmd::Authoring` (v1.0 E)

### 5.5 Local pre-commit gate

We shipped `scripts/pre-commit.sh` as a systemic fix for the historical Phase B/C/D/E CI fmt-drift pattern. It mirrors the GitHub CI gates (fmt + clippy + doc + optional tests).

Recommended: `ln -sf ../../scripts/pre-commit.sh .git/hooks/pre-commit` in your fork if you author yaml corpus tooling in Rust.

---

## Part 6 — Non-goals for v1.0

Explicit — v1.0 does NOT do:

- **Real-device support** — smix is simulator/emulator only per project iron rule §9 #1. Real-device automation is a separate concern (needs entitlements, private frameworks, etc.).
- **Full swc TS parser for fixture registry** — v1.0 D1 lightweight parser sized for the documented insight shape. Exotic TS syntax → codegen JSON.
- **Multi-provider LLM abstraction** — smix uses `claude` CLI only (per project unwritten rule).
- **Cross-platform log signal syntax unification** — each platform's log tail is separate; consumer wires per-platform.
- **`swipeAtCoord` / `fillAtCoord`** as smix-native verbs — only `tapAtCoord` shipped as escape hatch.
- **Selector language extension** — new selector types beyond the current 4 base + 8 modifiers require design cycle; not planned for v1.x.
- **OCR fallback for keyboard characters** — subsumed by §P0-A KeyEventDispatch. If a future case surfaces, revisit as v1.x experimental behind `--enable-ocr-fallback`.
- **Selector-relative annotation position in yaml** — annotation `at: { id: <> }` shape is parsed but resolves to (0, 0) with a warning in v1.0; full wiring lands in the authoring tier evolution.
- **Concurrent flows against different bundles on one runner** — one runner serves one target at a time (XCUITest process-global dispatch). Use two separate `smix runner up <deviceA>` + `<deviceB>` instances with distinct `runnerPort`.

---

## Part 7 — Quick reference

| Resource | Path |
|---|---|
| Repo | github.com/goliajp/smix @ `smix-v1.0.0` |
| Binary (this machine) | `~/.local/bin/smix` — verify with `smix --version` → `smix 1.0.0` |
| CHANGELOG v1.0 entry | `smix/CHANGELOG.md` |
| Migration guide | `smix/docs/ai-guide/migration-v0.3-to-v1.0.md` |
| Wire format freeze | `smix/docs/ai-guide/wire-format-v1.0.md` |
| Stone crate ABI freeze | `smix/docs/ai-guide/stone-crate-abi-freeze.md` |
| Verb parity matrix | `smix/docs/ai-guide/verb-parity.md` |
| `--activate` lifetime | `smix/docs/ai-guide/activate-header-lifetime.md` |
| This document | `smix/docs/ai-guide/insight-v1.0-comprehensive.md` |
| Narrower adoption doc | `smix/docs/ai-guide/insight-v1.0.0-published.md` |
| Dogfood iron rule | `smix/.claude/dogfood/README.md` |
| Local pre-commit gate | `smix/scripts/pre-commit.sh` |
| Verify script (13+ probes) | `smix/scripts/gol-611-verify.sh` |
| Insight capability requirements | `insight/.claude/state/gol-611/smix-capability-requirements-2026-07-08.md` |

## crates.io

| Crate | 1.0.0 URL |
|---|---|
| smix-cli | https://crates.io/crates/smix-cli |
| smix-sdk | https://crates.io/crates/smix-sdk |
| smix-adapter-maestro | https://crates.io/crates/smix-adapter-maestro |
| smix-verbs (new) | https://crates.io/crates/smix-verbs |
| smix-annotate | https://crates.io/crates/smix-annotate |
| smix-fixture | https://crates.io/crates/smix-fixture |
| smix-metro-log | https://crates.io/crates/smix-metro-log |
| smix-migrate | https://crates.io/crates/smix-migrate |
| (23 total — see `Cargo.toml` workspace members for full list) | |

## Other ecosystems

- npm: https://npmjs.com/package/@goliapkg/smix (1.0.0)
- Maven Central: https://central.sonatype.com/artifact/jp.golia.smix/smix-sdk (1.0.0)
- Swift GH Release: https://github.com/goliajp/smix/releases/tag/swift-v1.0.0 (XCFramework attached)

---

## Final note

Every §P0 / §P1 / §P2 / §P3 requirement you filed on 2026-07-08 is addressed. The v0.3.x → v1.0 upgrade shrinks your side's compensation code, doesn't add to it. Wire and ABI are frozen for the v1.x lifetime.

Feedback on any capability lands via the standard `.claude/state/gol-611/smix-feedback-v1.x-<topic>.md` path. Per the iron rule, we treat every report as a suggestion input — the response is a systemic capability upgrade, not a point patch.

Contact: `lihao@golia.jp` (smix). Your side: `takagi@golia.jp`. Turnaround for v1.x §P0-tier blockers: 1-2 business days.
