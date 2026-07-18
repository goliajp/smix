# smix v0.2.0 published — 4 ecosystems live

Date: 2026-07-08
Supersedes: `insight-v0.2.0-shipped.md` (which announced the tag; this doc announces the actual registry pushes).
Audience: the insight engineer choosing an install path — `bun add` from npm now works; no need to git clone smix.

Prior chain: `smix-feedback-path-b-attempt.md` → `gol-611-path-b-response.md` → `insight-integration-guide.md` → `insight-v0.2.0-shipped.md` → **this doc**.

---

## 0. What actually shipped where

| Ecosystem | Coordinate | Version | URL |
|---|---|---|---|
| **crates.io** | 19 crates (smix-error, smix-screen, smix-input, smix-selector, smix-selector-resolver, smix-recorder-ir, smix-simctl, smix-runner-wire, smix-runner-client, smix-host-coord-resolver, smix-recorder, smix-core, smix-driver, smix-adb, smix-ffi, smix-sdk, smix-adapter-maestro, smix-cli, smix-mcp) | 0.2.0 | https://crates.io/crates/smix-cli |
| **npm** | `@goliapkg/smix` | 0.2.0 | https://www.npmjs.com/package/@goliapkg/smix |
| **Maven Central** | `jp.golia.smix:smix-sdk` | 0.2.0 | https://central.sonatype.com/artifact/jp.golia.smix/smix-sdk (10-30 min propagation window from Sonatype Central Portal auto-release) |
| **Swift (GitHub Release)** | `SmixCoreFFI.xcframework.zip` + `.sha256` | swift-v0.2.0 | https://github.com/goliajp/smix/releases/tag/swift-v0.2.0 |

Behind the scenes: the smix repo's four release CI workflows exist but the necessary secrets aren't configured yet (v0.1.0 shipped by hand for the same reason). This publish was done locally with dev credentials; future v0.x.y patches can either continue the same manual path or gain CI auto-publish once repo secrets are set. If you need the exact CI workflow that expects secrets, see `.github/workflows/smix-sdk-{cargo,npm,swift,android}-release.yml`.

---

## 1. Install for insight — three paths

### 1.1 Recommended: `cargo install smix-cli` (fastest)

```bash
cargo install smix-cli --locked --version 0.2.0
which smix
smix --version
# expected: smix 0.2.0
```

This drops `smix` into `~/.cargo/bin/smix`. Works on any machine with Rust toolchain. Downside vs the git-clone install: no shipped-runner project, so `smix runner up` will fall through the resolver cascade to `<cwd>/swift-bridge/` (see §1.4).

### 1.2 Recommended for the runner path: git-clone + `install-local.sh`

Includes the runner project at `~/.local/share/smix/runner/SmixRunner.xcodeproj`, which is what makes `smix runner up` work from ANY cwd. This is what insight needs for the perf/visual gates.

```bash
git clone git@github.com:goliajp/smix.git
cd smix
git checkout smix-v0.2.0
bash scripts/install-local.sh
smix --version
# expected: smix 0.2.0
ls ~/.local/share/smix/runner/SmixRunner.xcodeproj
# expected: directory exists (this is the §1 gap fix)
```

### 1.3 npm SDK dependency (if you're writing tests in TypeScript)

If your test author path is TypeScript rather than yaml, add the SDK:

```bash
cd /Users/doracawl/workspace/qualcomm/insight
bun add @goliapkg/smix@0.2.0
```

Then in a test:

```ts
import { Smix, App } from '@goliapkg/smix'
// ...
```

Not blocking for Path B (yaml flows go through `smix run` binary), but relevant if insight wants to write native TS glue in the future.

### 1.4 Which install for what:

- **Path B for insight (yaml + `bun verify perf/visual`)**: §1.2. You need the runner project.
- **CI / cloud runner**: §1.1 in Dockerfile, add a step to git-clone smix and copy `swift-bridge/` to `~/.local/share/smix/runner/` OR pass `--runner-project` explicitly per invocation.
- **Native TS tests**: §1.3 in addition to §1.1 or §1.2.
- **Kotlin (Android side)**: `implementation("jp.golia.smix:smix-sdk:0.2.0")` in your `build.gradle.kts` (available after Maven Central propagation window).
- **Swift Package**: point Package.swift at `https://github.com/goliajp/smix.git`, exact `swift-v0.2.0`. The XCFramework is fetched from the GitHub Release assets.

---

## 2. Verify the install (60 seconds)

```bash
# All four should exist + produce expected output.
smix --version | grep -q '0.2.0'    && echo "✅ smix 0.2.0"
smix run --help | grep -q '\-\-env' && echo "✅ --env flag present (gol-611 §2)"
smix sim locale --help >/dev/null 2>&1 && echo "✅ sim locale subcommand (gol-611 §5)"
smix runner up --help | grep -q '\-\-runner-project' && echo "✅ --runner-project flag (gol-611 §1)"
```

All four `✅` → the install carries all v0.2.0 gap fixes.

Then, optionally, the full end-to-end acceptance:

```bash
cd path/to/smix        # if you have the git-clone install
bash scripts/gol-611-verify.sh sim-insight
# expected: PASS: 8   FAIL: 0   SKIP: 0
```

(The verify script needs a booted sim; it exercises all 5 gap fixes against real runner + real sim.)

---

## 3. Apply the Path B code swap

Unchanged from `insight-v0.2.0-shipped.md` §3. Patch bundle at `docs/ai-guide/patches/insight-path-b-v0.2.0.patch`. Three files:

- New file `.devtools/verify/smix-env.ts`
- Modify `.devtools/verify/stages/perf.ts` — 2 hunks
- Modify `.devtools/verify/stages/visual.ts` — 2 hunks

Post-apply type check:

```bash
cd /Users/doracawl/workspace/qualcomm/insight
bunx tsc --noEmit .devtools/verify/stages/perf.ts \
                  .devtools/verify/stages/visual.ts \
                  .devtools/verify/smix-env.ts
```

Expected: exit 0, no errors on these three files.

If the edits keep reverting (like they did during the smix-side v0.2.0 landing attempt), apply them manually via editor rather than through automation. `.husky/pre-commit` runs `bun verify-scenario pre-commit` but doesn't touch these files.

---

## 4. Run the gates

Prereqs stay the same as maestro-era:
- Sim `sim-insight` booted
- Metro alive on 8081 (`bun dev`)
- Insight app installed on the sim

```bash
cd /Users/doracawl/workspace/qualcomm/insight
smix runner up sim-insight    # ~30s cold, ~2s hot; per session

bun verify perf
# smix side verified: status: ok, 107 samples, 0 regressions

bun verify visual
# smix side didn't run this end-to-end; expected to work given the
# code shape is identical to perf.ts's smix path.
```

For baseline promotion:
```bash
bun verify perf --accept-perf
bun verify visual --accept-visual
```

---

## 5. What each flag on `smix run` does

Copy-paste reference for `stages/perf.ts` / `stages/visual.ts` maintainers.

```bash
smix run <flow.yaml> \
  --device <DEVICE>            # sims.json alias OR raw UDID (required)
  --no-launch                  # skip foreground call (pair with sim launch --child-env)
  --env KEY=VAL                # repeatable; yaml ${VAR} interpolation
  --debug-output <DIR>         # <dir>/step-<N>-<verb>.json + run-summary.json + fail PNGs
  --format <human|json>        # default human; json → stdout JSON at exit
  --verbose                    # SMIX_LOG=debug for this run
  --platform ios|android       # default ios
  --runner-port <PORT>         # default 22087 iOS, 28080 Android
  --bundle-id <BUNDLE>         # default read from yaml header
  --apps-config <path>         # cross-platform app: resolver
```

`stages/perf.ts` builds the invocation as:

```ts
const smixArgs = [
  'run', FLOW,
  '--platform=' + platform,
  '--no-launch',
  '--debug-output', debugOut,
  '--format', 'json',
]
if (simUdid) smixArgs.push('--device', simUdid)
smixArgs.push(...envFlags)  // loadSmixEnvFlags() → ['--env', 'E2E_EMAIL=...', '--env', 'E2E_PASSWORD=...', ...]
```

Full example in `patches/insight-path-b-v0.2.0.patch`.

---

## 6. Rollback (if v0.2.0 misbehaves)

Insight-side:
```bash
cd /Users/doracawl/workspace/qualcomm/insight
git checkout -- .devtools/verify/stages/{perf,visual}.ts .devtools/verify/maestro-env.ts
rm .devtools/verify/smix-env.ts
# Back on maestro, no other file touched.
```

smix binary rollback:
```bash
# If you installed via git clone:
cd path/to/smix
git checkout smix-v0.1.0    # if this tag existed (v0.1.0 baseline)
bash scripts/install-local.sh

# If you installed via cargo:
cargo install smix-cli --locked --version 0.1.0 --force
```

The insight-side patch and the smix binary version are decoupled — you can roll one back without the other.

---

## 7. Reporting back

Feedback protocol unchanged from v0.2.0 shipped doc §9:
- Insight side: append to `.claude/state/gol-611/smix-feedback-path-b-attempt.md`, or start `smix-feedback-v020-adoption.md`.
- smix side: reply lands in `docs/ai-guide/…-response.md`.

Include if reporting a gap:
1. `smix --version`
2. Install path (§1.1 / §1.2 / §1.3)
3. Exact command run + stderr/stdout
4. Which sim (name or UDID)
5. Metro state (`curl -sf http://127.0.0.1:8081/status`)

---

## 8. Next milestone: v0.2.5 preview

`docs/plan-hot.md` covers the three v0.2.5 items directly useful for insight Path A:

- **§D batch invocation** — `smix run flow-a.yaml flow-b.yaml flow-c.yaml` reuses one runner across N flows
- **§E migrate codemod** — `smix migrate --in-place .devtools/maestro/flows/**/*.yaml`
- **§I concurrency ports** — `.smix/sims.json` optional `runnerPort` field + pool allocation

Target date: +2 weeks from 2026-07-08.

Full roadmap through v1.0.0: `docs/ai-guide/insight-roadmap.md`.

---

## 9. Quick reference (updated 2026-07-08)

| resource | location |
|---|---|
| smix repo | `github.com/goliajp/smix` @ `smix-v0.2.0` |
| Binary (this machine) | `~/.local/bin/smix` (`smix --version` → `smix 0.2.0`) |
| Shipped runner (this machine) | `~/.local/share/smix/runner/SmixRunner.xcodeproj` |
| crates.io CLI | `cargo install smix-cli --version 0.2.0` |
| npm SDK | `bun add @goliapkg/smix@0.2.0` |
| Maven Central | `jp.golia.smix:smix-sdk:0.2.0` (post-propagation) |
| Swift Package | `swift-v0.2.0` tag on `goliajp/smix` |
| Integration guide | `smix/docs/ai-guide/insight-integration-guide.md` |
| Path B patch bundle | `smix/docs/ai-guide/patches/insight-path-b-v0.2.0.patch` |
| gol-611 response | `smix/docs/ai-guide/gol-611-path-b-response.md` |
| Insight roadmap | `smix/docs/ai-guide/insight-roadmap.md` |
| JSON schema | `smix/docs/ai-guide/schemas/run-report.json` |
| CHANGELOG | `smix/CHANGELOG.md` (v0.2.0 section) |
| Acceptance script | `smix/scripts/gol-611-verify.sh` |
| Probe yamls | `smix/tests/fixtures/gol-611-probes/` |
| plan-hot (v0.2.5) | `smix/docs/plan-hot.md` |
| plan-history | `smix/docs/plan-history/v0.2.0-gol-611-hot.md` |

If any URL / coordinate above is stale (e.g. crates.io still says 0.1.0), that's a propagation lag — most resolve within an hour of the publish timestamp above.
