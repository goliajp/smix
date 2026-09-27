# Wire format — smix v1.0.0

> The wire format between the smix client (`smix-runner-client`) and
> the smix runner (`SmixRunnerServer`) is frozen at v1.0. All shapes
> below are semver-major (breaking change = v2.0). Adding new
> optional fields is allowed within v1.x; renaming or removing
> existing fields is not.

## HTTP transport

Base URL: `http://127.0.0.1:<port>` — port default `22087`, overridable
via the registry's `runnerPort` or the `--runner-port` flag.

All requests use a JSON body; all responses return JSON.

A client may send a request again only when it cannot have arrived (the
connection was never made), or when it only reads. A `POST` taps, types or
presses, and a runner that took one and did not answer in time may already
have carried it out; sending it again is a second tap, or the text typed
twice. The Rust client fails such a request with `SentWithoutAnswer`
instead of repeating it; a `GET` is asked again.

## Request-context headers

Every route accepts these OPTIONAL headers; absent = default behavior
(runner-boot target, no activate, a11y-anchored dispatch):

| Header | Semantics |
|---|---|
| `App-Bundle-Id: <bundle>` | Per-request `XCUIApplication` rebind target. smix also uses it to ask a question: `GET /tree` with this header answers 200 when that app can be snapshotted and 500 `snapshot_unavailable` when it cannot, which is how `runner up` tells "the runner is answering" from "the app it was asked about is drivable". |
| `App-Activate: true` | Runner calls `.activate()` on the resolved target before the operation (main-actor hop) |
| `Input-Dispatch-Mode: a11y \| key-events \| auto` | How `/fill` puts text in. `a11y` (default) resolves the field and taps it to take focus; `key-events` skips resolution and types into whatever holds focus, for fields the tree cannot address. An unknown value degrades to `auto` rather than failing the step. |


## Routes

### `POST /tap`

Body: `{ selector: { text | id | label: string }, mode?: "resolve" | "resolveAndTap" | "daemonProxySynthesize" }`

Exactly one selector key. `text` matches label or identifier (the historical
behaviour); `id` matches identifier only; `label` matches label only. Regex
patterns, roles and spatial/index modifiers are not accepted here — they need
the full tree and resolve host-side, which is what the default tap path does.
(scope via `?include=` query, same mechanism as `GET /tree`; `mode` defaults to `resolveAndTap`)

Response on success (`TapResult` in `smix-runner-wire`; all fields beyond `ok` optional):

```json
{
  "ok": true,
  "matchedLabel": "Sign In",
  "frame":    { "x": 20.0, "y": 118.5, "w": 353.0, "h": 44.0 },
  "appFrame": { "x": 0.0,  "y": 0.0,   "w": 393.0, "h": 852.0 },
  "stages":   { "resolveMs": 12.5, "tapCallMs": 870.0, "totalMs": 882.5 }
}
```

`frame`/`appFrame` are returned by `mode: "resolve"` so the caller can
inject the tap at the resolved coordinate; `resolveAndTap` performs the
tap in-process and omits them. Miss → `404 { ok: false, error: "not_found", selector, visible: [] }`.

### `GET /tree?include=<scope>&hollow=<package>`

Response: `A11yNode` tree.

`hollow` (Android): every application window of that package is reported
as the window itself — its bounds, its place in the stack, whether it has
the focus — with no children. The host asks for it when the app carries
the semantics probe, whose view of the app replaces that window anyway;
the windows the probe cannot see (the keyboard, the system bars, another
app's dialog) are walked as always. A runner that predates the parameter
ignores it and walks everything.

Node fields of note:

- `rawType` — element type name. iOS: camelCase `XCUIElement.ElementType` name (`"button"`, `"staticText"`, …); Android: full a11y class name (`"android.widget.Button"`, …).
- `elementTypeRaw` (v1.0.22+) — the numeric `XCUIElement.ElementType.rawValue`. **iOS-only signal**; Android payloads omit it and deserialize to the default `1` (`.other`). Triage rule on iOS: `elementTypeRaw != 1 && identifier == "" && label == ""` ⇒ the OS typed the element but the app's a11y bridge dropped its name (app-side issue, not smix).
- `role` — curated semantic role. Android emits it directly (derived from class name); iOS consumers derive it client-side from `rawType`.
- `window` (Android, on each child of the root) — whose window the subtree is: `{ kind, package?, focused, layer, touchable }`, `kind` one of `application` / `inputMethod` / `system` / `other`. `layer` is the window's place in the stack, higher drawn over lower; the children are not listed in screen order. `touchable` (`{x, y, w, h}`, screen pixels) is where the window takes touches, which is not its bounds: the keyboard's window spans the screen below the status bar and takes touches only on the keys. A host aims a touch at the part of an element that no higher window takes touches over — an app drawn edge to edge has content under the status bar, and a touch there goes to the bar.

Response headers (metadata only — body shape unchanged; all additive):

| Header | Since | Meaning |
|---|---|---|
| `X-Tree-Size-Bytes` | v1.2 | Serialized payload size |
| `X-Tree-Node-Count` | v1.2 | Total node count |
| `X-Tree-Snapshot-Refresh-Count` | v1.0.23 (iOS) / v1.0.26 (Android) | Monotonic count of successful `/tree` serves since runner boot — a flat value across polls indicates a stalled snapshot pipeline |
| `X-Tree-Snapshot-Wall-Ms` | v1.0.23 (iOS) / v1.0.26 (Android) | Wall time of this snapshot walk — trending upward across a batch indicates the OS a11y pipeline is bogging down |

### `POST /find`

Body: `{ selector: { text: string }, requireOnScreen?: bool }`

**Text only.** This is the fast path for a simple text lookup; the
runner decodes `selector.text` and nothing else, and refuses any other
selector with `400 bad_request` / `missingText`. Send id, label, role or
any compound selector to `POST /tree` and match there — that is what the
first-party SDKs do. `include` is not read here either.

Response: `{ ok: true, found: bool }`. On `found:false` the runner may
add `diagnostics: { appState, candidates, rebound }` — advisory only,
and absent on the happy path so a client parsing the two-field shape
never meets a new field when the query worked.

`exists` is a **historical alias that no current runner emits**. The
first-party client still accepts it on input for old runners; do not
read it from a response, and do not OR-merge the two — a response has
`found` and only `found`.

`requireOnScreen: true` (v1.0.27) — `found` additionally requires the LIVE element frame to intersect the app frame. Snapshot frames drift on iOS 26.5 + RN Fabric for below-the-fold elements; the live query re-resolves current layout. The driver's visibility-semantic paths (`wait_for` / `find` / scroll probes) use this so `extendedWaitUntil` / `scrollUntilVisible` / `tapOn` agree on "visible". Deliberately checks frame∩viewport rather than `isHittable` — hittability is false under floating overlays, which are genuinely visible and assertable. `isHittable` is the only z-order-aware signal XCUITest offers, and that false-under-overlay behaviour is exactly why it stays rejected here.

### `POST /fill`

Body: `{ selector: Selector | "_focused_", text: string, clearFirst?: bool, include?: IncludeScope }`
Header: `Input-Dispatch-Mode`
Response: `{ ok: bool, focusMs?: u64, daemonSendMs?: u64 }`

`clearFirst` empties the field before typing, and is **true when
absent** — typing appends, so a route named `fill` that did not clear
concatenated old and new values. A runner too old to know the field
appends, which is what it did before, so the field is additive on the
wire. The host driver chunks long text into one POST per character;
`clearFirst` rides the first chunk alone.

`key-events` is for RN apps whose hidden `<TextInput>` defeats
a11y-focus lookup. On iOS the header reaches the runner, which skips
its focus-tap; on Android there is no header to send — `/input-text`
already types into the focused field — so the host driver honours the
mode by not resolving. Different mechanics, same guarantee.

### `POST /clear`

Body: `{ selector: Selector | "_focused_", include?: IncludeScope }`
Response: `{ ok: bool }`

iOS only. The Android runner has `/clear-text` instead, because the
work is different: there is no selector to resolve runner-side, and
the host has already tapped the field to focus it.

### `GET /windows` (Android)

Response: `{ status: "ok", count: int, windows: [ { index, type, layer,
active, focused, rootReadable, package } ] }`

`/tree` answers what is on screen; this answers why something is not.
A window whose root cannot be read is skipped by the tree walk, and a
window that was never attached is skipped too — different problems,
identical symptom, and until this route there was no way to look. The
tree's root object also carries `unreadableWindows`, a count of the
first kind.

```bash
curl -s http://localhost:28080/windows | jq '.windows[] | {package, active, rootReadable}'
```

If the app you are driving has no row at all, its window is not
attached for accessibility. If it has a row with `rootReadable: false`,
it is attached and smix cannot read it. The two want different fixes.

### `POST /clear-text` (Android)

Body: `{ focusRect?: [nx, ny, nw, nh] }` — the focused field is the target
(the one inside `focusRect` when given); the host focuses it first.
Response: `{ ok: bool, status: "ok" | "field_not_empty" | "no_focused_field",
method: "set-text" | "key-events", deletes: int, held: int }`

`ok` is whether the field is empty when the route looks again. `held` is
how many characters it still holds, and -1 when no focused field could be
found to ask — `no_focused_field`, which is not the same answer as a field
with something left in it. The whole wait for that second look is bounded
by the route's own budget (about 8 s when nothing takes focus).

`method` is not decoration. `set-text` empties the field through the
focused node's `ACTION_SET_TEXT` and is exact at any length.
`key-events` is the fallback for a field the accessibility tree cannot
address: it sends a bounded number of deletes, so a longer field
survives it partly filled, and a caller that cannot tell the two apart
cannot know which answer it got.

This replaced fifty `/press-key delete` posts from the host — fifty
sequential round trips over the adb forward, on every fill once fill
began clearing first, and still wrong past fifty characters.

### `POST /input-text` (Android)

Body: `{ text: string, focusRect?: [nx, ny, nw, nh], budgetMs?: number }`
Response: `{ ok: bool, status: "ok" | "text_did_not_land", text, before, held, chunks, retyped }`
— for a masked field `beforeLength` and `heldLength` in place of `before`
and `held`.

Types into the focused field (the one inside `focusRect` when given) and
reads it back. `ok` means the field holds what it held before with `text`
put in once, at the caret. A field holding characters this request did
not type is not `ok`, and neither is one missing some. A masked field
cannot be read, so its length is the whole of the judgement, and its
content never goes on the wire.

The text goes in 32 characters at a time, each chunk read back before
the next: one long `input text` drops characters under load. A chunk
whose end did not arrive has only its missing part typed again, up to
three times; `chunks` is how many there were and `retyped` how many of
those repairs it took. A chunk missing characters from its middle, or
holding one nobody typed, is not repaired — the failure names the
chunk and what the field held.

Typing costs time per character, so the host waits longer for a longer
text and tells the runner how long in `budgetMs`. The runner checks it
before each `input text` it sends; once it is spent it types nothing
more and answers `{ error: "text_budget_spent", message }`, the message
naming the chunk it stopped before and how many characters the field
holds. Without `budgetMs` the typing is not bounded.

`input text` goes wherever focus is, not to a node, so the field is read
back as the node that was typed into, and a repair is typed only while
that node still has focus. A field can leave the screen as it fills — a
code field that submits itself when full. When that happens after the
last chunk was sent, the answer is `{ ok: true, status: "field_left",
readBack: "unread", text, before, lastRead, chunks }`: the app took the
text and moved on, and there is nothing left to read. When it happens
with text still to send, or a short chunk's field has lost focus, the
runner types nothing more and answers `{ error: "field_left" }` or
`{ error: "focus_left" }`, the message naming the chunk and what the
field held when last read.

### `POST /press-key`

Body: `{ key: KeyName }` — the wire names: `return`, `delete`, `tab`,
`space`, `escape`, `arrowUp`, `arrowDown`, `arrowLeft`, `arrowRight`,
`home`, `lock`, `volumeUp`, `volumeDown`.
Response: `{ ok: bool }`

A key the device has no button for is refused by name:
`{ ok: false, error: "no_such_button", saw: "<why, and what to do instead>" }`,
with status 200. The iOS runner answers this for `lock`, `volumeUp` and
`volumeDown`; the Android runner presses all three.

`back` is a `KeyName` too, and never reaches this route: a client sends
it to `POST /back`, which answers whether anything went back. A key
press answers only that the event went in.

### `POST /tap-at-norm-coord`

Body: `{ nx, ny, times?, intervalMs?, holdMs?, doubleTap?, aimedBy? }` —
a point as shares of the app frame, and optionally a burst (`times`
touches `intervalMs` apart) or a held touch (`holdMs`). A tap, a double
tap (`times: 2, doubleTap: true`) and a long press (`holdMs: duration`)
are all this route. On iOS the touches of a burst are delivered one at
a time on their schedule, no closer than one delivery (about 280 ms);
`doubleTap` sends both in one event instead, close enough together to be
one gesture. The Android runner does not read `doubleTap` (its double
tap is `/double-tap-at-norm-coord`).
`aimedBy` is the tree the point was aimed from, `accessibility` or
`semantics`; absent means `accessibility`.

Response: `{ ok, chain: [{ identifier, label, frame }], complete?,
reader?, readerError?, x?, y?, latestDownOffsetMs?, earliestUpOffsetMs?,
handlerWallMs? }`. `chain` is what was under the point, innermost first,
read from the tree `aimedBy` named — `reader` says which. A runner that
cannot read that tree answers with no chain and `readerError`, and never
with the other tree's chain in its place: the host judges the tap
against the tree it aimed from, and a chain from another is a verdict on
some other aim. A runner with one tree (iOS) sends no `reader`, and its
chain is its accessibility tree's. `x`/`y` are the point in the
display's pixels. The three offsets come with a single touch and bound
when it was down, measured from the handler's entry — bounds, not
instants.

The Android runner reads `times`, `intervalMs` and `holdMs` on this route
as the iOS runner does: the point is resolved once and touched `times`
times. The Android runner's `/double-tap-at-norm-coord` and
`/long-press-at-norm-coord` take `aimedBy` and answer `reader` /
`readerError` the same way.

### `POST /swipe`

Body: `{ direction: SwipeDirection }` — variants: `up`, `down`, `left`, `right`.
Response: `{ ok: bool }`

### `POST /scroll` — retired

Gone as of 10.2. Scrolling to an element is one loop on the host: it
reads the tree, decides whether enough of the target can be seen and
whether it has stopped moving, and sends `/swipe-once` until it has.
The runner-side loop this route drove was a second implementation of
the same thing, with its own idea of "visible", and nothing had called
it since the host's three loops were unified — a route that exists and
that nobody walks reads, to anyone writing a client, like a path.

### `POST /hide-keyboard`

Body: `{ budgetMs?: number }`
Response: `{ ok: bool, error?: string, saw?: string }`

`budgetMs` is how long the host will wait. The iOS runner starts no
dismissal strategy once it is spent and answers with what it has; the
host sends 20000 and waits that long plus a margin. Without it the
dismissal is not bounded. `ok:false` carries `error`:
`keyboard_did_not_close` (the keyboard was there and is still there —
`saw` lists the strategies tried, and says so when the budget ran out
first) or `keyboard_state_unknown` (looking failed; not evidence the
keyboard is up). The Android runner ignores `budgetMs`.

### `POST /back`

Body: `{}` — empty.
Response: `{ ok: bool, settledBy?: string, saw?: string }`

`ok:false` here means the runner tapped, gestured, and did not observe
the screen change within its budget. It is a statement about what was
observed, not a proof that nothing moved — treat it as a refusal to
confirm rather than as evidence of a stuck screen.

`settledBy` names which branch decided: `titleChanged` (the navigation
bar's identifier moved), `sustainedAbsence` (no bar for long enough to
mean the destination has none), `noIdentity` (there was no title to
watch, so a fixed settle was used and nothing was verified), or
`gaveUp`. `saw` accompanies a refusal and carries the readings behind
it — whether a back button was there, the title before, the last title
read, and how many consecutive frames had no bar. Both fields are
diagnostic: an implementation may ignore them, and neither changes `ok`.

### `GET /screenshot`

Response: raw PNG bytes (`Content-Type: image/png`). A 503 means
`XCUIScreen` produced nothing, which is not the same as a blank screen —
it carries `error` and `reason`, and callers are expected to say so
rather than hand back an empty image.

Both platforms serve it, and `tap --then-screenshot` takes its frame
from it on both: the picture comes from the process that just acted.
The Android runner served no such route until 10.2, so a runner older
than that answers `501 not_implemented` — which callers are expected to
report as "this runner predates the route, bring it up again" rather
than quietly photographing the screen with something else.

### `POST /foreground`

Body: `{ bundle: string }` — sends to system `XCUIApplication(bundleId).activate()`.
Response: `{ ok: bool }`

### `GET /system-popups`

Response: `{ popups: [{ id, type, source, title, body, buttons: [{ id,
label, role, dangerous, outcomeHint? }] }] }`. `source` is who owns the
window — a bundle id on iOS, a package name on Android.

A popup is a window that **belongs to somebody other than the app under
test, took the focus, and carries something a caller can press**. All
three matter, and each was learned from a window that broke one of
them:

- The navigation bar carries four clickable, named buttons (Back, Home,
  Overview, Switch input method) and is over every app at all times. It
  never takes the focus, and it is not a popup.
- The keyboard is never a popup whatever is drawn on it. Its verb is
  `hideKeyboard`.
- An app's own dialog is answered by `/tree`, where the flow's own
  selectors can name its buttons.

Which app is "the app under test" comes from the `App-Bundle-Id` header
every smix client sends, or `?app=<pkg>`. **A caller who sends neither
gets every focused window carrying buttons, their own dialogs
included** — the alternative was guessing, and the guess that was tried
first ("the focused application window") took the permission dialog for
the app and reported nothing at all.

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

`no_focused_field` carries two more fields, from one walk of the window
stack: the `message` names any foreign window holding the focus, and
`windows` is the whole stack as `[{ package, type, kind, layer }]`. A
fill that finds no field is most often a fill with something else on
top, and the sentence that used to come back described only the focus.
When the app under test has no window in the stack at all, the message
says that too.

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
