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
| `doubleTapOn` / `doubleTap` | ✅ | ✅ | Android dispatches two clicks 150 ms apart at the resolved point |
| `repeatTap` | ✅ | ⚠️ | iOS packs every touch into one synthesised event, so the interval is the number you state; Android falls back to one request per touch, where the interval is a floor and not a guarantee |
| `longPressOn` / `longPress` | ✅ | ✅ | 500 ms by default (maestro's documented 0.5s, and XCUIElement's press convention); `{ duration: N }` sets it |
| `tapOn: { point: "X%,Y%" }` | ✅ | ✅ | Normalized [0, 1] coordinates; the escape hatch for screens with no a11y semantics. Not a verb of its own — there is no `tapByCoord` |

## Input family

| verb | iOS | Android | notes |
|---|---|---|---|
| `inputText` / `fill` | ✅ | ✅ | `--force-key-events` opt-in bypasses a11y-focus resolution for RN hidden-input patterns |
| `eraseText` / `clear` | ✅ | ✅ | iOS deletes proportionally to the field's own length; Android empties the focused node exactly (`ACTION_SET_TEXT`), falling back to bounded deletes for a field the tree cannot address |
| `pasteText` | ✅ | ❌ | Since Android 10 the clipboard serves only the focused app, and the runner cannot be focused while driving yours. Use `inputText` |
| `setClipboard` | ✅ | ❌ | Same clipboard restriction as `pasteText` |
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
| `assertScreenshot` | ✅ | ✅ | 64-bit dhash over the PNG, so it behaves the same on both. There is no region masking on either platform |

## Control flow

| verb | iOS | Android | notes |
|---|---|---|---|
| `runFlow` | ✅ | ✅ | Path resolution: cwd → `std/` catalogue |
| `runFlow: { when, commands }` | ✅ | ✅ | Inline conditional; `when.visible` / `when.notVisible` gates (mutually exclusive); OCR fires when the gate selector contains `ocrText`; skips emit `SKIPPED: <reason>` to stderr |
| `retry` | ✅ | ✅ | `maxRetries` field; default 3 |
| `repeat` | ✅ | ✅ | |
| `pressKey` | ✅ | ✅ | enter/return, delete, tab, space, escape, and the four arrows on both. home / lock / volumeUp / volumeDown reach Android; on the iOS simulator they report an explicit Skipped (Apple exposes no simulator path) |
| `back` | ✅ | ✅ | Navigation back — iOS nav-bar back / edge swipe, Android KEYCODE_BACK. Not a keystroke: `pressKey: back` is not a spelling of it |

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
| `takeScreenshot` | ✅ | ✅ | Long form with `annotate: [...]` (5 primitives) + auto-mkdir + PNG ext inference |
| `startRecording` | ✅ | ⚠️ | Android records on the device with `screenrecord`, whose `--time-limit` help calls 180 s the maximum, not a default to raise. iOS has no such cap |
| `stopRecording` | ✅ | ✅ | Android interrupts `screenrecord` rather than killing it — the mp4's moov atom is written on interrupt, and without it the file will not play — then pulls the file |
| `addMedia` | ✅ | ✅ | Android pushes to `/sdcard/Pictures/` and fires a media-scan broadcast. Landing the bytes is not enough: a file MediaStore has not indexed is invisible to the app |

## Gesture

| verb | iOS | Android | notes |
|---|---|---|---|
| `scroll` | ✅ | ✅ | |
| `scrollUntilVisible` | ✅ | ✅ | Polls a11y tree between scrolls; selectors containing `ocrText` also probe OCR per stroke |
| `swipe` (`direction:` or `start:`/`end:` or `from:`/`to:`) | ✅ | ✅ | Absolute + relative coord shapes |
| `hideKeyboard` | ✅ | ✅ | |

## Device

| verb | iOS | Android | notes |
|---|---|---|---|
| `openLink` / `openUrl` | ✅ | ✅ | System URL handler |
| `setLocation` | ✅ | ✅ | Android sends `geo fix` on the emulator console. The fix persists and replays when an app starts listening, so setting it early is not a race |
| `travel` | ✅ | ⚠️ | iOS hands the route to CoreSimulator. Android has no route primitive — the emulator console takes one position at a time — so smix walks it from the host, one `geo fix` a second. Both return immediately and travel in the background |
| `setPermissions` | ✅ | ✅ | `pm grant` / `pm revoke` per permission on Android; `simctl privacy` on iOS |
| `setOrientation` | ✅ | ⚠️ | An app that has locked its orientation stays where it is; neither platform reports that as a failure |

## smix-native extensions

These are verbs — write them in a flow.

| verb | iOS | Android | notes |
|---|---|---|---|
| `fixture` | ✅ | ✅ | JSON registry OR TS registry |
| `webview_eval` / `webviewEval` / `webViewEval` | ✅ | ✅ | RN WebView / native WebView bridge |

### Coordinates and OCR are not verbs

Coordinate taps, coordinate swipes and OCR are capabilities, reached
through the verbs and selectors that already exist. They were listed
here as verbs once; a flow that wrote `tapById:` or `tapAtCoord:` got
`unsupported command`.

| capability | iOS | Android | how you write it |
|---|---|---|---|
| tap by id | ✅ | ✅ | `tapOn: { id: "btn" }` — the id path skips OCR and the a11y walk |
| tap at a coordinate | ✅ | ✅ | `tapOn: { point: "50%,80%" }` — normalized 0..1 |
| swipe between coordinates | ✅ | ✅ | `swipe: { from: …, to: … }`; `{ direction: UP }` desugars to the same pair |
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
