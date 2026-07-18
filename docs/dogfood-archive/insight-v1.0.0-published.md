# smix v1.0.0 published — industrial-grade release, all §P0-§P3 closed

Date: 2026-07-08
Prior chain: `insight-v0.3.1-published.md` → v1.0 mega-cycle (Phase A-I in one release, no intermediate ships).

Insight capability requirements 2026-07-08 all closed:
- **§P0-A** RN hidden-input pattern — v1.0 Phase A KeyEventDispatch
- **§P0-B** Keyboard characters — subsumed by §P0-A key-event tier
- **§P0-C** migrate ↔ run verb agreement — v1.0 Phase A verb table single source of truth
- **§P0-D** Comment-preserving codemod — v1.0 Phase B line-based rewriter
- **§P1-A** Metro log signals — v1.0 Phase D2 allowlist multi-source
- **§P1-B** Fixture registry — v1.0 Phase D1 TS reader
- **§P2-A** `--format junit` — v1.0 Phase D3
- **§P2-B** `--metro-log-url` stdin — v1.0 Phase D4
- **§P2-C** Std subflow expansion — v1.0 Phase D5 (3 new files including donated quit-qa-mode)
- **§P2-D** `--activate` sticky docs — v1.0 Phase D6
- **§P3** Coverage + parity — v1.0 Phase E authoring + Phase F verb parity table

## 0. What actually shipped

| Ecosystem | Coordinate | Version | Notes |
|---|---|---|---|
| crates.io | `smix-cli` + 22 stones | 1.0.0 | 22 → 23 crates (new: smix-verbs) |
| npm | `@goliapkg/smix` | 1.0.0 | |
| Maven Central | `jp.golia.smix:smix-sdk` | 1.0.0 | |
| Swift Package (GitHub Release) | `SmixCoreFFI.xcframework.zip` | swift-v1.0.0 | |

Wire format frozen. Stone crate ABI frozen. See `docs/ai-guide/wire-format-v1.0.md` + `docs/ai-guide/stone-crate-abi-freeze.md`.

## 1. Install

```bash
cargo install smix-cli --locked --version 1.0.0 --force
# or
cd path/to/smix && git checkout smix-v1.0.0 && bash scripts/install-local.sh
smix --version    # → smix 1.0.0
```

### Sanity check

```bash
smix --version | grep -q '1.0.0' && echo "✅ smix 1.0.0"
smix authoring --help | grep -q 'suggest' && echo "✅ Phase E authoring surfaced"
smix run --help | grep -q '\-\-force-key-events' && echo "✅ Phase A KeyEventDispatch"
smix run --help | grep -q '\-\-format junit' 2>/dev/null; \
    smix run --format junit /dev/null --device stub 2>&1 | grep -q "testsuite\|error" && echo "✅ Phase D3 junit format"
smix run --help | grep -q '\-\-no-fail-annotate' && echo "✅ Phase C3 auto-annotate"
smix run --help | grep -q '\-\-metro-log-url' && echo "✅ Phase D4 metro-log-url"
smix run --help | grep -q '\-\-fixture-registry' && echo "✅ Phase D1 fixture-registry"
```

## 2. Complete adoption walkthrough

### 2.1 Retire the v0.3.1 workarounds one-by-one

Insight `stash@{0}` `smix-path-a-*` gets applied to the code, then walk each workaround:

**Workaround 1: RN hidden-input** — v1.0 Phase A resolves. Add `--force-key-events` if you want it explicit:

```typescript
// Before (v0.3.x): custom yaml step to tap wrapper first
- tapOn: qa-passcode-wrapper
- inputText: '123456'  // fails with ElementNotFound

// After (v1.0): inputText Just Works.
- inputText: '123456'  // v1.0 Driver::fill short-circuits Focused
```

**Workaround 2: migrate/run canonical mismatch** — v1.0 Phase A resolves. `smix migrate` outputs `tap`; `smix run` accepts both. Codemod round-trip clean.

**Workaround 3: PNG magic byte sniff** — v1.0 Phase C keeps `takeScreenshot` auto-`.png` extension inference from v0.3.1. Delete magic byte sniff in `collectScreenshots`; restore `endsWith('.png')`.

**Workaround 4: gen-test-report.ts shim** — v1.0 Phase D3 native junit. Delete ~50 LOC:

```typescript
// Before
const jsonReport = execSync(`smix run ${flow} --format json`)
const junit = convertJsonToJunit(jsonReport)  // ~50 LOC
writeFileSync('report.xml', junit)

// After (v1.0)
execSync(`smix run ${flow} --format junit --output report.xml`)
```

**Workaround 5: registry.ts codegen** — v1.0 Phase D1 native TS reader. Delete `gen-json.mjs`:

```bash
# Before
node .devtools/qa/fixtures/gen-json.mjs
smix run --fixture-registry .devtools/qa/fixtures/registry.json ...

# After (v1.0)
smix run --fixture-registry .devtools/qa/fixtures/registry.ts ...
```

**Workaround 6: Comment stripping** — v1.0 Phase B line-based codemod. Migrate all 12 yaml scope files:

```bash
smix migrate --in-place .devtools/qa/sim/**/*.yaml
# Copyright headers, GOL-<n> audit trails, # Reason: blocks all survive
git diff | grep -E "^-#|^\+#" | wc -l  # → 0 (comments unchanged)
```

### 2.2 Adopt new v1.0 authoring surface (optional, high-ROI)

```bash
# Discover selectors on live sim
smix authoring suggest 'id: qa-*'
# → lists all id-matching candidates with role + bounds

# Capture baseline for visual gate
smix authoring capture-tree > .devtools/baselines/dashboard.a11y.json

# Diff after code change
smix authoring diff-tree .devtools/baselines/dashboard.a11y.json
# → exit 0 clean / exit 2 drift; CI-friendly

# Session record (initial yaml scaffold)
smix authoring record --duration-secs 30 --output new-flow.yaml
# → captures stable IDs, emits assertVisible scaffold
```

### 2.3 Adopt new std subflows

```yaml
# Before
- launchApp:
    clearState: true
    clearKeychain: true

# After (v1.0)
- runFlow: std/wipe-app-state.yaml
```

Also available:
- `std/wait-metro-bundle.yaml` (bundle-ready signal await)
- `std/quit-qa-mode.yaml` (donated by insight §P2-C)

## 3. Full v1.0 CLI surface reference

| Flag | Since | Purpose |
|---|---|---|
| `--activate` | 0.2.1 | Send App-Activate: true header (per-request) |
| `--bundle-id <id>` | 0.2.0 | Target bundle for rebind |
| `--env KEY=VAL` | 0.2.0 | Env var for `${NAME}` interpolation |
| `--debug-output <dir>` | 0.2.0 | Per-step JSON + fail screenshot |
| `--format human\|json\|junit` | 0.2.0 (junit v1.0 D3) | Output format |
| `--fail-fast` | 0.2.5 | Batch abort on first fail |
| `--metro-log-url ws://\|file://\|-` | 0.3.0 (stdin v1.0 D4) | Metro log source |
| `--await-signal <regex>` | 0.3.0 | Append implicit expect.signal |
| `--expect-log-clean` | 0.3.0 | Append implicit expectLogClean |
| `--fixture-registry <path.json\|path.ts>` | 0.3.0 (TS v1.0 D1) | Fixture chip registry |
| `--force-key-events` | 1.0.0 A7 | RN hidden-input key event dispatch |
| `--no-fail-annotate` | 1.0.0 C3 | Disable auto-annotate on fail PNG |

Plus subcommands:
- `smix runner up/down` — runner lifecycle
- `smix sim boot/shutdown/locale/...` — simulator control
- `smix run <flow>` — flow execution
- `smix migrate` — yaml codemod (v1.0 comment-preserving)
- `smix annotate` — standalone PNG annotation
- `smix authoring <suggest|capture-tree|diff-tree|record>` — v1.0 authoring tier

## 4. Non-goals for v1.0 (deferred to v2.0)

Explicit — v1.0 does NOT do:

- Real-device support (sim/emulator only, iron rule)
- swc-based full TS parser (D1 lightweight extractor covers documented shape)
- Cross-platform log signal syntax unification
- Multi-provider LLM abstraction (single Claude CLI)

## 5. Feedback loop (unchanged from v0.3.x)

Same as prior: report via `.claude/state/gol-611/smix-feedback-v1.x-<topic>.md`. Iron rule per `.claude/dogfood/README.md`.

- **v1.x patch releases** — cadence: dogfood-driven (1-2 business days for §P0)
- **v1.x minor releases** — cadence: monthly
- **v2.0** — reserved for wire-format breakage; v1.x remains additive-only

## 6. Quick reference (updated 2026-07-08)

| Resource | Path |
|---|---|
| Repo | github.com/goliajp/smix @ `smix-v1.0.0` |
| Binary (this machine) | ~/.local/bin/smix (smix --version → 1.0.0) |
| CHANGELOG v1.0 entry | smix/CHANGELOG.md |
| Migration guide | smix/docs/ai-guide/migration-v0.3-to-v1.0.md |
| Wire format freeze | smix/docs/ai-guide/wire-format-v1.0.md |
| Stone crate ABI freeze | smix/docs/ai-guide/stone-crate-abi-freeze.md |
| Verb parity matrix | smix/docs/ai-guide/verb-parity.md |
| --activate lifetime | smix/docs/ai-guide/activate-header-lifetime.md |
| Dogfood iron rule | smix/.claude/dogfood/README.md |
| verify script (15+ probes) | smix/scripts/gol-611-verify.sh |
