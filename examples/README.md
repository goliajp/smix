# simx examples

Real, runnable AI-authored test samples. Each one is the shortest
plausible code an AI agent should produce for the listed scenario.

## What's in here today (v0.3)

| File | What it exercises | Prerequisites |
|---|---|---|
| `tap-text-selector.test.ts` | SDK `app.tap({text:"General"})` → `SimctlDriver.tap` → `RunnerClient` → POST `/tap` → XCUITest → real iOS UI navigation (Settings → General sub-page). The v0.2 closing-checkpoint smoke. | (1) iOS 26.x sim booted (UDID in `.simx/dev-sim.txt` or `--udid <UDID>`); (2) SimxRunner XCUITest target running (`bash scripts/simx-runner-health.sh` for manual start, or `bash scripts/simx-c6-tap-e2e.sh` for auto-start + probe). |
| `login-tap.test.ts` | v0.3 selector resolver showcase: `text` / `text:RegExp` / `id` / `role+name` / `inside` modifier / `waitFor` / `expect.toBeVisible` against the real Settings UI. The v0.3 closing-checkpoint smoke. | Same as `tap-text-selector.test.ts`. |
| `screenshot-only.test.ts` | SDK `app.launch` + `app.screenshot()` — the v0.1 milestone. No HID / runner needed. | iOS sim booted with mobilesafari pre-installed (Apple defaults). |

## Pending v0.7+ (HID keyboard fill / multi-action)

`_v03-pending/` holds illustrative tests written in the v0 / v0.1 design
stage. They use bundle ids that don't exist on the host (`com.example.app`)
and actions (`fill`, `longPress`, `scroll`, `pasteboard.set`, deep links)
that aren't wired yet. See `_v03-pending/README.md`. Do not run them as
smoke; v0.7+ will resurrect `login.test.ts` (needs HID keyboard `fill`)
and v0.4-v0.5 will resurrect `cart-checkout.test.ts` (needs `longPress` /
`scroll` / pasteboard / deep link).

## Running

```bash
# v0.3 closing smoke (auto-starts runner, drives examples, double-side probes).
bash scripts/simx-v03-acceptance.sh

# v0.2 sub-gate alone (subset of the v0.3 closing smoke).
bash scripts/simx-v02-acceptance.sh

# Single example (runner must already be up — see prereqs above).
bun src/cli/index.ts run examples/tap-text-selector.test.ts

# v0.3 selector showcase (depends on the SimxRunner already being up).
bun src/cli/index.ts run examples/login-tap.test.ts

# v0.1 screenshot smoke (no runner needed).
bun src/cli/index.ts run examples/screenshot-only.test.ts
```
