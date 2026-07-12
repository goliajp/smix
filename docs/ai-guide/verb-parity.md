# smix verb parity — cross-platform + tier

> Auditor-facing table of smix YAML verb support across iOS Simulator + Android emulator. The canonical source is `smix_verbs::VERB_TABLE`.

## Tier legend

- ✅ — fully supported
- ⚠️ — supported with caveats (documented per-row)
- ❌ — not supported on this platform in v1.0

Version at freeze: v1.0.26.

## Tap family

| verb | iOS | Android | notes |
|---|---|---|---|
| `tapOn` / `tap` | ✅ | ✅ | Selectors resolved via a11y tree; native tap dispatch; `fallback:` chains containing `ocrText` poll for `SMIX_TAP_OCR_POLL_MS` (default 3000 ms) |
| `doubleTapOn` / `doubleTap` | ✅ | ⚠️ | Android uses an IME long-press-then-release approximation |
| `longPressOn` / `longPress` | ✅ | ✅ | Duration = 700ms default; not configurable in v1.0 |
| `tapByCoord` | ✅ | ✅ | Normalized [0, 1] coordinates; escape hatch for non-a11y-semantic tests |

## Input family

| verb | iOS | Android | notes |
|---|---|---|---|
| `inputText` / `fill` | ✅ | ✅ | `--force-key-events` opt-in bypasses a11y-focus resolution for RN hidden-input patterns |
| `eraseText` / `clear` | ✅ | ✅ | Chunked deletes N chars |
| `pasteText` | ✅ | ⚠️ | Android uses `adb shell input keyevent PASTE`; per-app behavior varies |
| `setClipboard` | ✅ | ✅ | |
| `copyTextFrom` | ✅ | ⚠️ | Android limitation: reads via `copy_from_focused` API, some system fields blocked |

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
| `assertScreenshot` | ✅ | ⚠️ | Android: dhash comparison works; region masking is limited |

## Control flow

| verb | iOS | Android | notes |
|---|---|---|---|
| `runFlow` | ✅ | ✅ | Path resolution: cwd → `std/` catalogue |
| `runFlow: { when, commands }` | ✅ | ✅ | Inline conditional; `when.visible` / `when.notVisible` gates (mutually exclusive); OCR fires when the gate selector contains `ocrText`; skips emit `SKIPPED: <reason>` to stderr |
| `retry` | ✅ | ✅ | `maxRetries` field; default 3 |
| `repeat` | ✅ | ✅ | |
| `pressKey` | ✅ | ✅ | Names: enter, back, home, escape, delete, tab, etc. |
| `back` | ✅ | ✅ | Alias for `pressKey: back` |

## Lifecycle

| verb | iOS | Android | notes |
|---|---|---|---|
| `launchApp` | ✅ | ✅ | `clearState`, `clearKeychain`, `arguments`, `permissions` |
| `stopApp` / `terminate` | ✅ | ✅ | |
| `killApp` | ✅ | ✅ | |
| `clearState` / `reset` | ✅ | ✅ | |
| `clearKeychain` / `resetKeychain` | ✅ | ⚠️ | Android has no keychain; no-op with warning |
| `clearUserDefaults` | ✅ | ❌ | v1.0.27 — per-key NSUserDefaults deletion via `simctl spawn defaults delete`; Android SharedPreferences has no host-side per-key path (explicit error; use `clearAppData` for a full wipe) |

## Media

| verb | iOS | Android | notes |
|---|---|---|---|
| `takeScreenshot` | ✅ | ✅ | Long form with `annotate: [...]` (5 primitives) + auto-mkdir + PNG ext inference |
| `startRecording` | ✅ | ⚠️ | Android via `adb shell screenrecord`; 3-minute hard limit |
| `stopRecording` | ✅ | ✅ | |
| `addMedia` | ✅ | ⚠️ | iOS via Photos permission grant; Android via `am start` intent |

## Gesture

| verb | iOS | Android | notes |
|---|---|---|---|
| `scroll` | ✅ | ✅ | |
| `scrollUntilVisible` | ✅ | ✅ | Polls a11y tree between scrolls; selectors containing `ocrText` also probe OCR per stroke |
| `swipe` | ✅ | ✅ | Absolute + relative coord shapes |
| `hideKeyboard` | ✅ | ✅ | |

## Device

| verb | iOS | Android | notes |
|---|---|---|---|
| `openLink` / `openUrl` | ✅ | ✅ | System URL handler |
| `setLocation` | ✅ | ⚠️ | Android via `am start LocationSpoofer`; not all runtimes support |
| `travel` | ✅ | ⚠️ | Same as `setLocation` with route |
| `setPermissions` | ✅ | ⚠️ | Android per-permission; iOS via `simctl privacy` |
| `setOrientation` | ✅ | ⚠️ | Android may fail silently on locked-orientation apps |
| `toggleAirplaneMode` | ✅ | ⚠️ | Android needs system permission |

## smix-native extensions

| verb | iOS | Android | notes |
|---|---|---|---|
| `tapById` | ✅ | ✅ | Fast path — no OCR / a11y walk |
| `tapAtCoord` | ✅ | ✅ | Normalized coords |
| `swipeAtCoord` | ✅ | ✅ | |
| `ocrText` | ✅ | ✅ | Vision framework (iOS) / ML Kit (Android) |
| `anchorRelative` | ✅ | ✅ | Selector-relative anchoring |
| `findTextByOcr` | ✅ | ✅ | |
| `fixture` | ✅ | ✅ | JSON registry OR TS registry |
| `webview_eval` / `webviewEval` | ✅ | ✅ | RN WebView / native WebView bridge |

## Utility

| verb | iOS | Android | notes |
|---|---|---|---|
| `waitForAnimationToEnd` | ✅ | ✅ | Fixed sleep on BOTH platforms (bare = 400 ms; `: N` or `{ timeout: N }` = N ms). Not an animation-idle signal anywhere |
| `evalScript` | ⚠️ | ⚠️ | JS eval via the app's debug bridge (consumer wires) |
| `runScript` | ⚠️ | ⚠️ | Sibling of `evalScript` |

## v1.0 non-goals

Explicit — not supported in v1.0:

- `fillAtCoord` — deferred to a future minor release
- Real-device support (iOS simulator + Android emulator only)
- Cross-platform log signal syntax unification (each platform's log tail is separate)
- Full swc-based TS parser for fixture registry (the lightweight parser covers common cases)
