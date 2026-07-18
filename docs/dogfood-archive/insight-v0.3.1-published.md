# smix v0.3.1 published — MainActor policy + file-write helper (all 4 blockers resolved)

Date: 2026-07-08
Prior chain: `insight-v0.3.0-published.md` → `gol-611-v0.3.1-response.md` (plan) → **this doc** (shipped).

Fixes insight's feedback in `smix-feedback-v0.3.0-activate-thread.md` (5 issues → 2 systemic capability gaps → v0.3.1 primitive fixes). Dogfood record: `.claude/dogfood/2026-07-08-insight-activate-thread-and-screenshot-writes.md`.

Semver patch bump (0.3.0 → 0.3.1). Byte-compat with v0.3.0 clients and servers.

---

## 0. What actually shipped

| Ecosystem | Coordinate | Version |
|---|---|---|
| crates.io | 22 crates (all workspace crates bumped) | 0.3.1 |
| npm | `@goliapkg/smix` | 0.3.1 |
| Maven Central | `jp.golia.smix:smix-sdk` | 0.3.1 |
| Swift Package (GitHub Release) | `SmixCoreFFI.xcframework.zip` | swift-v0.3.1 |

---

## 1. Issues resolved

### §A `--activate` no longer crashes on main-thread violation (feedback §1)

Root cause at `SmixRunnerUITests.swift:754` — v0.2.1's `resolveApp` closure called `XCUIApplication.activate()` from an off-main async continuation. iOS 26.5 SDK requires it on the main queue → `NSInternalInconsistencyException`.

**Systemic fix** (not point patch):
- `SmixRunnerServer.onMain<T>(_ body: @MainActor () -> T) async -> T` — canonical hop helper
- `resolveApp` closure is now `async @Sendable` — wraps `.activate()` in `await SmixRunnerServer.onMain { … }`
- All 15 app-touching handlers cascade `let app = await resolveApp()` (was `let app = resolveApp()`)
- `foregroundHandler` at line 1425 also hopped through `SmixRunnerServer.onMain`
- Setup lines 722-723 (`.launch()` / `.activate()` in `test_runForever`) documented as on-main by XCTest's execution contract (no explicit hop needed)

Invariant: any new XCUITest call added to the runner must be categorized `main-actor-isolated` vs `Sendable` at review time. The comment on `SmixRunnerServer.onMain` names it as the audit point.

**Verified**: `scripts/gol-611-verify.sh §8` — `smix run --activate --bundle-id com.apple.MobileSafari` runs against a real sim; grep runner log for `NSInternalInconsistencyException` → absent.

### §B `takeScreenshot` mkdir-p + `.png` inference + fail-loud (feedback §2/§3/§4)

Root cause at `crates/smix-adapter-maestro/src/runtime.rs:1174-1185` — 12 lines inlining three defects:
1. No `create_dir_all(parent)` → silent write fail on fresh cwd
2. No extension inference → `takeScreenshot: sub/dir/hub-form` (no ext) lands as `.../hub-form` (breaks downstream `endsWith('.png')` filters, breaks maestro compat)
3. Write failure = `warnings.push(…)` + `Ok(RunStepReport::Ok)` → load-bearing verb silent-succeeds

**Systemic fix** (not point patch):
- New module `crates/smix-adapter-maestro/src/output.rs`
- `write_yaml_output(path, bytes, OutputIntent)` canonical helper
- `OutputIntent::LoadBearing` — write failure → `io::Error` → `RunError::Io` → `runOutcome:failure`
- `OutputIntent::BestEffort` — write failure → warning only (debug artifacts)
- `write_yaml_output_lenient` convenience wrapper for BestEffort tier
- Extension inference: PNG magic `89 50 4E 47 0D 0A 1A 0A` → `.png`; JPEG `FF D8 FF` → `.jpg`
- 9 unit tests including mkdir-p / ext-inference / fail-loud / lenient-swallow paths

Retrofit:
- `Step::TakeScreenshot` → `OutputIntent::LoadBearing` (feedback §2/§3/§4 addressed)
- `write_step_debug` fail-PNG + step-JSON writes → `OutputIntent::BestEffort` (adds mkdir invariant for deep `--debug-output` dirs)
- `Step::StartRecording` and assertScreenshot baseline record left for v0.3.5 (path handling is inside simctl / SDK layers, not adapter)

Reviewer invariant: grep `smix-adapter-maestro/src/` for `fs::write` — every non-test hit should be in `output.rs`.

**Verified**: `scripts/gol-611-verify.sh §7a + §7b`:
- §7a — `takeScreenshot: sub/dir/hub-form` in fresh cwd → file lands at `sub/dir/hub-form.png` (auto mkdir + `.png`)
- §7b — parent is a regular file → write fails → exit 5 (RunError::Io), not silent 0

### §5 notch region (feedback informational)

Consumer choice. smix captures full sim frame; maestro cropped notch. On first migration to smix baselines, run `bun verify visual --accept-visual` once to re-record. From then on stable.

Not a smix code change. Adoption doc `insight-v0.3.0-published.md` §7.5 documents the one-time cost.

---

## 2. Install v0.3.1

```bash
cargo install smix-cli --locked --version 0.3.1 --force
# or, if git-cloned:
cd path/to/smix && git checkout smix-v0.3.1 && bash scripts/install-local.sh
smix --version    # → smix 0.3.1
```

### Sanity check

```bash
smix --version | grep -q '0.3.1' && echo "✅ smix 0.3.1"
# Verify script: 12/12 PASS on a booted sim
bash smix/scripts/gol-611-verify.sh <device> | grep -E "PASS: 12"
```

---

## 3. Applying v0.3.1 on insight side

### 3.1 Pop the stash + drop the workarounds

`stash@{0}` on your side is labeled `smix-path-b-v0.3.0-workaround-blocked-by-screenshot-issues` per feedback §187.

```bash
cd /Users/doracawl/workspace/qualcomm/insight
git flow bugfix start GOL-611-smix-path-b-final
git stash pop stash@{0}
```

The stash contains four workarounds that v0.3.1 makes redundant:

1. **Drop `mkdirSync(join(outDir, '.devtools/maestro/screenshots'), { recursive: true })` before `smix run`** — v0.3.1's `write_yaml_output` does mkdir-p automatically
2. **Drop PNG-magic-byte sniff in `collectScreenshots`** — restore `endsWith('.png')`; v0.3.1 auto-appends the extension
3. **Drop the "skip `--activate`" invariant** — Phase A MainActor hop makes `--activate --bundle-id com.focusai.app.mobile` safe again
4. **Delete "runner up AFTER Insight foreground" invariant docs** — no longer necessary as a resilience workaround; can stay as a convenience note but shouldn't be delivery-gate-critical

### 3.2 Re-add `--activate` to stages/*.ts

```typescript
// .devtools/verify/stages/perf.ts (and stages/visual.ts)
const smixArgs = [
  'run', FLOW_PATH,
  '--platform=' + platform,
  '--no-launch',
  '--debug-output', debugOut,
  '--format', 'json',
  // v0.3.1: safe to re-enable — MainActor hop resolves the crash
  '--activate',
  '--bundle-id', 'com.focusai.app.mobile',
]
```

### 3.3 One-time baseline re-accept (feedback #5)

```bash
bun verify visual --accept-visual    # capture fresh baselines under v0.3.1 driver
bun verify visual                    # verify stable
bun verify perf                      # verify stable
```

After this, baselines are v0.3.1-native; no notch-region flakes.

### 3.4 Commit + merge

```bash
git add -A
git commit -m "GOL-611: adopt smix v0.3.1 Path B — --activate resilient + workarounds retired"
git flow bugfix finish --no-ff GOL-611-smix-path-b-final
```

---

## 4. What v0.3.1 does NOT change

Explicit non-goals:

- Bundled annotation fonts (still v0.3.5 target)
- yaml `takeScreenshot: { annotate: [...] }` verb (still v0.3.5)
- `--debug-output` fail-step auto-annotate (still v0.3.5)
- TS fixture registry reader (still v0.3.5)
- Metro log allowlist multi-source merging (still v0.3.5)
- Notch region crop feature (consumer choice; not a smix change)
- `Step::StartRecording` file-write policy retrofit (path handling lives in SDK/simctl; v0.3.5)

---

## 5. Feedback protocol (unchanged)

Same as v0.3.0 (see `insight-v0.3.0-published.md` §7):

- Where: your side under `.claude/state/gol-611/smix-feedback-v0.3.x-<topic>.md`
- What: name the primitive layer (Phase A MainActor policy / Phase B file-write policy) that a defect hits, not just the yaml verb; that lets us diagnose whether the issue is inside a primitive or in composition
- How it flows: smix side rolls to `.claude/dogfood/<date>-insight-<topic>.md` → response doc → next patch

---

## 6. Regression gate

`scripts/gol-611-verify.sh` v0.3.1 has 12 probes total (was 9 in v0.3.0):

- §1a/b/c — runner-project cascade
- §2a/b — env interpolation + debug-output
- §3 — `--format json`
- §4 — when-false silence
- §5 — sim locale (v6.10 c2)
- §6 — `--bundle-id` rebinds runner (v0.2.1)
- **§7a — mkdir-p + `.png` inference (v0.3.1 NEW)**
- **§7b — write-fail → runOutcome:failure (v0.3.1 NEW)**
- **§8 — `--activate` no main-thread crash (v0.3.1 NEW)**

Any v0.3.x+ release blocks on 12/12 PASS.

---

## 7. Quick reference (updated 2026-07-08)

| resource | location |
|---|---|
| smix repo | `github.com/goliajp/smix` @ `smix-v0.3.1` |
| Binary (this machine) | `~/.local/bin/smix` (`smix --version` → `smix 0.3.1`) |
| CHANGELOG v0.3.1 | `smix/CHANGELOG.md` |
| Response doc (plan) | `smix/docs/ai-guide/gol-611-v0.3.1-response.md` |
| Dogfood log | `smix/.claude/dogfood/2026-07-08-insight-activate-thread-and-screenshot-writes.md` |
| File-write helper | `smix/crates/smix-adapter-maestro/src/output.rs` |
| MainActor hop | `smix/swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift` (`onMain<T>`) |
| Verify probes §7-§8 | `smix/scripts/gol-611-verify.sh` (12/12 PASS on real sim) |
