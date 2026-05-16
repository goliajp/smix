# simx

> Companion to the iOS Simulator — AI-native automation for **TDD** and **E2E testing**.

Simulator only. No real-device support, by design.

## Why another one

Maestro, Appium, Detox were all designed before AI coding agents were the
primary author. simx is designed for the opposite: **most tests will be
written by an AI**, so every API choice is graded on "does an AI write
correct code on the first try given only the function signature."

Concretely:

- **DSL shape mirrors Playwright** — overwhelming training-data majority,
  so AI authoring quality is highest there.
- **Selectors are semantic only** (text / id / label / role+name). No
  xpath, no coordinates — AI writes those poorly.
- **Errors are AI-readable fix prompts**, not human stack traces. Paste
  a failure back to a coding agent and it can self-correct.
- **MCP server is first-class**, with `screen_describe` as the agent's eyes.
- **Recording produces few-shot samples** (code + a11y trace + screenshots)
  that future test generation can use as context.

See [`docs/design.md`](./docs/design.md) for the full rationale, the
two rounds of competitor research, and the iOS 26+ compatibility plan.

## Status

**v1.0** — release ready. All 7 Success Criteria pass via
`bash scripts/v1-acceptance.sh` (single-line JSON: 7 SC + total + all_ok).

| # | Criterion | Status | Evidence (`v1-acceptance.sh` field) |
|---|---|---|---|
| [1] | AI 0-shot from README "Authoring guide" + MCP server | ✅ | `sc1_ai_0shot_assets` |
| [2] | One-shot self-correct from `ExpectationFailure.toPrompt()` | ✅ | `sc2_self_correct` |
| [3] | Cold start `simx run` → first tap < 5s | ✅ | `sc3_cold_start_under_5s` |
| [4] | Single tap < 50ms (iOS 26 main path, 9-arg digitizer) | ✅ | `sc4_tap_under_50ms` |
| [5] | 100-case serial long-run, no zombie | ✅ | `sc5_longrun_100` |
| [6] | iOS 17.5 / 18.4 / 26.x runtime matrix | ✅ | `sc6_runtime_matrix` |
| [7] | Claude Code plugin one-step install | ✅ | `sc7_plugin_install` |

| Baseline | Value |
|---|---|
| TS vitest tests | 593 passed (27 test files) |
| Swift unit tests | 102 passed |
| MCP tools | 27 (ping / 7 lifecycle / 4 observe / 7 interaction / 3 compound / 4 system / 1 VLM `explain_screen`) |
| `simx doctor` checks | 6 (xcode / runtimes / claude / bun / hid / axp), all `supported` |
| Prod deps | 3 (`citty`, `@modelcontextprotocol/sdk`, `zod`) |

See [`docs/v1.md`](./docs/v1.md) for the full v1 scope & decision log,
[`docs/roadmap.md`](./docs/roadmap.md) for v1.1 / v2 plans, and
[`docs/plugin-install.md`](./docs/plugin-install.md) for the Claude Code plugin install flow.

```
src/
  core/        Selector, Role, Screen, Error — wire types shared across
               SDK / CLI / MCP / driver; resolve-selector (4 base + 8
               modifiers, pure TS)
  sdk/         App, ElementHandle, expect, matchers, test runner
  driver/      Driver interface + SimctlDriver (real tap + tree /
               findOne / findAll / waitFor real impl)
  sim/         simctl wrapper, SimSession, Cell L1, runner-client (HTTP
               to SimxRunner XCUITest /health + /tap + /tree),
               a11y-tree-source, AXP probe (via simx-host-hid)
  cli/         simx list | run | doctor | repl | mcp
  mcp/         27 MCP tools (lifecycle / observe / interaction /
               compound / system / vlm explain_screen)
examples/      Golden-path tests — what an AI should produce; see
               examples/README.md for per-file prereqs and run guide
docs/          Design decisions, v1 scope, roadmap, plugin install
```

## Quick start

Pick one:

**Claude Code plugin (recommended for end users):**

```bash
claude --plugin-dir /absolute/path/to/simx
# then inside the claude session:
#   /mcp
# to enumerate the 27 simx:: tools
```

Full flow: [`docs/plugin-install.md`](./docs/plugin-install.md).

**Local dev (clone + bun):**

```bash
git clone <repo> simx && cd simx
bun install
bun run typecheck
bun x vitest run     # 593 vitest tests
bash scripts/v1-acceptance.sh   # 7 Success Criteria, single-line JSON
```

**npm registry (placeholder — v1.0+):** publishing to npm and Claude
Code marketplace lands post-release; see
[`docs/plugin-install.md` §4](./docs/plugin-install.md).

> _Note: the `homepage` / `repository` fields in `.claude-plugin/plugin.json`
> and `docs/plugin-install.md` currently hold the placeholder
> `https://github.com/goliajp/simx`; replace with the
> real repo URL on first GitHub push._

## Example

```typescript
import { test, expect } from 'simx/test'

test('login flow', async ({ app }) => {
  await app.launch('com.example.app', { fresh: true })

  await app.tap({ text: 'Sign in' })
  await app.fill({ id: 'emailField' }, 'user@example.com')
  await app.fill({ id: 'passwordField' }, 'secret')
  await app.tap({ role: 'button', name: 'Continue' })

  await expect(app.element({ text: /Welcome/ })).toBeVisible()
})
```

More in [`examples/`](./examples). See
[`examples/README.md`](./examples/README.md) for the per-file
prereq & run guide. Selector showcase: see
[`examples/login-tap.test.ts`](./examples/login-tap.test.ts) for v0.3
selector forms running against real Settings UI.

---

## Authoring guide for AI agents

> The section below is written to be pasted directly into a system prompt
> for an AI coding agent writing simx tests.

### Selectors — what to use

Always prefer, in this order:

1. `{ role: 'button', name: 'Sign in' }` — most robust, AI-friendly
2. `{ id: 'loginButton' }` — when the app sets `accessibilityIdentifier`
3. `{ text: 'Sign in' }` or `{ text: /Sign in/i }` — visible text
4. `{ label: 'Login submission' }` — `accessibilityLabel`

Disambiguate with modifiers, not by index:

- `{ text: 'Skip', near: { text: 'Onboarding' } }`
- `{ role: 'button', name: 'Save', inside: { role: 'alert' } }`
- `{ role: 'cell', name: /Headphones/, below: { text: 'Recommended' } }`

Use `nth` / `first` / `last` **only as a last resort** when the UI has
genuinely identical siblings.

### Selectors — what NOT to use

- ❌ XPath — not supported on purpose. If you want to write XPath, you
  haven't found the right semantic anchor yet.
- ❌ Coordinates — never write `tap({ x: 100, y: 200 })`. Not part of the
  surface.
- ❌ CSS selectors — wrong platform.
- ❌ Chained query strings — selectors are objects, not strings.

### Actions

```typescript
app.launch(bundleId, { fresh?, args?, env? })
app.terminate(bundleId)
app.background() / app.foreground(bundleId)

app.tap(selector)
app.doubleTap(selector)
app.longPress(selector, { durationMs })
app.fill(selector, text)         // autoclear + focus + type
app.clear(selector)
app.swipe('up' | 'down' | 'left' | 'right', { from? })
app.scroll(selector, 'up' | 'down')
app.scrollTo(selector)            // scroll element into view

app.pressKey('return' | 'delete' | 'space' | 'tab' | 'escape' | 'arrow*')
app.hideKeyboard()

app.waitFor(selector, { timeoutMs })   // never use raw sleep
app.screenshot()
app.describe()                          // full ScreenDescription

app.element(selector)                   // -> ElementHandle for expect()

app.pasteboard.set(text)
app.pasteboard.get()
app.permissions.grant('camera' | 'photos' | 'notifications' | ...)
app.system.openUrl(url)                 // deep link
app.system.setAppearance('light' | 'dark')
app.system.setLocale('en_US')
app.system.setNetwork('online' | 'offline' | 'slow-3g')
```

### Assertions

```typescript
expect(app.element(sel)).toBeVisible({ timeout? })
expect(app.element(sel)).toBeHidden({ timeout? })
expect(app.element(sel)).toHaveText('exact' | /regex/)
expect(app.element(sel)).toBeEnabled()
expect(app.element(sel)).toHaveCount(n)
expect(await app.describe()).toMatchSnapshot('name')
```

All matchers auto-poll until success or timeout (default 5s). You do
**not** need to write retry loops.

### Things to never do

- **Never call `setTimeout` or write a sleep helper.** If you need to
  wait for something, use `app.waitFor(selector)` or pass `{ timeout }`
  to an `expect` matcher.
- **Never query elements by reading `app.describe()` and filtering.**
  That bypasses the matcher engine and gives no auto-poll. Just write
  the selector and let `expect` resolve it.
- **Never invent action names.** The surface above is exhaustive. If
  something seems missing (e.g. "pinch"), it isn't built yet — write
  the test as if it existed and surface the gap.
- **Never write end-of-test cleanup that depends on screen state.**
  Use `app.terminate(bundleId)` if needed; don't try to "navigate back
  to home" through taps.

### When a test fails

`ExpectationFailure` is structured for you to read directly:

```
FAIL [ELEMENT_NOT_FOUND]: Expected element to be visible.
  selector: { text: "Sgin in" }
  suggestions:
    - Did you mean "Sign in"? (similarity 0.86)
  visible elements (top 10):
    - button name="Sign in" id="signInButton"
    - button name="Register"
    - link name="Forgot password?"
```

Update the selector based on the suggestions / visible elements list.
Do not try/catch around assertions to swallow failures.

---

## Local dev

```bash
bun install
bun run typecheck
bun run test            # 593 vitest unit tests across 27 test files
```

Swift bridge unit tests + the iOS-26 sim e2e smoke need Xcode 26.x:

```bash
swift test --package-path swift-bridge   # 102 Swift unit tests
```

### Dev simulator

simx tests + e2e scripts read the booted UDID from
`.simx/dev-sim.txt`. Create the dev sim once:

```bash
mkdir -p .simx
UDID=$(xcrun simctl create simx-dev "iPhone 17 Pro" \
  com.apple.CoreSimulator.SimRuntime.iOS-26-4)
xcrun simctl boot "$UDID"
echo "$UDID" > .simx/dev-sim.txt
```

The dev sim UDID is the one stable target every e2e script reuses;
override with `SIMX_DEV_LOCK=off` if you need a one-off run against a
different booted sim.

### v1.0 acceptance smoke

End-to-end verification of all 7 Success Criteria:

```bash
bash scripts/v1-acceptance.sh
# Single-line JSON output:
# {"sc1_ai_0shot_assets":"ok",...,"total":7,"all_ok":"ok"}
# Expected: exit 0, all 7 SC = ok, ~15s on warm dev sim
```

For per-version sub-gates (kept for regression bisection), see
`scripts/simx-v0{2,3,4,5,6}-acceptance.sh`.

## Roadmap

- **v0** (done) types-only SDK surface + golden-path examples
- **v0.1** (done) simctl wrapper + SimSession + SimctlDriver + CLI (`list` / `run` / `doctor`) + real e2e screenshot
- **v0.2** (done) HID injection (host-side IOHIDEvent digitizer + 9-arg Indigo) + InputChannel + SimxRunner XCUITest HTTP server + `SimctlDriver.tap` real path + Cell L1
- **v0.3** (done) selector resolver (4 base + 8 modifiers) + AX read (XCUITest snapshot via runner `/tree`; AXP host-side probe)
- **v0.4** (done) SDK behaviour completeness — matcher failure context real fill, `.simx/trace/` output, `waitFor` improvements, HID `fill`
- **v0.5** (done) CLI `repl` + full `doctor` (6 checks: Xcode / runtime / claude / bun / hid / axp, with `compatibility: supported`)
- **v0.6** (done) MCP server 27 tools (ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 VLM `explain_screen`)
- **v0.7** (done) Hardening: 100-case long-run (auto runner restart every 50) + CI matrix iOS 17.5/18.4/26.x + Claude Code plugin (`.claude-plugin/plugin.json`) + `scripts/v1-acceptance.sh`
- **v1.0** (✅ released) all 7 Success Criteria pass, release docs done
- **v1.1** Watch mode + Cell L4 parallel scheduling + matrix run + Cell L3 TUI status line
- **v2** Full recorder + Vision OCR fallback + on-device Foundation Models + snapshot diff + Xcode 26 Automation Explorer parasite + Cell L3 in-line frame streaming viewer

See [`docs/roadmap.md`](./docs/roadmap.md) for the canonical roadmap.

## License

MIT
