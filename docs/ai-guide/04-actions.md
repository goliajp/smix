# 04 — Actions

> Every action verb (tap / fill / swipe / scroll / press-key / etc.), the route it takes under the hood, and when a tap needs a different one.

## Action mental model

```
yaml verb (tapOn) → smix-adapter (translation) → Driver trait method
                                                  ├─ IosDriver     → host resolve → /tap-at-norm-coord
                                                  │                  dispatch: xcui        → /tap-by-id
                                                  │                  dispatch: daemonProxy → /tap
                                                  └─ AndroidDriver → host resolve → /tap-at-norm-coord
                                                                     /tap-by-id (view-id anchored)
```

The YAML is platform-agnostic. The driver picks the right strategy for the target platform.

## Tap

### Default tap

```yaml
- tapOn:
    id: "home-increment-btn"
```

**iOS dispatch**: the host fetches the a11y tree, resolves the selector
against it, and sends the element's centre to `/tap-at-norm-coord`,
which taps via `coordinate(withNormalizedOffset:).tap()` — the Apple
native UI event chain. This is the only route a `tapOn` without
`dispatch:` takes; there is no fallback to another one, and having an
`accessibilityIdentifier` does not change it.

**Android dispatch**: the same host-side resolve, then the Kotlin
runner's `/tap-at-norm-coord`. A view-id anchored tap
(`findAccessibilityNodeInfosByViewId` → centre → native synthesize) is
what `dispatch: xcui` reaches, mirroring the iOS route of that name.

The two other routes exist because two specific runtimes need them, and
both are opt-in — see the next section.

**What a successful `tapOn` means.** The runner reports every named
element containing the point it aimed at, and the step fails with
`TAP_MISSED` if the element you named is not among them. So success
means **the aim was inside your target**, judged against the
accessibility snapshot — a stronger statement than "a touch was
synthesised somewhere", and a weaker one than "your element was
touched".

The gap between those two is not hypothetical. The comparison happens
entirely in the coordinate space the snapshot describes; the
synthesised event is read in whatever space it is stamped with, and
on a landscape screen those are not the same space —
every tap reports the button it aimed at and the screen does not
change. `smix` refuses the tap outright when it can see the two
disagree, but the general shape of the limit stands: this line is
evidence about aim.

It does **not** mean the target received the touch. Something drawn
over your element contains the same point, and the a11y snapshot
carries no z-order, so smix cannot tell which one is on top. This is a
limit of the platform, not an omission: the snapshot is a dead frame
with no frontmost or occlusion field on it, and the only z-order-aware
signal XCUITest offers is `isHittable`, which reports false under
floating overlays that are genuinely visible and assertable. If a tap
reports success and nothing happens, see `07-errors.md` →
"tap returns `ok: true` but state doesn't change".

### Tap with explicit dispatch (v1.0.26)

The default tap path (host-resolve → IOHID native-event synthesize) fires SwiftUI `onTap` and RN Pressable `onPress` reliably. Two runtime-specific cases need a different mechanism — declare it per tap:

```yaml
# SwiftUI .sheet / .alert / .confirmationDialog / .fullScreenCover
# dismiss BINDINGS don't flip from coord-based taps on iOS 17+; the
# XCUIElement-anchored path is the one that fires them. Requires `id:`.
- tapOn:
    id: "modal-sheet-dismiss-btn"
    dispatch: xcui

# XCTRunnerDaemonSession synthesize — bypasses the XCUIElement gesture-
# recognizer chain so RN RCTTouchHandler receives the raw touch. Use
# when a Pressable swallows the default path on an older RN.
- tapOn:
    id: "btn-login"
    dispatch: daemonProxy
```

- Omit `dispatch:` for the default routing — it is right for almost every tap.
- `dispatch: xcui` requires an `id:` selector (resolution is by accessibilityIdentifier). Cross-platform: on Android it routes through the runner's id-anchored tap (`/tap-by-id`), same semantics.
- `dispatch: daemonProxy` is an iOS-runner mechanism; on Android it errors with an explicit unsupported message (native synthesize is already the Android default — the override is never needed there).

### Tap by coord

```yaml
- tapOn:
    point: "50%,80%"    # fraction of the viewport, not pixels ("0.5,0.8" is the same)
```

- nx, ny in viewport normalized [0, 1].
- Bypasses element resolution — direct touch dispatch at coord.
- Brittle; only use when no element identifier is reliable.

### Tap with OCR fallback

```yaml
- tapOn:
    fallback:
      - id: "btn-skip"            # tier 1 — a11y identifier (cheap)
      - text: "Skip"              # tier 2 — a11y text
      - ocrText: "Skip"           # tier 3 — Vision (iOS) / ML Kit (Android)
```

- Each tier is probed in order; first hit taps. OCR taps at the recognized text's bounding-box center via native event synthesize.
- When the chain contains any `ocrText`, the whole chain is **polled** for up to `SMIX_TAP_OCR_POLL_MS` (default 3000 ms, 250 ms cadence) before failing — this closes the race where the tap fires while the target is still mounting and a one-shot OCR would snapshot too early. Chains without OCR keep single-pass semantics (no perf change).
- On full miss the failure hint carries a per-layer trace (`L1 …: MISS; L2 …: MISS; L3 …: MISS`) naming exactly which tiers were probed.
- One OCR call is ~500 ms on-sim; keep at least one cheap tree tier ahead of `ocrText`.

### Double tap

```yaml
- doubleTapOn:
    id: "photo-1"
```

- Two taps with <300ms gap (system default).
- iOS: XCUI `element.doubleTap()` or two IOHID events.
- Android: two `/tap-at-norm-coord` events spaced ~200ms.

### Long press

```yaml
- longPressOn:
    id: "list-row-3"
    duration: 1500     # ms
```

- iOS: a synthesised touch held on the element's centre for `duration`.
  `XCUIElement.press(forDuration:)` is not used — it was measured taking
  a constant ~2.6s for every hold from 500ms to 6000ms on iOS 26.5, so
  the duration it performed was not the one it was given.
- Android: Kotlin `/long-press-at-norm-coord` with duration.

#### Capturing the held state

Verifying a pressed appearance — a highlight, a scale-down, a ghost
background — means seeing the screen *while* the touch is down.

```yaml
- longPressOn:
    id: "hdr-back-btn"
    duration: 1500
    captureDuring: true
```

Frames are written to `--debug-output` when given, else `.smix/press/`,
one PNG per capture, named for where it sits relative to the press:

| suffix | meaning |
|---|---|
| `-during` | the touch was provably down for the whole capture |
| `-unplaced` | the capture straddles a boundary; its pixels could be from either side |
| `-outside` | the touch was provably not down for part of it |

The step **fails** when no frame can be placed inside the press, rather
than handing back frames that might be of the resting screen. That
failure is the one this exists to prevent: screenshotting alongside a
press and reading a resting screen as "the animation never fired".

`duration` must be at least **800ms**. The gesture call carries
290-342ms of overhead and a capture takes 190-350ms, so a shorter hold
leaves no stretch a frame provably sits inside. 800ms yields one placed
frame; 1500ms yields three.

Android does not support `captureDuring`: `UiDevice.swipe` reports
nothing about when the touch was down, so no frame could be placed.

## Text input

### Fill

```yaml
- inputText:
    id: "form-email-input"
    text: "alice@example.com"    # replaces what the field holds
- inputText: "alice@example.com" # types into the focused field, appending
```

**You can only replace a field you named.** The mapping form above
empties the field first, so returning to a screen and filling it again
leaves the second value rather than both concatenated — which is
invisible in a password field and surfaces as a login that fails with
the right password typed. The scalar form has no field to empty: it
types wherever focus already is, and appends, which is also what
maestro's `inputText` and `pasteText` do.

To append to a named field, clear nothing and type into the focus you
already have: `tapOn` it, then the scalar form.

- iOS: XCUI `typeText` after explicit field tap (focus + type).
- Android: Kotlin `/input-text` → `am instrument` shell input (UiAutomation.executeShellCommand wraps `input text`).
- Android read-back: the runner checks that the characters arrived
  before answering. A field that masks its contents cannot be asked
  that question — its accessibility node reports one bullet per
  character and never the characters — so a masked field is judged by
  how much longer it got instead. The masked branch is keyed on the
  node reporting itself as a password, never on the text looking like
  one, so a plaintext field holding `aaaa` is still checked by content.
- Android targeting: a named fill tells the runner where the field was
  tapped, and the runner waits for focus to reach *that* field rather
  than for any field to have focus. Without it, a fill naming one field
  could clear and type into whichever field had focus a moment earlier.
  A scalar `inputText` names no field and still means "wherever focus
  is".
- Special chars: spaces, unicode are handled; backslash-escaping is internal.
- Important Android quirk: UiAutomation.executeShellCommand does NOT use `sh -c`, so quote-wrapping the text causes literal quotes. The runner handles this; do not pre-quote yourself.

### Clear

```yaml
- eraseText: 5     # backspace 5 times (default 50 if omitted)
- eraseText        # default depth
```

- iOS: XCUI `typeText(XCUIKeyboardKeyDelete)` × N.
- Android: dispatched as N "BACK" key events (or N input shell calls if BACK not available).

### Hide keyboard

```yaml
- hideKeyboard
```

- iOS: several strategies in order — dismiss/return keys, a tap outside
  any text field, and a tap just above the keyboard's own frame — until
  the keyboard is gone.
- Android: Kotlin runner `/hide-keyboard` → `IME.hide()`.

**It answers about the keyboard, not about itself.** No keyboard present
is success (there is nothing to hide). A keyboard still up after every
strategy ran is `keyboard_did_not_close`, and a runner that raised while
looking is `keyboard_state_unknown` — see
[07 — Error codes](07-errors.md). Until 9.0.0 all four cases answered
`ok: true`.

## Scroll

### One-shot scroll

```yaml
- scroll
- scroll:
    direction: UP
```

- Default: scroll viewport DOWN by one screen height (or by component's pageSize for paginated content).
- Directions: UP / DOWN / LEFT / RIGHT.
- iOS: `element.swipeUp()` / `coord.scroll(byDeltaX:deltaY:)`.
- Android: `/swipe-once` with hardcoded distance ratios.

### Scroll until visible

```yaml
- scrollUntilVisible:
    element:
      text: "Row #5000"
    direction: DOWN
    timeout: 30000

# OCR-aware variant — for virtualized lists whose off-screen items are
# dropped from the a11y tree (RN Fabric LazyColumn/LazyRow etc.), add
# an ocrText tier: tree + OCR are both probed between swipe strokes.
- scrollUntilVisible:
    element:
      fallback:
        - id: "btn-target"
        - ocrText: "Target label"
    direction: DOWN
```

- Iteratively scrolls + checks for element visibility.
- Default timeout 30s.
- Useful for very long lists.
- When the selector contains `ocrText`, each inter-swipe probe fires OCR in addition to the tree check (30-swipe / 20 s budget). Selectors without OCR keep the tree-only fast path.

### Custom swipe (explicit start/end)

```yaml
- swipe:
    start: "10%,50%"
    end: "90%,50%"
    duration: 400
```

- nx, ny normalized 0–1.
- Useful for horizontal tab bars, carousels, gesture-driven menus.

## Press hardware key

```yaml
- pressKey: ENTER
- pressKey: HOME
- pressKey: VOLUME_UP
- pressKey: LOCK
```

- Available keys: ENTER / TAB / SPACE / DELETE / ESCAPE / HOME / LOCK / VOLUME_UP / VOLUME_DOWN / ARROW_UP / ARROW_DOWN / ARROW_LEFT / ARROW_RIGHT.
- **Back navigation is not a key press.** Use the `- back` verb: on
  Android it sends the back key, on iOS it taps the navigation-bar back
  button. `pressKey: BACK` is refused deliberately — an earlier alias
  mapped it to Delete, which turned every back step into a silent
  backspace that reported success.
- **`VOLUME_UP` / `VOLUME_DOWN` are skipped on the iOS simulator**, not
  executed: Apple documents `XCUIDevice.Button.volumeUp` / `.volumeDown`
  as unavailable there, and maestro has the same limitation. The step
  reports as skipped with the reason rather than failing.
- There is no `POWER` key. `LOCK` is the closest equivalent
  (`XCUIDevice.perform(.lockButton)` on iOS).
- iOS: maps to XCUIRemote / device interaction methods.
- Android: maps to KeyEvent constants via runner `/press-key`.

## App lifecycle

```yaml
- launchApp:
    appId: com.example.app
    clearState: true
    clearKeychain: true
    arguments: ["--debug-mode"]

- killApp                  # current
- killApp: com.acme.other  # named

- stopApp                  # graceful
```

- iOS: `simctl terminate / launch` with clearState wiping `~/Library/Caches` and defaults.
- Android: `am force-stop` + `pm clear` (clearState).

## Orientation

```yaml
- setOrientation: LANDSCAPE_LEFT
- setOrientation: PORTRAIT
```

- iOS: `simctl io <udid> orientation <name>`.
- Android: `adb shell content insert --uri content://settings/system --bind name:s:user_rotation`.
- Some apps lock orientation in Info.plist / manifest → setOrientation no-op; assert with `assertVisible` against orientation-dependent label to verify.

## System permissions

```yaml
- setPermissions:
    camera: allow
    location: allow
    notifications: allow
    photos: deny
```

- iOS: `simctl privacy grant/deny`.
- Android: `adb shell pm grant <pkg> android.permission.<NAME>` or revoke.
- Available keys: camera / microphone / location / contacts / photos / calendar / notifications / motion / faceid / siri / health.

## Deep links / openLink

```yaml
- openLink: "myapp://home/details/42"
- openLink:
    link: "https://example.com"
```

- iOS: `xcrun simctl openurl`.
- Android: `adb shell am start -W -a android.intent.action.VIEW -d '<url>' <pkg>`.

## Recording

```yaml
- startRecording: "trace.mp4"
- stopRecording
- takeScreenshot: "step5.png"
```

- iOS: `simctl io recordVideo` (background process).
- Android: `screenrecord` shell command (background process).

## Output destinations

By default, screenshots/videos save to `./.smix/trace/<run-id>/`. Override via `--trace-dir` flag on `smix run`.

## See also

- [02-yaml-reference.md](02-yaml-reference.md) — full step grammar
- [03-selectors.md](03-selectors.md) — how to identify the element to act on
- [05-cli.md](05-cli.md) — when you want to invoke actions one-shot via CLI (not YAML)
