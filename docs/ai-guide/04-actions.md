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
element containing the point it touched, and the step fails with
`TAP_MISSED` if the element you aimed at is not among them. So success
means the touch landed inside your target, not merely that a touch was
synthesised somewhere — which is what it used to mean.

It does **not** mean the target received the touch. Something drawn
over your element contains the same point, and the a11y snapshot
carries no z-order, so smix cannot tell which one is on top. If a tap
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

- iOS: XCUI `element.press(forDuration:)`.
- Android: Kotlin `/long-press-at-norm-coord` with duration.

## Text input

### Fill (replaces focused field content)

```yaml
- inputText:
    id: "form-email-input"
    text: "alice@example.com"
```

- iOS: XCUI `typeText` after explicit field tap (focus + type).
- Android: Kotlin `/input-text` → `am instrument` shell input (UiAutomation.executeShellCommand wraps `input text`).
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

- iOS: tap outside any text field area (the "tap outside" trick).
- Android: Kotlin runner `/hide-keyboard` → `IME.hide()`.

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
