# Insight × smix integration guide

Date: 2026-07-07
Author: smix side, based on `.claude/state/gol-611/simx-feedback.md` (insight) + `docs/ai-guide/insight-feedback-gol-611-response.md` (smix) + survey of `/Users/doracawl/workspace/qualcomm/insight` HEAD at 2026-07-07.

Audience: an insight engineer (human or AI) who wants to adopt smix — as a full replacement for the maestro-driven verify stages, or as a coexisting second runner while maestro stays wired.

Scope: iOS only. Android smix (v6.0+) exists but is dogfood-quality; insight's maestro-android surface is already thin (7 flows tagged `android`), so this guide assumes iOS.

---

## 0. TL;DR

Insight's four documented smix blockers (gol-611 feedback, 2026-06-28) are all closed on smix `develop` at HEAD:

| # | Insight blocker | smix status |
|---|---|---|
| §1 | `runFlow: <path>` shorthand not parsed | Was already supported (`parser.rs:474`, `Step::RunFlow`) — clarified in gol-611 response as a misread. Shorthand + block form both parse. |
| §2 | `runFlow: { when:, commands: [...] }` inline | Landed v6.8 c1, `Step::RunFlowInline` (`parser.rs:481`, `entry.rs:176`). |
| §3 | Sim boots in zh-Hans, `dismiss-open-in` never matches "Open in" | Native enforcement via `sims.json` `locale` field, landed v6.10 c2. Insight's workaround is no longer needed. |
| §4 | iOS 26 "Open in Insight?" dialog hangs XCUITest driver | Root fix is prelaunch env injection (bypasses the dialog entirely). smix `sim launch --child-env` landed v6.8 c2 (`main.rs:388-400`) — same semantics as insight's `SIMCTL_CHILD_*` pattern. |

If you already have `.devtools/verify/prelaunch-sim-app.ts`, the smix migration is mostly *keeping* that file and swapping the runner call from `MAESTRO_BIN` to `smix`.

Two verify stages (`.devtools/verify/stages/perf.ts` + `visual.ts`) are the smallest useful migration unit. That is roughly 40 lines of TypeScript on your side plus zero yaml changes.

---

## 1. What insight looks like today (2026-07-07 snapshot)

Numbers to keep in mind when reading this guide:

- **56 flow yamls** in `.devtools/maestro/flows/*` + **21 subflow yamls** in `.devtools/maestro/subflows/*`.
- **~700 Maestro verb invocations** across those files. Breakdown (top 5): `tapOn` 251, `waitForAnimationToEnd` 167, `runFlow` 166, `extendedWaitUntil` 140, `assertVisible` 87.
- **Selectors**: `id:` 162 (test-id based), `text:` 97 (localized copy + regex OR), `index:` 11, `point:` 2, `below:` 3. **No XPath.** This is right in smix's wheelhouse — smix's selector table (see `docs/ai-guide/03-selectors.md`) supports all of these except `point:` and `below:` (see §12 open questions).
- **Wrapper stack**: `.devtools/verify/{scenario-runner, atom-runner, run-cmd, require-sim, prelaunch-sim-app, maestro-env}.ts` + `.devtools/verify/stages/{perf,visual}.ts`. These are the shape you'll modify — the yamls are almost 1:1 portable.
- **CI**: none. `.github/workflows/release-build.yml` does not run maestro; all e2e runs on developer machines via `bun verify perf` / `bun verify visual`.
- **Sim identity**: hard-coded across files. Device name `sim-insight`, UDID `FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1`, iOS 26.5, bundleId `com.focusai.app.mobile`, Metro `127.0.0.1:8081`, perf receiver `127.0.0.1:9999`.
- **Two runner corpuses**: (a) `.devtools/maestro/flows/*` — the "real user flow" corpus driven from `atom-runner` + verify stages. (b) `.devtools/qa/sim/flows/*` — a QA-mode fixture corpus driven from `.devtools/qa/sim/runner.ts`. This guide targets corpus (a). Corpus (b) is a fine second migration target once (a) is stable.

---

## 2. Mental model — smix vs maestro, three differences that matter

**Same shape overall.** Both are yaml step lists with imperative verbs, run against a booted sim, use `testID`/`text` selectors, produce a JUnit-style report. If you can read a maestro flow you can read a smix flow.

**Difference 1 — smix has a persistent per-sim runner process, maestro spins up a driver per invocation.**
Maestro rebuilds its XCUITest driver each first-run against a sim (this is the "cold-start driver flake" you retry-once in `stages/visual.ts:88` per GOL-588). smix has a single `smix runner up` step that starts a long-lived `SmixRunner.app` on the sim exposing a local HTTP server on `127.0.0.1:22087+`. Subsequent `smix run` invocations reuse it. First-run cost is higher but flake goes away and second-run+ is fast.

Practical consequence: your gate is now three phases, not two.
```
maestro:  require-sim → maestro test <flow>
smix:     require-sim → smix runner up → smix run <flow>  (repeat for N flows)
                                     → smix runner down  (at end of scenario)
```

**Difference 2 — smix selectors are semantic-only; there is no XPath.**
Insight already writes ID-first (`id: input-email`) with fallback to text/index, so this is a non-issue. `point: "50%,90%"` (used twice in `merlin-input.yaml` + `counting-fullscreen.yaml`) does have a smix equivalent: `App::tap_at_coord(nx, ny)` — the single sanctioned coordinate escape hatch (CLAUDE.md §9 #3). See §7 for yaml syntax.

**Difference 3 — failure output is AI-readable JSON, not human logs.**
Insight's runners currently `console.warn` maestro's exit code and stderr. smix on failure emits an `ExpectationFailure` object with:
- What the step wanted (`selector: Id("input-email"), timeoutMs: 10000`)
- What the a11y tree actually contained at failure time (`visibleElements: [...]`)
- Suggestions ranked by string similarity (`suggestions: [{ id: "input-email-field", score: 0.86 }]`)

Your `runCmd` wrapper can just pipe stdout+stderr as JSON — no XML parsing needed. See §11 for parsing tips.

---

## 3. Three migration paths — pick one, don't try to do all three

### Path A — Full swap, single yaml corpus

Every yaml under `.devtools/maestro/flows/` moves to `.devtools/smix/flows/`, `MAESTRO_BIN` → `smix`, done. Estimated effort: 4-8 hours if the yaml surface really is 1:1 portable. Highest ceiling but riskiest if a rare verb turns out to be unsupported.

### Path B — Partial swap, two verify stages only

Migrate `stages/perf.ts` + `stages/visual.ts` to shell to smix. Keep every other maestro path (`.devtools/qa/sim/runner.ts`, `.devtools/test/run-e2e-legacy.ts`, all yamls under `.devtools/maestro/flows/`) working as-is. The perf + visual gates use only `_perf/golden-path.yaml` (1 flow) and `_visual/golden-anchors.yaml` (1 flow) — very small blast radius. Estimated effort: 1-2 hours. **Recommended starting point.**

### Path C — Coexist indefinitely

Keep both runners. Author new tests in smix, freeze maestro corpus. `bun verify perf` runs against smix, `bun test:e2e` runs against maestro. Estimated effort: infra-only, ~30 min for the sim scope switch (see §5). Downside: two lock files, two zombie-cleanup regimes, two flake profiles to keep in your head.

The rest of this guide assumes Path B unless called out.

---

## 4. Selector shape mapping

Everything insight uses maps 1:1 except `point` and `below`.

| Maestro | smix | Notes |
|---|---|---|
| `text: "Log in to Insight"` | `text: "Log in to Insight"` | Identical. Substring match by default in both. |
| `text: "Log in to Insight\|Device"` | `text: { matches: "Log in to Insight\|Device" }` | smix regex form is explicit; smix `text:` short form is exact substring. |
| `id: "input-email"` | `id: "input-email"` | Identical. |
| `id: "dev-bubble\|qa-bubble"` | `id: { matches: "dev-bubble\|qa-bubble" }` | Same regex convention. |
| `optional: true` | drop the assertion or wrap in `try:` block | smix doesn't have `optional:` inline; conditional patterns use `try` / `orElse`. See §5. |
| `index: N` | `id: "foo", index: N` | smix `index:` is per-selector, same as maestro. |
| `point: "50%,90%"` | `tapAtCoord: { nx: 0.50, ny: 0.90 }` | Percentages become normalized (0-1). Single sanctioned coordinate escape hatch. |
| `below: id: "..."` | `anchor: { relation: "below", of: { id: "..." } }` | Spatial-relation chain. Same expressive power but more verbose. Insight uses this in 3 places only. |
| nested `visible:` block | `visible:` block | Structurally identical. |

Insight `subflows/dismiss-open-in.yaml` uses `optional: true` extensively (`- runFlow: { when: { visible: "Open in\|在\"Insight\"中打开?" }, commands: [...] }`). With smix's `runFlow: { when, commands }` form (v6.8 c1) this maps directly — see §5.

---

## 5. Verb mapping

`✅ 1:1` = paste-portable. `↔ renamed` = same semantics, different name. `⚠️ shaped` = same intent, different structure.

| Maestro | smix | Status | Note |
|---|---|---|---|
| `tapOn:` | `tap:` | ↔ renamed | Same selector shapes. |
| `assertVisible:` | `expect: { visible: ... }` | ⚠️ shaped | smix's `expect:` is a wrapper for arbitrary predicates. `expect: { visible: "Log in" }` works. |
| `assertNotVisible:` | `expect: { notVisible: ... }` | ⚠️ shaped | |
| `inputText:` | `fill:` | ↔ renamed | `fill: { id: "input-email", value: "${E2E_EMAIL}" }`. |
| `eraseText:` | `clear:` | ↔ renamed | Deletes existing text before `fill:` if you want the maestro sequence. |
| `pressKey:` | `pressKey:` | ✅ 1:1 | Same key names (`Enter`, `Backspace`, etc.). |
| `hideKeyboard` | `pressKey: Escape` OR `tap: { text: "Done" }` | ⚠️ shaped | smix doesn't have a bare `hideKeyboard` verb yet — dismiss the way a user would. |
| `back` | `pressKey: Back` (Android) / gesture (iOS) | ⚠️ shaped | Insight uses this 3 times, all Android. iOS side use `tap: { text: "Back" }` or a testID. |
| `scrollUntilVisible: { element, direction }` | `scrollUntilVisible: { selector, direction }` | ↔ renamed | Same 4 directions. |
| `swipe` | not used by insight | — | smix has `swipe` but you don't need it. |
| `waitForAnimationToEnd` | `waitForAnimationToEnd` OR (implicit) | ✅ 1:1 | smix retries selectors internally for a configurable poll window, so many `waitForAnimationToEnd:` uses become redundant. Safe to keep as no-ops. |
| `extendedWaitUntil: { visible, timeout }` | `expect: { visible, timeoutMs }` | ⚠️ shaped | The 140 `extendedWaitUntil` calls in insight yamls become `expect:` calls. Timeout units: maestro takes ms, smix takes ms. |
| `runFlow: "path"` | `runFlow: "path"` | ✅ 1:1 | Shorthand supported (`parser.rs:474`). |
| `runFlow: { file, as }` | `runFlow: { file, as }` | ✅ 1:1 | Block form. |
| `runFlow: { when: { visible }, file }` | `runFlow: { when: { visible }, file }` | ✅ 1:1 | Conditional file form. |
| `runFlow: { when: { visible }, commands: [...] }` | `runFlow: { when: { visible }, commands: [...] }` | ✅ 1:1 | Inline form (v6.8 c1). This is the 2026-06-28 addition. |
| `repeat: { times, commands }` | `repeat: { times, commands }` | ✅ 1:1 | |
| `retry: { max, commands }` | `retry: { maxRetries, commands }` | ↔ renamed | Field name difference only. |
| `openLink: "url"` | `openLink: "url"` | ✅ 1:1 | Same behavior. But — see §7 on the iOS 26 SpringBoard dialog. |
| `launchApp: { clearState: true, clearKeychain: true }` | `launchApp: { clearState: true, clearKeychain: true }` | ✅ 1:1 | Insight uses this only in `launch-fresh.yaml`. |
| `stopApp` | `terminate` | ↔ renamed | |
| `takeScreenshot: "name"` | `screenshot: { name }` | ⚠️ shaped | Written into the run's debug output dir; `stages/visual.ts` iterates the dir the same way. |
| `runScript: "path"` / `evalScript: "js"` | Not a smix concept | — | Insight uses these 8 times total. All 5 `runScript` invocations are for the tenant-config JSON stamp (`copy-tenant-config.js`); all 3 `evalScript` are one-line JS expressions. Options: (a) inline the js into your `atom-runner` wrapper as a shell-out before the flow starts, or (b) keep those specific flows on maestro. Path B doesn't hit these since perf + visual don't use `runScript`. |

**Verbs insight does NOT use** that smix has: `doubleTap`, `longPress`, `swipe`, `assertNoAlert`, `waitForCondition`. Don't worry about them.

---

## 6. The prelaunch pattern — keep it verbatim

Insight's `.devtools/verify/prelaunch-sim-app.ts` is 32 lines that:

1. `xcrun simctl terminate <udid> <bundleId>` (deterministic state)
2. Map every `childEnv[K] = V` to `SIMCTL_CHILD_<K>=V` in the *parent process* env
3. `xcrun simctl launch <udid> <bundleId>` (child inherits `SIMCTL_CHILD_*`)

You do not need to reimplement this on the smix side. Two options:

**Option A — keep `prelaunch-sim-app.ts` unchanged.** It's just `xcrun simctl` calls; it works whether the next step is maestro or smix. Your `stages/perf.ts` calls it before firing the flow.

**Option B — use `smix sim launch --child-env` directly.** Landed v6.8 c2 (`crates/smix-cli/src/main.rs:388`). Semantics are identical to `prelaunch-sim-app.ts` plus explicit registry-aware sim resolution. Command:

```bash
smix sim launch sim-insight com.focusai.app.mobile \
  --child-env LAUNCH_FORCE_PUSH=true \
  --child-env INSIGHT_PERF_RECEIVER=http://127.0.0.1:9999
```

This is what `stages/perf.ts` should shell to. `sim-insight` resolves via `sims.json` registry (see §8).

Either way, **the reason this pattern exists survives**: iOS exposes `SIMCTL_CHILD_*` in the child's `ProcessInfo.environment`, and `insight-native-helper`'s `getLaunchEnvVar` reads it. The `LAUNCH_FORCE_PUSH=true` toggle flips `remote-pulse.ts` into sampler-push mode without any user tap. This has nothing to do with maestro vs smix; it works with any runner.

The comment in `stages/perf.ts:65-73` is the canonical explanation and should be quoted verbatim in your new smix wrapper — it justifies the choice against three iOS 26 / SDK 56 traps (expo-dev-launcher argv-swallowing, foreground `simctl openurl` never delivering, dev-launcher intercepting cold `launchApp`). All three still apply under smix; the prelaunch pattern remains the fix.

---

## 7. The iOS 26 "Open in Insight?" SpringBoard dialog

The gol-611 §4 blocker. What happened, why it's not a blocker anymore, and what to do in yaml.

**What used to break.** `subflows/dismiss-open-in.yaml` runs after every `openLink:` to dismiss the SpringBoard confirmation ("`打开 'Insight'?`" on zh-Hans, "`Open in 'Insight'?`" on en). Under smix's XCUITest runner, `Descendants matching type Dialog/Popover/Alert/Sheet` queries hang for 30 s each. Insight's flow has 4 openLinks × 4 conditional finds = 192 s per run.

**Why it's fixed.** Two independent smix changes:

1. `sim launch --child-env` (v6.8 c2): the *root* fix. If you enter the app via `simctl launch` with the toggle env injected up front, no subsequent `openLink:` needs to fire — the dialog never appears. This is what `stages/perf.ts` uses today and what your smix migration should preserve.
2. Locale enforcement (v6.10 c2): if you *do* still need `dismiss-open-in.yaml`, smix now boots the sim in the locale set by `sims.json` `locale: "en"`, so `visible: "Open in"` matches. No manual `defaults write -g AppleLanguages` needed.

**Practical guidance.**
- For the perf gate: don't use `openLink:` at all. Use `sim launch --child-env` at scenario start; the app comes up already carrying the flag.
- For the visual gate: it enters via `subflows/launch-warm.yaml`'s `openLink: exp+focus-ai-app://...` deeplink. Set `locale: "en"` in `sims.json` and `dismiss-open-in.yaml` works. Or, better, rewrite `launch-warm.yaml` to skip the `openLink:` in favor of `sim launch` — but that's a wider change and can wait.
- Do not carry over the `Descendants matching type Dialog/Popover/Alert/Sheet` timeout debugging from the gol-611 writeup — that hang is fixed in current smix runner (v6.8 tree walker rewrite).

---

## 8. Sim identity + registry

**Update (v0.2.0):** the `.smix/sims.json` `locale:` field applies at *next sim boot* — not to an already-booted sim. For a running sim, use `smix sim locale <DEVICE> <LANG> --reboot` (v0.2.0 new command) to write the locale and reboot in one step. See gol-611 path-b response §5.

Insight identifies its sim three different ways today (`REQUIRED_DEVICE = 'sim-insight'` in `require-sim.ts`, hard-coded UDID `FFC57DAE-...` in stale `main-report.xml`, `SIM_NAME` in `qa/sim/runner.ts`). smix wants exactly one source of truth: `.smix/sims.json`.

Add insight to your `sims.json` — this file is at your repo root, next to your `package.json`, not in smix's repo:

```json
{
  "version": 1,
  "sims": {
    "insight": {
      "deviceName": "sim-insight",
      "udid": "FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
      "locale": "en"
    }
  }
}
```

`locale: "en"` is the v6.10 c2 field — smix enforces it at boot via `defaults write` before the app installs, so `dismiss-open-in` matchers work under the English string.

After this, everywhere insight has `--device sim-insight` or the raw UDID, replace with `--device insight` (the alias) — smix will resolve via the registry.

`require-sim.ts`'s three-way check (booted + app installed + Metro answering) can stay unchanged. It's environmental preflight, orthogonal to smix.

---

## 9. Concrete integration — `stages/perf.ts` and `stages/visual.ts`

The Path B migration. Two files.

### `stages/perf.ts` — before

```ts
// simplified
const bin = process.env.MAESTRO_BIN ?? `${process.env.HOME}/.maestro/bin/maestro`
const flow = '.devtools/maestro/flows/_perf/golden-path.yaml'
await prelaunchSimApp({ bundleId, udid, childEnv: { LAUNCH_FORCE_PUSH: 'true' } })
const result = await runCmd(bin, [
  '--platform=ios', '--device', udid, 'test',
  '--debug-output', outDir,
  flow,
  ...maestroEnv(),
])
```

### `stages/perf.ts` — after (smix)

```ts
const bin = process.env.SMIX_BIN ?? `${process.env.HOME}/.local/bin/smix`
const flow = '.devtools/smix/flows/_perf/golden-path.yaml'

// Runner up: one-time cold build, then reused across the whole scenario.
// Idempotent — if the runner is already up on this sim it's a no-op returning fast.
await runCmd(bin, ['runner', 'up', '--sim', 'insight'], { timeoutMs: 600_000 })

// Prelaunch with the perf-receiver toggle. Same semantics as your existing
// prelaunch-sim-app.ts; using smix keeps it registry-aware.
await runCmd(bin, [
  'sim', 'launch', 'insight', 'com.focusai.app.mobile',
  '--child-env', 'LAUNCH_FORCE_PUSH=true',
  '--child-env', `INSIGHT_PERF_RECEIVER=${RECEIVER_URL}`,
])

// Run the flow. --debug-output writes JSON events + screenshots to outDir,
// same directory shape as maestro's --debug-output.
const result = await runCmd(bin, [
  'run', flow,
  '--sim', 'insight',
  '--debug-output', outDir,
  ...smixEnv(),           // same shape as maestroEnv() — see below
])

if (result.code !== 0) {
  // smix emits ExpectationFailure JSON on stdout; no XML parsing needed.
  const failure = tryParseFailure(result.stdout)
  return { ok: false, failure }
}
```

### `stages/visual.ts` — the same shape

Swap `runCmd(MAESTRO_BIN, [...])` for `runCmd(SMIX_BIN, [...])`. Keep the diff-PNG comparison (`.devtools/test-baselines/visual/ios/*.png`), thresholdPct, `--accept-visual` promote path — all of that is your logic, orthogonal to which runner produced the shots.

The retry-once-on-cold-flake block (`stages/visual.ts:88-106`, GOL-588) can be removed. smix's persistent runner doesn't have that flake — the equivalent condition is "first `runner up` per session is slow"; subsequent runs reuse the process.

### `maestro-env.ts` → `smix-env.ts`

```ts
// maestro-env.ts today: reads .env.local, produces ["-e", "K=V", "-e", "K=V", ...]
// smix takes the same shape via --env K=V (or a repeated --env flag).
export function smixEnv(): string[] {
  const raw = readEnvFile('.env.local')
  return Object.entries(raw)
    .filter(([k]) => k.startsWith('E2E_') || k.startsWith('IMAP_'))
    .flatMap(([k, v]) => ['--env', `${k}=${v}`])
}
```

Yamls reference vars the same way: `${E2E_EMAIL}`. No yaml changes needed for env interpolation.

### `run-cmd.ts`

No changes. Same `spawn` wrapper, same 5-minute default timeout, same SIGTERM→SIGKILL escalation. The 3-second grace period between signals is generous enough that smix's runner-down-on-teardown works cleanly.

### Runner teardown

Add a scenario-level teardown that fires `smix runner down --sim insight` once the whole scenario finishes. Insight's `scenario-runner.ts` has a `finally` block already; hook here.

```ts
try {
  await runScenario(name)
} finally {
  await runCmd(SMIX_BIN, ['runner', 'down', '--sim', 'insight'], { timeoutMs: 15_000 })
    .catch(() => {}) // best-effort; sim may already be down
}
```

Leaving the runner up between `bun verify perf` and `bun verify visual` runs is fine — the second invocation is fast. `runner down` before `sim:down` (in `bun sim:down`) is idempotent and safe.

---

## 10. Two yamls to actually port (Path B minimum)

Only two flow files are in the perf/visual gate hot path.

### `flows/_perf/golden-path.yaml` (30-second cold-start + idle perf scenario)

Insight's version uses `openLink: "insight://lab/perf/golden"` after a warm launch. Under smix Path B, replace the openLink with a `sim launch --child-env INSIGHT_PERF_ANCHOR=golden` at the scenario start (before the flow) and drop the `openLink:` step from the yaml. The rest of the yaml (assertions, waits, screenshots) ports 1:1.

Rationale: the deeplink was there because expo-dev-launcher swallowed launchApp args pre-v6.8. With prelaunch env, the toggle path is the child-env; the deeplink is redundant.

### `flows/_visual/golden-anchors.yaml` (visual regression anchors)

Ports 1:1. Every step is either `openLink: "insight://lab/visual-hub/<anchor>"` + `takeScreenshot: "<anchor>"` + `waitForAnimationToEnd`, or a `runFlow: ../subflows/dismiss-open-in.yaml`. All three are direct-port. Keep the `dismiss-open-in` subflow calls until locale-enforcement is verified on your machine; then drop them.

Do not port `subflows/launch-fresh.yaml` / `launch-warm.yaml` at this stage — perf + visual don't need them. The `require-sim` preflight + `sim launch --child-env` combo replaces the "get to a known-good app state" work those subflows did.

---

## 11. Debugging — reading smix failure output

Two output modes as of v0.2.0:

**Default (`--format human`)** — stderr text summary via `ExpectationFailure::to_prompt()`. Human-oriented; matches maestro-style output shape.

**`--format json`** — stdout emits a single top-level JSON object at exit time. Consumers `JSON.parse(runCmd(...).stdout)`. Shape when a step fails:

```json
{
  "flow": ".devtools/smix/flows/login.yaml",
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

Field notes:
- `code` — one of the `FailureCode` variants (`ElementNotFound`, `Timeout`, `DriverError`, ...).
- `message` — human text (mirrors `to_prompt` prefix).
- `selector` — debug-formatted `Selector` variant.
- `suggestions` — string similarity ranked list from `smix-error::build_suggestions` (edit-distance today; rename detection in a later milestone — see `insight-roadmap.md` §G).
- `visibleCount` — number of visible elements captured at failure; the full array is available in `--debug-output <dir>/run-summary.json`.

The shape is stable — if smix widens it, existing keys keep their meaning.

Parse it in your `atom-runner`; surface the top suggestion in the terminal summary; store the whole thing in `.tmp/qa-sim/failures/<flow>-<step>.json` for later inspection.

Two things worth doing on top:

1. **Wire failure JSON into scope-log allowlists.** Your `BASE_METRO_LOG_ALLOWLIST` gates ERROR/WARN lines. When a smix flow fails, the failure JSON may reference a testID; grep the metro log for it in the same window and attach the surrounding metro log to the failure record. This is a pattern insight already does per-scope; it works for smix too.
2. **Baseline a "no suggestion is clearly the same" heuristic.** If the top suggestion confidence is above 0.75 and it's a rename (see GOL-611 clarifications on suggestion sources), surface it as a testID drift alert rather than an assertion failure — same category as your `expired-test-detector.ts`.

---

## 12. Known gaps + open questions

**Verified working end-to-end** (as of 2026-07-07, smix `develop` HEAD):

- ID / text / index selectors with substring + regex.
- `runFlow` shorthand + block + conditional-file + inline-commands.
- `sim launch --child-env` prelaunch env injection.
- `locale` enforcement at boot (v6.10 c2).
- Persistent runner + `runner up` / `run` / `runner down` lifecycle.

**Verdict on items originally listed as "not yet verified"** — updated with v0.2.0 status:

- `point:` → `tap_at_coord` in `merlin-input.yaml` + `counting-fullscreen.yaml`. [**deferred to v0.2.5**] path B (perf + visual) doesn't touch these yamls; Path A codemod (roadmap §E) picks them up.
- `below:` positional selectors (3 sites, none in perf/visual). [**deferred to v0.2.5**] same rationale — Path A only, covered by codemod scope.
- Regex `text:` matching against localized zh-Hans strings after `locale: "en"`. [**verified in v0.2.0**] `smix sim locale <DEVICE> en --reboot` (v0.2.0 new command) applies locale to a booted sim; subsequent regex text matching operates on English strings deterministically.
- `hideKeyboard` (3 sites, qa/sim/subflows/, none in perf/visual). [**deferred to v0.2.5**] Path A codemod scope. Workaround in §5 (guide) works today.
- `back` on iOS (3 sites, all `tags: android`). [**out of scope**] Non-issue for iOS Path B; Android smix path handles.

**Known smix side gaps that would matter if you did Path A**:

- `runScript: "copy-tenant-config.js"` and `evalScript:` — no smix equivalent. [**tracked as roadmap §E addendum**] insight-roadmap.md notes this as an open item; `runShell: <cmd>` addition would cover it. Not blocking Path B.
- Report format. smix's stdout is JSON via `--format json` (v0.2.0); no JUnit XML directly. [**deferred to v0.2.5**] JUnit shim planned as part of migrate codemod milestone. Insight-side ~20 LOC shim reading the JSON is a fine bridge until then.
- No headless mode. [**intentionally out of scope**] smix always drives a real sim; matches insight's current use.

**Insight side gaps that would need attention regardless of runner**:

- `flows/bootstrap/*` deleted but `bootstrap-report.xml` still references them. Old artifact; safe to delete `bootstrap-report.xml`.
- `flows/_skip/` 15 flows parked for various reasons; when you migrate, keep the same skip semantics — smix has `enabled: false` at the flow header level for the same effect.
- `main-report.xml`'s 9 failures are pre-existing app bugs, not runner bugs. Migrating to smix does not fix them; they must be fixed on the app side or the flow author side.

---

## 13. CI considerations (short — insight doesn't run maestro in CI today)

If you decide to add e2e to CI at some point:

- smix has `scripts/ci/prep-selftest-gate.sh` in its own repo — a reference for how to bring up sim + smix-server + install fixture on GitHub-hosted macOS runners. It's macos-15 shaped and covers postgres + valkey deps that insight doesn't need.
- macos-15 hosted runner Xcode is 16.4 (iOS 26.2 latest sim); dev boxes run Xcode 27 seed (iOS 26.5). smix's `ensure_sim_alias` handles the fallback: if `sims.json` `runtime` isn't available on the runner, it picks latest available and continues. So `sims.json` committed with iOS 26.5 works cross-machine.
- Cold `smix runner up` on GH-hosted macOS is ~2-3 minutes (cargo build cache miss + Swift build of `SmixRunner.app` + first sim boot). Budget accordingly. See smix `.github/workflows/matrix-nightly.yml` for a reference `timeout-minutes: 180`.

---

## 14. Appendix — one-page cheat sheet

**Boot + preflight** (per developer session)
```bash
bun sim:up                                 # boots sim-insight
                                           # (this stays maestro-era code)
smix runner up --sim insight               # brings up SmixRunner.app (~5s hot / ~2m cold)
```

**One flow** (perf hot path)
```bash
smix sim launch insight com.focusai.app.mobile --child-env LAUNCH_FORCE_PUSH=true
smix run .devtools/smix/flows/_perf/golden-path.yaml --sim insight
```

**Teardown** (per scenario)
```bash
smix runner down --sim insight
bun sim:down
```

**Full verify** (once wired in Path B)
```bash
bun verify perf                            # → stages/perf.ts → smix
bun verify visual                          # → stages/visual.ts → smix (--accept-visual to promote baselines)
```

**smix v0.2.0 flag reference** (for `smix run`)
```
--env KEY=VALUE           # repeatable; yaml ${VAR} interpolation
--debug-output <DIR>      # writes <DIR>/run-summary.json on exit
--verbose                 # debug-level tracing on adapter/sdk/driver
--format <human|json>     # stdout JSON at exit when json; human by default
--device <alias>          # sims.json alias (preferred over raw UDID)
--runner-port <port>      # default 22087 iOS; sims.json auto-assigns in v0.2.5
--no-launch               # skip foreground call (use with `sim launch --child-env`)
```

**Debug a failing step**
```bash
# The debug-output dir contains every step's a11y tree + screenshot.
smix run <flow> --sim insight --debug-output ./out --verbose
# On failure, the ExpectationFailure JSON is on stdout; the failure step's
# screenshot is at ./out/step-<N>-fail.png; the a11y tree is at ./out/step-<N>.json.
```

---

## 15. Where to file smix issues

- Small parser/runner issues: PR against `develop` in `github.com/goliajp/smix` targeting `crates/smix-adapter-maestro/` or `crates/smix-cli/`.
- Insight-specific requests where the "why" needs project context: append to `.claude/state/gol-611/smix-feedback.md` on the insight side; the smix side (`docs/ai-guide/insight-feedback-*.md`) replies in kind. That kept the v6.8/6.10 conversation coherent; the pattern works.
- Emergency / blocker: mention the CLAUDE.md §12.2 "capability-gap-first" rule in the report — smix side treats missing insight-facing capabilities as core gaps to fill, not driver-specific patches.

---

## Related docs

- `docs/ai-guide/01-quickstart.md` — smix quickstart for a greenfield project.
- `docs/ai-guide/03-selectors.md` — full selector reference (including the anchor spatial-relation chain).
- `docs/ai-guide/04-actions.md` — full verb reference.
- `docs/ai-guide/insight-feedback-gol-611-response.md` — the 2026-06-28 gol-611 write-back that this guide follows up on.
- `docs/ai-guide/v6.12-simx-to-smix-rename-migration.md` — if any of your scripts still reference `simx`, port them per this doc.
- Insight side: `.devtools/verify/{scenario-runner, atom-runner, run-cmd, require-sim, prelaunch-sim-app, maestro-env}.ts` — the wrapper stack this guide plugs into.
