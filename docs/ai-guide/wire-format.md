# Wire format — smix v1.0.0

> The wire format between the smix client (`smix-runner-client`) and
> the smix runner (`SmixRunnerServer`) is frozen at v1.0. All shapes
> below are semver-major (breaking change = v2.0). Adding new
> optional fields is allowed within v1.x; renaming or removing
> existing fields is not.

## HTTP transport

Base URL: `http://127.0.0.1:<port>` — port default `22087`, overridable
via `.smix/sims.json` `runnerPort` or the `--runner-port` flag.

All requests use a JSON body; all responses return JSON.

## Request-context headers

Every route accepts these OPTIONAL headers; absent = default behavior
(runner-boot target, no activate, a11y-anchored dispatch):

| Header | Semantics |
|---|---|
| `App-Bundle-Id: <bundle>` | Per-request `XCUIApplication` rebind target |
| `App-Activate: true` | Runner calls `.activate()` on the resolved target before the operation (main-actor hop) |
| `Input-Dispatch-Mode: a11y \| key-events \| auto` | Text input dispatch tier for `/fill` |

## Routes

### `POST /tap`

Body: `{ selector: Selector, include?: IncludeScope }`
Response: `{ ok: bool, tapped: bool }` on success; standard error envelope on failure.

### `GET /tree?include=<scope>`

Response: `A11yNode` tree.

### `POST /find`

Body: `{ selector: Selector, include?: IncludeScope }`
Response: `{ found: bool, exists: bool }` (both fields present; consumers may OR-merge for backwards compatibility).

### `POST /fill`

Body: `{ selector: Selector | "_focused_", text: string, include?: IncludeScope }`
Header: `Input-Dispatch-Mode`
Response: `{ ok: bool, focusMs?: u64, daemonSendMs?: u64 }`

### `POST /clear`

Body: `{ selector: Selector | "_focused_", include?: IncludeScope }`
Response: `{ ok: bool }`

### `POST /press-key`

Body: `{ key: KeyName }` — `KeyName` variants: `enter`, `back`, `home`,
`delete`, `tab`, `escape`, `space`, `up`, `down`, `left`, `right`.
Response: `{ ok: bool }`

### `POST /swipe`

Body: `{ direction: SwipeDirection }` — variants: `up`, `down`, `left`, `right`.
Response: `{ ok: bool }`

### `POST /scroll`

Body: `{ selector: ScrollSelector, direction: SwipeDirection, include?: IncludeScope }`
Response: `{ scrolled: u32 }` — count of scroll gestures dispatched.

### `POST /hide-keyboard`

Body: `{}` — empty.
Response: `{ ok: bool }`

### `POST /back`

Body: `{}` — empty.
Response: `{ ok: bool }`

### `GET /screenshot`

Response: raw PNG bytes (`Content-Type: image/png`).

### `POST /foreground`

Body: `{ bundle: string }` — sends to system `XCUIApplication(bundleId).activate()`.
Response: `{ ok: bool }`

### `GET /health`

Response: `{ ok: true, version: string, uptime_ms: u64 }`

## Error envelope

Failures return HTTP 4xx/5xx with:

```json
{
  "ok": false,
  "error": "<code>",
  "message": "<human-readable>"
}
```

Codes: `not_found`, `snapshot_unavailable`, `app_unavailable`,
`invalid_request`, `runner_error`.

## Wire selector schema

```json
// text-family
{ "text": "Sign In", "modifiers": {...}? }
// id-family
{ "id": "qa-submit", "modifiers": {...}? }
// label / role / anchor / focused
{ "label": "..." }
{ "role": "button", "name": "..." }
{ "anchor": { "text": "..." }, "below": {...} }
{ "focused": true }
```

## Modifier schema (extended selectors)

`{ near, below, above, leftOf, rightOf, inside, ancestor, nth,
first, last }` — see the `smix-selector` crate.

## Include scope

Optional query param `?include=all-windows` extends element resolution
to see-through modal overlays.

## Compatibility promise

- v1.0 client + v1.x runner: compatible
- v1.x client + v1.0 runner: compatible
- v1.0 client + v2.0 runner: not guaranteed

Any breaking change bumps the major version.
