# smix-mcp

Drive an iOS Simulator from an agent, over MCP.

This is the surface Claude Code (or any MCP client) gets: launch an app, look
at the screen, tap, type, scroll, assert. One server process binds one
simulator.

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

Bring the runner up first — the server talks to it, it does not start it:

```bash
smix runner up <udid> --soft
```

## Environment

| | |
|---|---|
| `SMIX_UDID` | The simulator to drive. Required for anything that goes through simctl (screenshots, app lifecycle). |
| `SMIX_RUNNER_PORT` | Where the runner is listening. Defaults to `22087`. |

## Tools

**Look**

| | |
|---|---|
| `smix_describe` | The visible elements and their bounds — start here. |
| `smix_tree` | The full accessibility tree, when `describe` isn't enough. |
| `smix_screenshot` | A base64 PNG. |
| `smix_find` | Is this element on screen? true/false, no failure. |

**Act**

| | |
|---|---|
| `smix_tap` | Tap an element. |
| `smix_fill` | Type into a field. |
| `smix_press_key` | Return, Delete, Tab, Space, Escape, arrows. |
| `smix_swipe` | Swipe once. |
| `smix_scroll` | Swipe until an element is in view, then stop. |

**Lifecycle**

| | |
|---|---|
| `smix_launch_app` | Launch, or foreground if already running. |
| `smix_stop_app` | Terminate. |

**Assert**

| | |
|---|---|
| `smix_assert_visible` | Fails, with the visible elements and near-miss suggestions, when the element isn't there. |
| `smix_assert_not_visible` | The inverse. |

## Naming an element

Every tool that touches an element takes the same shape — exactly one of:

```json
{ "id": "form-submit-btn" }
{ "text": "Submit" }
{ "label": "Close" }
{ "role": "button", "name": "Reload" }
{ "ocrText": "Submit" }
```

**Prefer `id`.** It survives copy edits and translation; `text` does not.
Reach for `ocrText` only when the accessibility tree cannot see the element at
all — a canvas, an image of text — because it costs an OCR pass.

Naming nothing, or naming two, is an error rather than a guess.

## When something fails

Failures arrive as a rendered `to_prompt()`: the code, the selector, ranked
near-miss suggestions, and the elements that *were* on screen. Read it — it
usually contains the answer:

```
FAIL [ELEMENT_NOT_FOUND]: no element matched id=form-submit-btn
  suggestions:
    - form-submit-button
  visible elements (top 10):
    - button "Submit" id=form-submit-button
```

That is a typo in the id, and the failure says so.

## What is not here

- **The AI-assertion tier.** Whatever is calling these tools is already a
  model, and it can call `smix_screenshot` and look. Routing that through a
  tool that asks another model about a screen the caller can see would be a
  detour, not a capability.
- **The whole SDK.** This is a driving surface, not a mirror of every method
  `smix-sdk` has.

## See also

- [MCP setup guide](../../docs/ai-guide/11-mcp.md)
- [Selectors](../../docs/ai-guide/03-selectors.md) — the same taxonomy, in depth
- [Errors](../../docs/ai-guide/07-errors.md) — reading a failure

## License

Dual-licensed under either Apache License 2.0 or MIT, at your option.
