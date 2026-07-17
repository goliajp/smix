# 11 — MCP

> Give an agent the simulator. Launch, look, tap, type, assert — over MCP,
> with no yaml in between.

## When this is the right tool

YAML flows are for tests you run again. MCP is for the loop *before* that
exists: an agent building a feature, reproducing a bug, or finding out what a
screen actually contains.

The rule of thumb: if you'd write the flow down and keep it, write yaml. If
you're exploring, drive it over MCP.

## Setup

Two pieces: the runner drives the simulator, the MCP server talks to the
runner. Bring the runner up first — the server does not start it.

```bash
smix runner up <udid> --soft
```

Then point your MCP client at `smix-mcp`:

```json
{
  "mcpServers": {
    "smix": {
      "command": "smix-mcp",
      "env": {
        "SMIX_UDID": "<booted-simulator-udid>",
        "SMIX_RUNNER_PORT": "22087"
      }
    }
  }
}
```

One server process binds one simulator. Two simulators means two entries with
different `SMIX_UDID`s.

## A session

What driving actually looks like:

```
smix_launch_app { "bundleId": "com.example.app" }
smix_describe                                        → what's on screen
smix_tap        { "id": "tab-form" }
smix_tap        { "id": "form-email-input" }
smix_fill       { "target": { "id": "form-email-input" }, "text": "alice@example.com" }
smix_press_key  { "key": "return" }
smix_assert_visible { "id": "form-submitted-label" }
```

`smix_describe` first, most of the time. It returns the visible elements and
their ids, which is what you need to write the next call — guessing an id and
getting `ELEMENT_NOT_FOUND` is the slow path.

## Naming an element

Every element-facing tool takes the same shape. Exactly one of:

| | |
|---|---|
| `{ "id": "form-submit-btn" }` | The app's testID. **Prefer this.** |
| `{ "text": "Submit" }` | Visible text, case-insensitive. |
| `{ "label": "Close" }` | Accessibility label, exact. |
| `{ "role": "button", "name": "Reload" }` | Kind, optionally narrowed. |
| `{ "ocrText": "Submit" }` | Read off the pixels. Slowest; last resort. |

Ids survive copy edits and translation. Text does not — a flow written against
`{"text": "Submit"}` breaks when someone changes the button to "Send", and
again in every other locale.

Naming nothing, or naming two, is an error. smix will not pick for you.

`smix_fill` and `smix_scroll` need a selector *and* something else, so theirs
goes under `target`:

```json
{ "target": { "id": "form-email-input" }, "text": "alice@example.com" }
{ "target": { "text": "Sign out" }, "direction": "down" }
```

## Read the failures

They are written to be read back:

```
FAIL [ELEMENT_NOT_FOUND]: no element matched id=form-submit-btn
  suggestions:
    - form-submit-button
  visible elements (top 10):
    - button "Submit" id=form-submit-button
    - textField id=form-email-input
```

The suggestion is the answer: the id has `-button`, not `-btn`. The failure
carries the near-misses and the elements that *were* there, so the next call
can be right rather than another guess.

## Tools

**Look** — `smix_describe` · `smix_tree` · `smix_screenshot` · `smix_find`
**Act** — `smix_tap` · `smix_fill` · `smix_press_key` · `smix_swipe` · `smix_scroll`
**Lifecycle** — `smix_launch_app` · `smix_stop_app`
**Assert** — `smix_assert_visible` · `smix_assert_not_visible`

`smix_find` returns true/false; `smix_assert_visible` fails. Use `find` to
decide what to do next, `assert` when absence is a problem.

`smix_scroll` beats a loop of `smix_swipe` — it knows when to stop.

## Directions

`up` / `down` / `left` / `right` name **what you want to see**, not which way
the finger moves. `down` reveals what is below. Both platforms follow the same
convention, so the same call behaves the same on iOS and Android.

## What is deliberately absent

**The AI-assertion tier.** Whatever calls these tools is already a model, and
it can call `smix_screenshot` and look at the screen itself. Asking a tool to
ask another model about a screen the caller can already see is a detour. The
`assertCondition` verb exists for *yaml* flows, where nothing is watching.

**The rest of the SDK.** `smix-sdk` has dozens of methods. This is a driving
surface, not a mirror.

## See also

- [03-selectors.md](03-selectors.md) — the selector taxonomy in depth
- [07-errors.md](07-errors.md) — the failure format
- [10-ai-assertions.md](10-ai-assertions.md) — the fenced AI tier, for yaml
