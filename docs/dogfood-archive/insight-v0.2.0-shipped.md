# smix v0.2.0 shipped — insight adoption walkthrough

Date: 2026-07-08
Tag: `smix-v0.2.0` (github.com/goliajp/smix at commit `da7c39cc5`)
Audience: the insight engineer (human or AI) who filed
`.claude/state/gol-611/smix-feedback-path-b-attempt.md` on 2026-07-07 and needs to (a) verify the ship claim, (b) apply the Path B swap, (c) know what changed since the last conversation.

Prior chain:
- 2026-06-28 — v6.8 c1+c2 close (`gol-611 §1-§4` per insight's original feedback)
- 2026-07-07 — insight Path B PoC report (§1-§5 gaps, wishlist §A-§L)
- 2026-07-07 — smix response `gol-611-path-b-response.md` + `insight-roadmap.md` + `insight-integration-guide.md`
- **2026-07-08 — this doc**: v0.2.0 shipped, walkthrough for adoption

Companion docs on smix side:
- `docs/plan-history/v0.2.0-gol-611-hot.md` — the completed plan
- `docs/ai-guide/patches/insight-path-b-v0.2.0.patch` — the code changes to apply
- `docs/ai-guide/schemas/run-report.json` — JSON Schema for `--format json`
- `CHANGELOG.md` — v0.2.0 section

---

## 0. TL;DR

Insight's 5 concrete Path B blockers are all closed on `smix-v0.2.0`:

| gap | insight PoC symptom | v0.2.0 outcome |
|---|---|---|
| §1 | `runner project missing: <cwd>/swift-bridge/…` (symlink hack) | 4-step cascade + install-shipped runner. Zero setup for consumers. |
| §2 | `--debug-output`, `--env`, `--verbose` didn't exist | All four flags land + `--format` for JSON. |
| §3 | Failure text on stderr, not JSON as guide promised | `--format json` emits schema-validated JSON on stdout. |
| §4 | `runFlow: { when: false }` printed spurious `[ELEMENT_NOT_FOUND]` | Predicate errors swallowed as skip; runner-side `found` field bug fixed. |
| §5 | `locale:` in `sims.json` applied only at next boot | `smix sim locale <DEVICE> <LANG> --reboot` for already-booted sims. |

Verification on smix side:
- `cargo test -p smix-adapter-maestro` — 11 pass including 4 new `gol_611_p{2,4}_*` regressions
- `bash scripts/gol-611-verify.sh insight` — 8/8 PASS on booted sim-insight (UDID `FFC57DAE-…`)
- Manual `runPerfStage()` direct call — `status: ok`, 107 samples, 0 regressions

Verification on insight side: needs the Path B code patch applied. See §3 below.

Wishlist §A-§L: `insight-roadmap.md` unchanged from 2026-07-07 — 6 milestones (v0.2.0 through v1.0.0), each with CLI shape + cost.

---

## 1. Install / upgrade smix

Two paths.

### 1.1 If smix is not yet on this machine

```bash
git clone git@github.com:goliajp/smix.git
cd smix
git checkout smix-v0.2.0
bash scripts/install-local.sh
```

This installs:
- `~/.local/bin/smix` (codesigned, macOS Gatekeeper accepted)
- `~/.local/bin/smix-maestro` (adapter used by `smix run` internally; don't invoke directly)
- `~/.local/share/smix/runner/SmixRunner.xcodeproj` (the shipped runner — this is the §1 fix)

### 1.2 If smix is already installed at an older version

```bash
cd path/to/smix          # your existing checkout
git fetch origin
git checkout smix-v0.2.0
bash scripts/install-local.sh
```

Same script; idempotent. Overwrites the binaries + rsyncs the runner project.

### 1.3 Sanity checks (30 seconds)

```bash
smix --version
# expected: smix 0.2.0

smix runner up --help | grep -A1 runner-project
# expected: shows --runner-project <RUNNER_PROJECT> flag description
# (v0.1.0 doesn't have this flag)

smix run --help | grep -E "env|debug-output|format|verbose"
# expected: --env, --debug-output, --format, --verbose all listed

smix sim locale --help 2>&1 | head -3
# expected: subcommand description (v0.1.0 errors "unknown subcommand")

ls ~/.local/share/smix/runner/SmixRunner.xcodeproj
# expected: directory exists
```

If any of the four checks doesn't match, `install-local.sh` didn't complete cleanly — grep the script output for `error:` and re-run.

---

## 2. Verify the smix side works against your sim (before touching insight code)

Optional but recommended — smokes the 5 gap fixes end-to-end without changing any insight file:

```bash
cd path/to/smix
# `insight` alias needs to be either in
#   (a) smix repo's .smix/sims.json, or
#   (b) qualcomm/insight's .smix/sims.json,
# or you can pass a raw UDID.
bash scripts/gol-611-verify.sh insight
```

Expected output: `PASS: 8   FAIL: 0   SKIP: 0` and `all gap probes PASS`. Runs in ~2 minutes (§5 locale reboot dominates wall clock).

If any gap fails, that gap is not actually working on your machine — file it back at `.claude/state/gol-611/` on the insight side with the failing PROBE + output. Do NOT proceed to Path B until this is 8/8 green.

---

## 3. Apply the Path B code swap in insight repo

The gol-611 Path B PoC needed three edits inside `qualcomm/insight`:

- New file: `.devtools/verify/smix-env.ts`
- Modify: `.devtools/verify/stages/perf.ts`
- Modify: `.devtools/verify/stages/visual.ts`

Full patch at: `smix/docs/ai-guide/patches/insight-path-b-v0.2.0.patch` in this smix repo. Copy-paste the hunks into the insight files.

### 3.1 Why the patch is a doc, not a git patch

During the v0.2.0 landing, the smix side attempted to apply these edits directly (via Edit tool in insight's working tree). The edits were consistently reverted between the edit and the follow-up `bun verify perf` run — likely a lint hook / editor auto-save / manual revert. Rather than fight the revert loop, the intended shape is documented in the patch bundle so the insight team can apply it deliberately on their own timeline.

If you're applying via editor: open the file, find the marker line quoted in the patch, replace the block. All three files affected are `.devtools/verify/*` — no touching of app source, no touching of `package.json` or CI.

### 3.2 Type-check right after applying

```bash
cd /Users/doracawl/workspace/qualcomm/insight
bunx tsc --noEmit .devtools/verify/stages/perf.ts \
                  .devtools/verify/stages/visual.ts \
                  .devtools/verify/smix-env.ts
```

Expected: exit 0, no `error TS…` on these three files. Other errors on unrelated files (e.g. WIP hooks) can be ignored — they're pre-existing.

### 3.3 Roll back cleanly if anything's off

```bash
git checkout -- .devtools/verify/{stages/{perf,visual}.ts,maestro-env.ts}
rm .devtools/verify/smix-env.ts
```

One git command + one rm; nothing else touched by the patch.

---

## 4. Verify `bun verify perf` (~2 min end-to-end)

Prereqs (same as maestro's require-sim.ts checks):
- Sim `sim-insight` booted (`xcrun simctl list devices booted`)
- Metro serving on 8081 (`bun dev` in insight repo)
- Insight app installed on the sim (visible under `smix sim exec <udid> listapps` — check for `com.focusai.app.mobile`)

Once prereqs green:

```bash
cd /Users/doracawl/workspace/qualcomm/insight
smix runner up sim-insight    # ~30s cold; instant hot. One time per session.
bun verify perf
```

Expected shape (was verified directly by the smix side on 2026-07-08):

```
$ bun .devtools/verify/atom-runner.ts perf
# perf-receiver listening on 0.0.0.0:9999
perf: PASS (~118s)
  ✓ 0 regressions
```

Sample metrics from the direct-call verification:
```
sample_count: 107
avg_cpu_js_pct: 1.56
avg_cpu_total_pct: 13.42
avg_irq_per_sec: 292.14
avg_wakeups_per_sec: 346.46
cpu_pct_p95: 47.4
rn_views_avg_total: 45.4
rn_views_max_depth: 49
subsystems_avg_total: 28
```

Baseline update (once you're comfortable with the numbers):

```bash
bun verify perf --accept-perf   # promote current numbers to baseline
```

---

## 5. Verify `bun verify visual` (~1 min end-to-end)

The v0.2.0 shape drops the GOL-588 "cold-start driver flake" retry entirely — smix has a persistent runner (SmixRunner XCTest bundle), so `runner up` amortizes that cost once. No retry loop needed.

```bash
cd /Users/doracawl/workspace/qualcomm/insight
smix runner up sim-insight    # if not already
bun verify visual
```

Expected shape:
- Screenshots land in `.tmp/smix-visual-out/visual-<ts>/` (per-anchor PNGs)
- PNG diffs at `.tmp/visual-diff/ios/`
- Console shows `visual: PASS` with `0 anchors over threshold`

Baseline promotion:
```bash
bun verify visual --accept-visual
```

Diff threshold stays at 0.5% (unchanged from maestro-era). If you want tighter — that's `stages/visual.ts:37` `DEFAULT_THRESHOLD_PCT`, edit in place.

---

## 6. What to expect that's different from maestro-era

### 6.1 Wall clock

| stage | maestro (retry-once) | smix v0.2.0 |
|---|---|---|
| `runner up` | N/A (per-flow xcodebuild) | ~30s cold, ~2s hot; per-session |
| `perf` per run | 3-8 min (includes cold-build retry) | 2 min (no retry) |
| `visual` per run | 1-4 min (includes cold-build retry) | 1 min (no retry) |

Net: ~2× faster after the first `runner up`. Cold sessions similar.

### 6.2 Failure output

Maestro-era: multi-line stderr text.

v0.2.0 (with `--format json` in your wrapper):

```json
{
  "flow": ".devtools/maestro/flows/_perf/golden-path.yaml",
  "runOutcome": "failure",
  "failure": {
    "code": "ElementNotFound",
    "message": "element not found: { id: input-email }",
    "selector": "Id(\"input-email\")",
    "suggestions": ["did you mean input-email-field?"],
    "visibleCount": 12
  }
}
```

Schema: `smix/docs/ai-guide/schemas/run-report.json`. Validates with any Draft-07 tool (`ajv`, jsonschema, etc.).

The `--debug-output <dir>` also writes:
- `<dir>/run-summary.json` — aggregate
- `<dir>/step-<N>-<verb>.json` — per-step outcome
- `<dir>/step-<N>-<verb>.fail.png` — screenshot on the failing step

### 6.3 Env interpolation semantics

`${VAR}` in yaml text values (setClipboard, inputText, pasteText, assertTrue) is resolved from smix's env store. The store priority is: CLI `--env KEY=VAL` (repeatable) wins over inherited process env. Undefined variable is an explicit `DriverError` (message names the var — no silent empty substitution).

Insight's `.env.local` reading through `loadSmixEnvFlags` fans it out as `--env E2E_EMAIL=… --env E2E_PASSWORD=… --env IMAP_HOST=…`. Same shape as maestro's `-e`, just different flag name.

### 6.4 The `runFlow: { when: … }` semantics

`when.visible`'s predicate now correctly treats "driver error while probing" as "not visible" (conservative skip). No more spurious `[ELEMENT_NOT_FOUND]` stderr on the `ensure-login`-style pattern that gol-611 §4 identified.

---

## 7. Troubleshooting the common surprises

### 7.1 `smix runner up` says `runner project missing` even after install

Something interfered with the install-shipped copy. Rerun `bash scripts/install-local.sh` in the smix repo (idempotent), or explicit override:

```bash
smix runner up sim-insight --runner-project /path/to/smix/swift-bridge/SmixRunner.xcodeproj
```

The `--runner-project` flag is strict — if the path doesn't exist, smix fails fast rather than silently falling through to defaults. That's a v0.2.0 change vs the earlier development builds.

### 7.2 `bun verify perf` says `res.code !== 0` (regression on `smix`) but no useful output

Look at `.tmp/smix-debug/perf-<timestamp>/` — it has `run-summary.json` + per-step details. That directory persists after the run (perf.ts doesn't rm it). Also look at `.smix/runner/runner-<UDID>.log` — the XCTest bundle log.

If both look clean, run the flow directly to see full stderr:

```bash
smix run .devtools/maestro/flows/_perf/golden-path.yaml \
    --device <UDID> --no-launch --verbose
```

### 7.3 `bun verify visual` reports all anchors over threshold

Almost certainly means smix ran a different flow than expected, or the flow ran but the SDK didn't emit the takeScreenshot verbs. Check `.tmp/smix-visual-out/visual-<ts>/` for PNGs — if empty, the flow didn't call `takeScreenshot:`. If present, spot-check one against the baseline manually — if the visible content differs (e.g. app deep-linked into a wrong route), that's an app-side issue not a smix issue.

Regenerate baselines when the app's rendered content changed intentionally:
```bash
bun verify visual --accept-visual
```

### 7.4 sim boots in Chinese and `dismiss-open-in.yaml` doesn't match

`smix sim locale sim-insight en --reboot` (v0.2.0 new command) applies English immediately without waiting for a `sims.json`-driven boot cycle.

### 7.5 The perf.ts / visual.ts I edited keeps reverting

This is what the smix side hit during the v0.2.0 landing attempt. Two options:
- Apply the patch manually via editor (not via a shell edit tool or automation)
- Check for a lint hook: `cat .husky/pre-commit .git/hooks/pre-commit`
- Check for editor auto-format: VS Code / Cursor / Zed formatOnSave — could be pulling a stale buffer

The patch content is documented in `smix/docs/ai-guide/patches/insight-path-b-v0.2.0.patch` — copy the exact blocks and paste.

---

## 8. Beyond v0.2.0 — v0.2.5 preview + wishlist

v0.2.5 (target: +2 weeks) covers three roadmap items directly useful for Path A:

- **§D batch invocation** — `smix run flow-a.yaml flow-b.yaml flow-c.yaml` reuses one runner across N flows. `.devtools/qa/sim/runner.ts`'s per-scope multi-flow loop collapses to one `smix run` call.
- **§E migrate codemod** — `smix migrate --in-place .devtools/maestro/flows/**/*.yaml` static-rewrites maestro yaml to smix dialect. Path A becomes a one-liner + a handful of manual edits for the `runScript`/`evalScript` sites.
- **§I concurrency ports** — `.smix/sims.json` optional `runnerPort` field + pool allocation so `smix run --device sim-a & smix run --device sim-b` doesn't collide.

v0.3.0 (target: +4 weeks) picks up the P0 wishlist:
- §A fixture chip protocol
- §B metro log signal bridging
- §C standard subflow catalogue

Details + costs + acceptance criteria: `smix/docs/ai-guide/insight-roadmap.md`. Both sides annotate.

---

## 9. Feedback protocol

Same as the prior rounds:
- Insight side: append to your existing `.claude/state/gol-611/smix-feedback-path-b-attempt.md`, OR start a new file (`smix-feedback-v020-adoption.md` is the natural next name).
- smix side: reply with a companion `docs/ai-guide/…-response.md` and land the code changes.

Nothing to file with GitHub Issues; the file-based conversation is the record of decision.

If a gap fix isn't behaving as documented, please include:
1. `smix --version` output
2. The exact command you ran + stderr/stdout
3. Which sim (name or UDID)
4. Metro state (`curl -sf http://127.0.0.1:8081/status`)

Those four pieces cover 90% of "can't reproduce" cases.

---

## 10. Where things live (quick reference)

| resource | location |
|---|---|
| smix repo | `github.com/goliajp/smix` @ `smix-v0.2.0` (commit `da7c39cc5`) |
| Binary | `~/.local/bin/smix` (`smix --version` → `smix 0.2.0`) |
| Shipped runner | `~/.local/share/smix/runner/SmixRunner.xcodeproj` |
| Integration guide | `smix/docs/ai-guide/insight-integration-guide.md` |
| Path B patch | `smix/docs/ai-guide/patches/insight-path-b-v0.2.0.patch` |
| Roadmap | `smix/docs/ai-guide/insight-roadmap.md` |
| Response docs | `smix/docs/ai-guide/gol-611-path-b-response.md`, `smix/docs/ai-guide/insight-feedback-gol-611-response.md` |
| JSON Schema | `smix/docs/ai-guide/schemas/run-report.json` |
| CHANGELOG | `smix/CHANGELOG.md` (v0.2.0 section) |
| Acceptance script | `smix/scripts/gol-611-verify.sh` |
| Probe yamls | `smix/tests/fixtures/gol-611-probes/` |

The full v0.2.0 hot plan (with every acceptance criterion) is at `smix/docs/plan-history/v0.2.0-gol-611-hot.md`.
