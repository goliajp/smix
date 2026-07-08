# @goliapkg/smix demo-app SDK surface coverage matrix

> The demos here exercise the full `@goliapkg/smix` SDK surface. They stand
> in as evidence the SDK is usable end-to-end for the workflows Detox /
> Playwright users typically write.

## Coverage matrix

| SDK API | login | form-validation | list-scroll | multi-screen-nav |
|---|---|---|---|---|
| `Smix.launchApp(.bundleId)` | Yes | Yes | Yes | Yes |
| `App.tap(Selector.id)` | Yes | Yes | Yes | Yes |
| `App.fill` | Yes | Yes | — | — |
| `App.swipe` | — | — | Yes (4 directions) | — |
| `App.screenshot` | — | — | — | — |
| `App.pressKey` | — | — | — | Yes (escape) |
| `App.tapAtCoord` (0..1) | — | — | — | Yes |
| `App.terminate / relaunch` | — | — | — | Yes |
| `App.openUrl` | — | — | — | Yes |
| `App.find` | Yes | Yes | Yes | Yes |
| `Locator.toBeVisible` | Yes | Yes | Yes | Yes |
| `Locator.toContainText` | Yes | — | Yes | — |
| `Locator.toHaveLabel` | — | Yes | — | — |
| `Locator.toHaveCount` | — | — | Yes | — |
| `MockSimRuntime.afterSnapshot` hook | Yes | Yes | Yes | Yes |
| `Selector.id / .text / .role / .label` | Yes | Yes | Yes | Yes |

## Remaining surface (not exercised in demos)

- `App.systemPopups()` — needs system-alert simulation
- `App.tree()` — used internally by Locator but not as user-facing call
- `Selector.focused()` / `.anchor()` / `.localizedText()` — covered by
  vitest schema tests in `src/__tests__/SelectorFullSchema.test.ts`
- `Selector` fluent chaining (`.below()`/`.nth()`/etc) — covered by
  vitest schema tests

## How to run all flows

```bash
cd npm/smix-rn/examples/demo-app
bun login-flow.ts
bun form-validation-flow.ts
bun list-scroll-flow.ts
bun multi-screen-nav-flow.ts
```

Each prints a `PASS` line and exits 0 on success, or a `FAIL` line and
exits 1 on `ExpectationFailure` or other error.

## Friction metrics vs Detox (estimated, mock baseline)

| metric | smix demo | Detox equivalent (estimate) |
|---|---|---|
| LoC per flow | ~80-100 | ~140-180 |
| Setup time (mock) | 0 (no Metro/sim) | n/a (Detox needs sim) |
| Failure JSON | structured AI-readable via `ExpectationFailure.toJson()` | text stack trace |
| Selector schema | typed Playwright-style (`.id().below().nth()`) | manual matchers `by.id().withAncestor()` |
| Locator poll | built-in 250ms tick + WRONG_STATE/TIMEOUT distinction | `waitFor` with manual loops |
