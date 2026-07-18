# Insight × smix capability roadmap

Date: 2026-07-07
Companion to: `gol-611-path-b-response.md` (which handles the 5 concrete Path B blockers). This doc handles the wishlist §A-L in the same feedback.

Audience: insight team + smix maintainers. Both sides annotate; the doc is a joint design surface.

Legend for status:
- **✅ committed** — building this milestone; acceptance criteria fixed
- **🎯 shaped** — design shape agreed, sequencing TBD
- **🔬 explored** — needs shared design conversation before commitment
- **⏸ deferred** — will not build without a new signal

Cost columns are rough: `S` = < 1 week smix-side, `M` = 1-2 weeks, `L` = 2-4 weeks. Insight-side integration cost is separate.

---

## Milestone map

| Milestone | Target date | Includes | Unlocks for insight |
|---|---|---|---|
| **v0.2.0 "external-consumer readiness"** | this cycle | §1-5 gap fixes from response doc | Path B ships. `stages/perf.ts` + `stages/visual.ts` migrated. |
| **v0.2.5 "batch + migrate"** | +2 wks | §E migrate codemod + §D batch invocation + §I concurrency wiring | Path A becomes a codemod pass. Insight can `find . -name '*.yaml' \| xargs smix migrate --in-place`. |
| **v0.3.0 "signal-driven testing"** | +4 wks | §A fixture chip + §B metro log signal + §C standard subflows | `.devtools/qa/sim/runner.ts` (the maestro-based qa scope runner) retires entirely. |
| **v0.4.0 "author leverage"** | +8 wks | §E part 2 (session recording) + §G selector auto-suggest + §F semantic a11y diff | Test authoring speed 2-3× faster. Visual gate acceptance rate up. |
| **v0.5.0 "control + coverage"** | +12 wks | §H native control channel + §J coverage report + §K deterministic time | Sim-test as design leverage. Insight ships coverage next to code coverage. |
| **v1.0.0 "long-tail infra"** | +TBD | §L replay bundle | "Cannot reproduce" excuse retired. |

Deferrable at any point without breaking downstream items — each row consumes contracts fixed by prior rows but doesn't require the current row's optional pieces.

---

## §A — Fixture chip protocol (P0, milestone v0.3.0, cost M)

Status: **🎯 shaped**

### Public contract

```yaml
# yaml surface
- fixture: prime-search-history

# or explicit form
- fixture:
    id: prime-search-history
    timeoutMs: 8000
```

Semantics: smix opens `qa-bubble` overlay, taps the chip whose `testID` matches `fixture.id`, awaits the chip's designed log signal (see §B — the signal comes from the app-side declaration), then returns. Idempotent — if the chip is already in "fired" state (per its declared log signal already present in the recent window), returns fast.

### App-side contract

Every fixture chip in the QA panel declares itself in a well-known TypeScript module (in insight: `.devtools/qa/fixtures/registry.ts`). Shape:

```ts
export const FIXTURES: Record<FixtureId, FixtureDecl> = {
  'prime-search-history': {
    testID: 'qa-chip-prime-search-history',
    signal: { level: 'log', regex: /\[fixture\] prime-search-history: seeded (\d+) rows/ },
    timeoutMs: 8000,
  },
  ...
}
```

smix reads this registry at run time (path via `.smix/config.json` `fixturesRegistry` field), so the yaml only names the fixture — no testID, no signal regex to duplicate. When insight adds a chip, both sides get it for free.

### CLI equivalent

```
smix fixture insight prime-search-history [--timeout 8000]
```

Same semantics as the yaml verb, for ad-hoc debugging.

### Insight integration

The 12 `.devtools/qa/sim/flows/*/*.yaml` scope flows currently open `qa-bubble` → wait → tap chip → wait for signal (3-6 lines each = 36-72 lines of duplication). Post-migration, each becomes `- fixture: <id>` (one line). Estimated saving: ~250 yaml lines across the qa corpus.

### Cost

- smix side: M. New verb parser, new runtime dispatch, new registry loader (read one JSON/TS module at run time), plumbing into signal await (§B). Design agreed; implementation is ~200 LOC parser + ~150 LOC runtime + tests.
- insight side: write `.devtools/qa/fixtures/registry.ts` (2h — content mostly exists as informal per-scope allowlist entries), migrate 12 yamls (1h with codemod support from §E).

### Milestone: v0.3.0. Depends on §B (signal await).

---

## §B — Metro log signal bridging (P0, milestone v0.3.0, cost M)

Status: **🎯 shaped**

The single most productive capability per insight's ranking. This is smix observing the app-under-test's designed log signals so yaml can assert on state transitions, not just UI shape.

### Public contract

Two yaml verbs:

**Presence** — signal appears in a window:
```yaml
- expect:
    signal:
      regex: "env=qa-mode"          # matches metro log line
      timeoutMs: 8000               # wait up to 8s for it to appear
      window:                       # optional; default = since previous expect:signal
        sinceStep: 2                # from end of step 2
```

**Ordering** — signals in exact order:
```yaml
- expect:
    signals:
      - regex: "launchOverrideConsumed\\('force-update'\\)"
      - regex: "autoLoginValidated"
      - regex: "readyForInteractive"
    order: strict                   # exactly this sequence, no interleaving
    timeoutMs: 30000
    window:
      sinceRun: 0                   # entire run
```

### CLI equivalent

```
smix run <flow> --await-signal 'env=qa-mode' --timeout 8000
```

For one-off scripting; the yaml form is the primary surface.

### Log source model

smix owns the metro log tail. At `smix runner up` (or `sim launch`), it inherits a `metroLog:` config field from `.smix/config.json`:

```json
{
  "metroLog": {
    "url": "ws://127.0.0.1:8081/logs",
    "or_file": "~/Library/Logs/expo/metro.log",
    "allowlist": [ "^skipping register: must use physical device", "^bundle scheme is file - unable to" ]
  }
}
```

smix keeps a ring buffer of the last N seconds (default 300 s) so `expect.signal.window` can look backward. Consumer specifies `allowlist` to gate a *log-hygiene assertion* (bare `--expect-log-clean`, verified after flow completion — replaces insight's `logGate`).

### Ordering — the differentiator

Both "presence" and "ordering" ship in v0.3.0. Ordering is the differentiator vs any UI-only runner: it directly maps to insight's temporal-contract "event ledger" pattern (`.claude/business/qa/infra.md` §4.1). No other RN-testing tool exposes this.

### Insight integration

`.devtools/qa/sim/runner.ts`'s `logGate` (the metro tail parser + allowlist) retires. Each scope's `signals:` array in the qa registry becomes an `expect.signals:` step in the corresponding yaml.

### Cost

- smix side: M. New WebSocket subscriber, new ring buffer, new yaml verb + parser + runtime, allowlist matcher. ~400 LOC total. Design fixed; test surface known (present/absent × in-order/out-of-order × in-window/out-of-window = 8 cases).
- insight side: S. Delete `logGate` (~100 LOC), add signals to yaml (~50 lines across 12 scope files).

### Milestone: v0.3.0. Independent of §A but co-develops (both hit the "smix knows what the app is emitting" surface).

---

## §C — Standard subflow catalogue (P1, milestone v0.3.0, cost S)

Status: **✅ committed** (for the platform-general subset)

Two primitives ship as `std/`:

```yaml
- runFlow: std/dismiss-open-in.yaml    # iOS 26 SpringBoard dialog dismiss
- runFlow: std/ensure-locale.yaml      # ensure app locale matches sims.json locale
```

Insight's `ensure-login.yaml` and `enter-qa-mode.yaml` do NOT ship as `std/` — they're app-specific. `dismiss-open-in.yaml` and locale-ensure are platform-general (iOS 26 dialog is Apple's, not insight's).

Version pinning: `runFlow: std/dismiss-open-in@1.0/dismiss.yaml` when semver matters. First shipped versions carry `@1.0`; deprecations bump minor.

### Cost

- smix side: S. Bundle 2 yaml files in the smix release, add resolver logic to yaml parser that treats `std/<path>` as coming from the shipped directory (like an implicit alias). ~50 LOC.
- insight side: S. Delete forked copies of `dismiss-open-in.yaml`, update references. ~30 lines diff.

### Milestone: v0.3.0. Independent.

---

## §D — Batch invocation (P1, milestone v0.2.5, cost S)

Status: **✅ committed**

### Shape

```
smix run <device> flow-a.yaml flow-b.yaml flow-c.yaml
# or
smix run <device> --batch flows.txt
```

- One `runner up` reused across all N flows.
- Per-flow report accumulated into a single `run-summary.json` (matches `--debug-output` shape when set).
- `--fail-fast` optional; default continues on flow failure.
- Exit code = max of per-flow exit codes.

### Cost

- smix side: S. Loop wrapper around `smix_adapter_maestro::run_flow`, aggregate report. ~100 LOC.
- insight side: S. `runCmd(SMIX, ['run', 'insight', ...flows])` in `scenario-runner.ts`.

### Milestone: v0.2.5.

---

## §E — Session recording + `smix migrate` codemod (P1, milestone v0.2.5 + v0.4.0, cost part 1 = S, part 2 = M)

Status: **✅ committed** (migrate) / **🎯 shaped** (record)

### Part 1 — `smix migrate` codemod (v0.2.5)

```
smix migrate maestro-flow.yaml > smix-flow.yaml
smix migrate --in-place flows/*.yaml
```

Static parse of maestro yaml → rewrite via smix parser's yaml emitter. Verb renames (`tapOn` → `tap`, `extendedWaitUntil` → `expect timeoutMs`, `retry.max` → `retry.maxRetries`), field shape adjustments, no semantic changes. Un-migratable steps (`runScript`, `evalScript`) emit a `WARN` and pass through as-is; user decides.

Cheap because both parsers already exist in `smix-adapter-maestro`. ~250 LOC + tests. 1-2 days.

### Part 2 — `smix record` (v0.4.0)

```
smix record --device insight --output tap-through.yaml
# tester drives sim by hand for a minute
# ^C or `smix record stop` finalizes yaml
```

smix records taps / fills / screen transitions from the runner's event stream, chooses selectors (id > text > coord fallback), emits yaml.

Selector-choice heuristic: prefer id if unique in the a11y tree; else text; else coordinate with a `TODO: replace with semantic selector` comment. First-cut yaml from 30 s of clicking; 5 min from author to committable.

Cost: M. Needs an event-recording extension to SmixRunner XCTest bundle (~200 LOC swift) + yaml emitter with selector-choice heuristic (~300 LOC rust) + tests.

### Insight integration

Migrate is one shell one-liner per corpus. Record is a new author workflow — insight `bun verify:record <flow>` binds to it.

### Milestones: v0.2.5 (migrate) + v0.4.0 (record).

---

## §F — Semantic a11y-tree diff for visual gate (P2, milestone v0.4.0, cost M)

Status: **🎯 shaped**

Insight's PNG diff is fragile under theme rotation. Ship a parallel structural diff:

```yaml
- takeScreenshot: hub-form
- takeA11ySnapshot: hub-form           # new verb; writes .a11y.json next to hub-form.png
```

Gate:
```yaml
gates:
  - visual: threshold: 0.5%             # existing PNG diff
  - a11y: strict                        # new; any structural change fails
```

Baseline: `.devtools/test-baselines/visual/ios/hub-form.a11y.json`. Diff ignores stable-identifier keys (`frame`, `size`, transient `hasFocus`); gates on added/removed nodes, changed `accessibilityLabel`, `role`, `id`.

Both diffs coexist. PNG = "looks right", a11y = "structure right". PNG can flake without a11y noticing; a11y catches testID drift regardless of theme.

### Cost

- smix side: M. New verb, new snapshot serializer (subset of `A11yNode` deemed stable), diff algorithm with ignore-list. ~500 LOC. Reuses `smix-error`'s existing suggestion-building for "closest structural match" text on diff.
- insight side: S. `stages/visual.ts` extended to iterate `.a11y.json` alongside `.png`. ~50 LOC.

### Milestone: v0.4.0. Depends on §J coverage manifest format (shared `A11yNode` subset serializer).

---

## §G — Selector auto-suggest at authoring time (P2, milestone v0.4.0, cost M)

Status: **🎯 shaped**

Extend the failure-side suggestion (already in `smix-error`) to authoring:

```
smix inspect insight --near 'id=input-emial'
```

Output:
```
Near-matches on current a11y tree (5):
  id="input-email"        edit distance 1   (testID last renamed 2026-06-15 in EmailField.tsx)
  id="input-email-field"  edit distance 6   semantically similar; used by AccountSetup form
  ...
```

Rename detection: git log for the file that defines the closest-match testID, look for `testID: "..."` string changes. Uses `git log -S` per candidate.

### Cost

- smix side: M. Reuse `smix-error/build_suggestions` for edit-distance. New git-history walker for rename detection (~200 LOC). New CLI subcommand + doc.
- insight side: S. Wrap in `bun verify:inspect` binding.

### Milestone: v0.4.0. Independent.

---

## §H — Native control channel (P2, milestone v0.5.0, cost M)

Status: **🔬 explored**

App exposes a dev-only HTTP endpoint (`http://127.0.0.1:22088/state`) with structured state. smix `expect: appState:` asserts against it.

```yaml
- expect:
    appState:
      key: "remotePulse.forcePush"
      value: true
```

App side: `apps/qa-server/index.ts` (in insight) starts a small HTTP server behind `Env.isDev || Env.isQaMode`, publishes a state dict from an internal registry. Contract: keys are dotted paths, values are JSON-serializable.

### Concerns worth exploring

- **Security boundary.** The endpoint listens on localhost, guarded by dev flag. Fine for dev builds. Not fine for staging builds. Contract needs a build-time guard confirmed at each shipping build.
- **State registration model.** Which internals expose to the channel? An opt-in registry (`registerState('remotePulse.forcePush', () => currentValue)`) is safer than reflect-everything.
- **Overlap with signal (§B).** Signals are event-emissions; state is queryable. They cover different questions. Both useful.

### Cost

- smix side: M. Yaml verb + parser + HTTP client + retry policy. ~300 LOC.
- insight side: M. App-side server + state registry. ~200 LOC + threading through existing dev-only paths.

### Milestone: v0.5.0. Wait until §B ships and we've learned from that pattern.

---

## §I — Concurrency across sims (P3, milestone v0.2.5, cost S)

Status: **✅ committed**

Two changes:

1. `.smix/sims.json` per-entry `runnerPort:` field (optional). If unset, auto-assigns from a pool (default range 22087-22095).
2. `smix runner up <device>` respects the assigned port. Multiple runners on multiple sims never collide.

Insight then runs `smix run insight-26-2 <flow> & smix run insight-26-5 <flow>` in parallel; each has its own runner state.

### Cost

- smix side: S. Extend `SimEntry` struct, add port allocator. ~80 LOC.
- insight side: minimal — write two `sims.json` entries.

### Milestone: v0.2.5. Ships alongside §D batch (both are "run more than one thing" primitives).

---

## §J — Flow coverage report (P3, milestone v0.5.0, cost M)

Status: **🎯 shaped**

Post-run summary of selectors touched vs available:

```
flow: golden-path.yaml
selectors touched: 8 unique
  - id: btn-open-menu   (1 tap)
  - text: "Log in to Insight" (2 waits)
  ...
selectors NOT touched (from .smix/coverage-manifest.json): 42
  - id: profile-additional-photo-*   (new in commit abc123)
  - text: "Search by face"
```

Coverage manifest: generated by insight's build tooling from the RN component tree — extract every `testID:` string, dump to `.smix/coverage-manifest.json`. smix ingests, diffs against touched selectors.

Cross-run aggregation: `smix coverage --input .smix/runs/*.json --manifest .smix/coverage-manifest.json` gives per-manifest-entry hit counts across all runs.

### Cost

- smix side: M. Selector tracking during run (already partial), manifest ingest, diff, summary format. ~300 LOC.
- insight side: M. Build tool that extracts `testID:` from RN source. eslint-rule-based extraction reuses `insight-flow/probe-id-defined`.

### Milestone: v0.5.0. Independent.

---

## §K — Deterministic time / animations mode (P3, milestone v0.5.0, cost S)

Status: **🔬 explored**

`smix run --stable` does:
- `xcrun simctl status_bar override --time 09:41 --batteryLevel 100 --wifiBars 3` before run
- Sends a control message to SmixRunner to disable animations (`UIView.setAnimationsEnabled(false)`) for the run's duration
- Sends env `SMIX_STABLE=1` via `sim launch --child-env`, app-side disables Metro reload during run

App-side integration point: insight's `apps/qa-server/index.ts` (§H) can also honor `SMIX_STABLE`. Alternatively, a bare env-var check in the RN root.

### Concerns

- Test authors need this consistently or not at all. Half-stable mode makes debugging weird.
- Interacts with §F (a11y diff): with stable mode on, both PNG and a11y should be highly reproducible.

### Cost

- smix side: S. Add `--stable` flag, three shell-outs to simctl. ~80 LOC.
- insight side: S. env-var honor. ~20 LOC.

### Milestone: v0.5.0. Coupled with §F for full effect.

---

## §L — Failure-driven replay bundle (P3, milestone v1.0.0, cost L)

Status: **⏸ deferred**

Every failed run bundles inputs (yaml + sims.json snapshot + metro log + a11y tree per step + PNGs) into a `.smix/replay/<flow>-<ts>.tar`. `smix replay <bundle> [--breakpoint step:12]` re-runs deterministically.

### Why deferred

Deterministic replay is hard because:
- App state depends on backend state at the moment of run (staging DB rows, remote config).
- Sim state depends on prior test residue (mmkv wipe helps but doesn't fully reset).
- Metro state depends on Metro cache.

A meaningful replay needs either a backend record/replay layer (`nock`-style HTTP capture) or a hermetic app build with baked fixtures. Both are large; both are outside smix's remit.

What smix CAN do cheaply is bundle inputs (§L bundle format) without promising deterministic replay. That reduces the "cannot reproduce" cost by giving the reproducer a starting kit, without the false promise of automated replay.

### Ship shape (if scoped to bundle-only, not replay-only)

- `.smix/replay/<flow>-<ts>.tar` on failure — bundle inputs + all step artifacts.
- `smix replay <bundle>` = `smix run` with the yaml + `.smix/sims.json` from the tar, in a temp workdir.

Cost drops from L to M with the scope trim.

### Milestone: v1.0.0. Reassess when §B + §H are in production for a quarter.

---

## Not-in-scope pipeline items

These came up in the feedback or adjacent conversation and are marked as intentionally not on the roadmap:

- **Cloud runner farms / hosted CI for insight**. Different product. Out of smix's scope.
- **VLM-based visual verification**. See CLAUDE.md §9 #2 — "不引入 multi-provider VLM 抽象".
- **Non-iOS platforms as first-class**. Android smix exists and is dogfood-quality; making it first-class needs a separate roadmap. See `docs/ai-guide/uiautomator-sim-runtime-impl-guide.md` for state of Android.

---

## Amendment protocol

Insight edits their side, smix edits ours, both sides annotate agreement/disagreement inline with `//INS:` and `//SMX:` prefixes. Every quarterly review, we reconcile.

Insight amendments welcome via appending to `.claude/state/gol-611/`; smix amendments land in `docs/ai-guide/`.
