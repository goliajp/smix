# smix verb parity — cross-platform + tier

> What each smix YAML verb does on the iOS Simulator and on the Android
> emulator. `smix_verbs::VERB_TABLE` lists the verbs; this page says what
> they do, and each row was checked against the code that runs it.

## Tier legend

- ✅ — supported
- ⚠️ — supported, with the caveat stated on the row
- ❌ — not supported; the row says what to do instead

A verb marked ❌ on a platform **returns an error there**. None of them
succeed quietly: a flow that clears the keychain does it so the next step
meets a signed-out app, and reporting success without doing it hands that
step a signed-in one and blames the step.

### Where the two platforms differ underneath

Selectors resolve in different places. On iOS the runner resolves them and
acts in one call (`/find`, `/tap`, `/fill`). On Android the host resolves
against the tree and acts by coordinate (`/tap-at-norm-coord`,
`/input-text`). The verbs behave the same; the route lists do not match, and
that is why.

## Tap family

| verb | iOS | Android | notes |
|---|---|---|---|
| `tapOn` / `tap` | ✅ | ✅ | Selectors resolved via a11y tree; native tap dispatch; `fallback:` chains containing `ocrText` poll for `SMIX_TAP_OCR_POLL_MS` (default 3000 ms) |
| `doubleTapOn` / `doubleTap` | ✅ | ✅ | Resolved on the host and judged like `tapOn`: a double tap delivered to something else fails `TAP_MISSED`. iOS sends both touches in one synthesised event, 80 ms apart; Android two clicks 150 ms apart |
| `repeatTap` | ✅ | ⚠️ | iOS packs every touch into one synthesised event, so the interval is the number you state; Android falls back to one request per touch, where the interval is a floor and not a guarantee |
| `longPressOn` / `longPress` | ✅ | ✅ | 500 ms by default (maestro's documented 0.5s); `{ duration: N }` sets it. Resolved on the host and judged like `tapOn` on both platforms |
| `tapOn: { point: "X%,Y%" }` | ✅ | ✅ | Normalized [0, 1] coordinates; the escape hatch for screens with no a11y semantics. Not a verb of its own — there is no `tapByCoord` |

## Input family

| verb | iOS | Android | notes |
|---|---|---|---|
| `inputText` / `fill` | ✅ | ✅ | `--force-key-events` opt-in bypasses a11y-focus resolution for RN hidden-input patterns |
| `eraseText` / `clear` | ✅ | ✅ | iOS deletes proportionally to the field's own length; Android empties the focused node exactly (`ACTION_SET_TEXT`), falling back to bounded deletes for a field the tree cannot address |
| `pasteText` | ✅ | ❌ | Since Android 10 the clipboard serves only the focused app, and the runner cannot be focused while driving yours. Use `inputText` |
| `setClipboard` | ✅ | ❌ | Same clipboard restriction as `pasteText`. On a registered physical iPhone it needs Xcode 27 (`devicectl device pasteboard`) |
| `copyTextFrom` | ✅ | ❌ | Same clipboard restriction as `pasteText`. Assert on what the app renders instead |

## Assert family

| verb | iOS | Android | notes |
|---|---|---|---|
| `assertVisible` / `expect` | ✅ | ✅ | Visibility check via a11y tree bounds + visible flag |
| `assertNotVisible` / `expectNotVisible` | ✅ | ✅ | |
| `extendedWaitUntil` | ✅ | ✅ | `timeout` field; polls at 250 ms; `ocrText` in `fallback:` fires OCR per iteration; auto-captures screenshot + tree JSON to `.smix/timeouts/` on timeout |
| `expect: { signal }` | ✅ | ✅ | Metro log signal; consumer configures `.smix/config.json` metroLog |
| `expect: { signals }` | ✅ | ✅ | Ordered / any-order variants |
| `expectLogClean` | ✅ | ✅ | Allowlist multi-source merge |
| `assertTrue` | ✅ | ✅ | Expression engine — `${output.name}`, `${env.NAME}`, arithmetic |
| `assertScreenshot` | ✅ | ✅ | Two comparisons, the same on both: `threshold` (default 5) is a 64-bit dhash distance; `thresholdPercentage` is maestro's share of matching pixels (RGB within 10%), and a size mismatch fails. Writing both is an error. **Differs from maestro**: a flow that writes neither compares by hash, where maestro compares pixels at 95. `cropOn` compares one element's region and the baseline is the cropped image. `mask:` regions count neither way; masks covering everything are refused. `label` / `optional` refused by name |
| `rememberBounds` | ✅ | ✅ | smix's own — maestro has no verb for it. Keeps where an element is, under a name, in device-independent pixels (points on iOS; pixels ÷ density on Android). Takes any selector except `ocrText` and `anchorRelative`, which name no box to measure |
| `assertBoundsUnchanged` | ✅ | ✅ | smix's own — maestro has no verb for it. The element's box now matches the one `rememberBounds` kept under `was`, every edge within `within` device-independent pixels (default 0). A failure prints both boxes and how far each edge moved |
| `neverVisible` | ✅ | ✅ | smix's own — maestro has no verb for it. Runs the steps under `during` and, beside them, keeps asking the question `assertNotVisible` asks, as fast as the device answers; one sighting fails it, with the time since the span began and the inner step that was running. A pass says how many times it looked and the longest stretch nobody was looking. Refuses `ocrText` and `anchorRelative` for the same reason `assertNotVisible` does |

## Control flow

| verb | iOS | Android | notes |
|---|---|---|---|
| `runFlow` | ✅ | ✅ | Path resolution: cwd → `std/` catalogue |
| `runFlow: { when, commands }` | ✅ | ✅ | Inline conditional; `when` takes `platform` / `true` / `visible` / `notVisible` / `label`, combined with AND in maestro's order; OCR fires when a gate selector contains `ocrText`; `env` / `label` / `optional` on the block; unknown keys are parse errors; skips emit `SKIPPED: <reason>` to stderr |
| `retry` | ✅ | ✅ | `maxRetries` field; default 3 |
| `repeat` | ✅ | ✅ | `while:` takes the same conditions as `runFlow.when`; `label` / `optional` on the block |
| `pressKey` | ✅ | ✅ | One key table for the flow, `smix press-key` and MCP, read when the flow is read: maestro's spellings, the wire names, shorthands. enter/return, delete, tab, space, escape, and the four arrows on both. `back` is the `back` verb below. lock / volumeUp / volumeDown are pressed on Android (`KEYCODE_POWER` and the volume keys); on iOS the step fails with `no_such_button` and the reason — no lock button in XCUIDevice, no volume buttons on the simulator. maestro's TV remote keys are refused by name; `Power` points to `lock` |
| `back` | ✅ | ✅ | Navigation back — iOS nav-bar back / edge swipe, Android KEYCODE_BACK. `pressKey: back` and `smix press-key back` are this, not a keystroke. Closes a system share sheet under gesture navigation, where there is no back button to tap. Both platforms answer whether the screen changed, not whether the key was delivered, and both report which reading decided (`settledBy`); a back an app swallows is a failure |

## Lifecycle

| verb | iOS | Android | notes |
|---|---|---|---|
| `launchApp` | ✅ | ✅ | `clearState`, `clearKeychain`, `arguments`, `permissions` |
| `stopApp` / `terminate` | ✅ | ✅ | |
| `killApp` | ✅ | ✅ | |
| `clearState` / `reset` | ✅ | ⚠️ | Android clears via `pm clear`, which also reverts the app's runtime permissions — app data is app-private, so the host has no way to wipe one without the other. iOS clears the sandbox and privacy separately |
| `clearKeychain` / `resetKeychain` | ✅ | ❌ | Credentials live in each app's own KeyStore, out of the host's reach. Use `clearState` (a full `pm clear`), or have the app expose a sign-out path — `clearAppData` also errors on Android |
| `clearUserDefaults` | ✅ | ❌ | v1.0.27 — per-key NSUserDefaults deletion via `simctl spawn defaults delete`; Android SharedPreferences has no host-side per-key path (explicit error; use `clearState` for a full wipe — `clearAppData` is iOS-only) |

## Media

| verb | iOS | Android | notes |
|---|---|---|---|
| `takeScreenshot` | ✅ | ✅ | Long form with `annotate: [...]` (5 primitives) + auto-mkdir + PNG ext inference; `cropOn:` writes only that element (a baseline for `assertScreenshot` `cropOn`). Other keys, `label` / `optional` included, are refused by name |
| `startRecording` | ✅ | ⚠️ | Android records on the device with `screenrecord`, whose `--time-limit` help calls 180 s the maximum, not a default to raise. iOS has no such cap |
| `stopRecording` | ✅ | ✅ | Android interrupts `screenrecord` rather than killing it — the mp4's moov atom is written on interrupt, and without it the file will not play — then pulls the file |
| `addMedia` | ✅ | ✅ | Android pushes to `/sdcard/Pictures/` and fires a media-scan broadcast. Landing the bytes is not enough: a file MediaStore has not indexed is invisible to the app |

## Gesture

| verb | iOS | Android | notes |
|---|---|---|---|
| `scroll` | ✅ | ✅ | |
| `scrollUntilVisible` | ✅ | ✅ | One host-side loop on both: swipe, look (tree, then each `ocrText`), stop when the element is wholly on screen and has stopped moving. `visibilityPercentage` / `centerElement` / `timeout` / `label` / `optional` read; `speed` / `waitToSettleTimeoutMs` refused by name |
| `swipe` (`direction:` or `start:`/`end:` or `from:`/`to:`) | ✅ | ✅ | Absolute + relative coord shapes |
| `hideKeyboard` | ✅ | ✅ | |

## Device

| verb | iOS | Android | notes |
|---|---|---|---|
| `openLink` / `openUrl` | ✅ | ✅ | System URL handler |
| `setLocation` | ✅ | ✅ | Android sends `geo fix` on the emulator console. The fix persists and replays when an app starts listening, so setting it early is not a race. On a registered physical iPhone it needs Xcode 27, and the location stays until `xcrun devicectl device simulate location clear --device <UDID>` |
| `travel` | ✅ | ⚠️ | iOS hands the route to CoreSimulator. Android has no route primitive — the emulator console takes one position at a time — so smix walks it from the host, one `geo fix` a second. Both return immediately and travel in the background. A registered physical iPhone takes the route through `devicectl` (Xcode 27), with the same caveat as `setLocation` |
| `setPermissions` | ✅ | ✅ | `pm grant` / `pm revoke` per permission on Android; `simctl privacy` on iOS |
| `setOrientation` | ✅ | ⚠️ | An app that has locked its orientation stays where it is; neither platform reports that as a failure. Android reads the display's rotation back before answering, so a rotation that does not arrive is a failure rather than a silent no-op |

## smix-native extensions

These are verbs — write them in a flow.

| verb | iOS | Android | notes |
|---|---|---|---|
| `fixture` | ✅ | ✅ | JSON registry OR TS registry |
| `webview_eval` / `webviewEval` / `webViewEval` | ✅ | ✅ | RN WebView / native WebView bridge |
| `clearLocation` | ✅ | ⚠️ | The way back from `setLocation` / `travel`, which outlive the flow that ran them — maestro has no verb for it. iOS clears through `simctl location clear`, a registered iPhone through `devicectl device simulate location clear`. On Android it stops the route smix is walking and leaves the device where it stands: the emulator console has no inverse of `geo fix`, and an emulator has no real position to be given back |

### Coordinates and OCR are not verbs

Coordinate taps, coordinate swipes and OCR are capabilities, reached
through the verbs and selectors that already exist. They were listed
here as verbs once; a flow that wrote `tapById:` or `tapAtCoord:` got
`unsupported command`.

| capability | iOS | Android | how you write it |
|---|---|---|---|
| tap by id | ✅ | ✅ | `tapOn: { id: "btn" }` — the id path skips OCR and the a11y walk |
| tap at a coordinate | ✅ | ✅ | `tapOn: { point: "50%,80%" }` — normalized 0..1 |
| swipe between coordinates | ✅ | ✅ | `swipe: { from: …, to: … }`; on the CLI `smix swipe --from 50%,80% --to 50%,20%`, and `swipe_from` / `swipe_to` through MCP |
| find text by OCR | ✅ | ✅ | the `ocrText` selector, below — the `find_text_by_ocr` wire route has no verb of its own |

### Selector forms

Written inside a selector, not as a step.

| form | iOS | Android | notes |
|---|---|---|---|
| `ocrText` | ✅ | ✅ | Vision framework (iOS) / ML Kit (Android) |
| `anchored` (alias `anchorRelative`) | ✅ | ✅ | Selector-relative anchoring |

## Utility

| verb | iOS | Android | notes |
|---|---|---|---|
| `waitForAnimationToEnd` | ✅ | ✅ | Bare form compares frames until the screen holds still. A screen that never settles — a spinner, a caret — is not a failure; the wait just ends at its ceiling. `: N` / `{ timeout: N }` sets that ceiling |
| `evalScript` | ❌ | ❌ | Errors unconditionally on both platforms ("a complete JS runtime is not supported") with an `assertTrue` pointer. No debug-bridge path exists |
| `runScript` | ❌ | ❌ | Sibling of `evalScript`; same unconditional error |
| `clearAppData` | ✅ | ❌ | iOS session-scoped in-place wipe (cooperative terminate → sandbox rm → relaunch). Android errors — use `clearState` |
| `resetAppData` | ✅ | ✅ | App-owned URL-scheme wipe via `openurl` / `am start VIEW`; `waitFor.logLinePattern` needs `--metro-log` on both |
| `assertCondition` | ✅ | ✅ | Host-side AI judge over a screenshot (local `claude` CLI); platform-independent |
| `extractWithAI` | ✅ | ✅ | Same host-side AI lane, writes into `output.*` |

## Names in the table that are not verbs

There are none. Eleven rows in `VERB_TABLE` once named things the parser
never dispatched, and every one is settled: ten were deleted — `ocrText` and
`anchorRelative` are selector fields, `tapAtCoord` is `tapOn: {point}`,
`tapById` is `tapOn: {id}`, `toggleAirplaneMode` was implemented nowhere —
and `back` now parses directly.

Deleting those rows is what made `doubleTap` and `longPress` start working:
a row whose maestro and smix names are identical shadows the alias when the
parser normalizes a verb, so the name never reached the lookup that would
have mapped it onto `doubleTapOn`. The row promising the verb was what
stopped it.

A test in the adapter reads the parser's dispatch out of the source and
compares it with the table in both directions, so neither can drift from the
other in silence.

## Not supported

- `fillAtCoord` — no coordinate escape hatch for typing; `tapAtCoord` is the
  only one, by design
- Real devices — the simulator and the emulator only
- One log-signal syntax across platforms — each platform's log tail is read
  on its own terms
