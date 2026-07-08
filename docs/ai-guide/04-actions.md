# 04 — Actions

> Every action verb (tap / fill / swipe / scroll / press-key / etc.) with how it dispatches under the hood, and when to use Path A vs Path B.

## Action mental model

```
yaml verb (tapOn) → smix-adapter (translation) → Driver trait method
                                                  ├─ IosDriver  → XCUITest server (Path B)
                                                  │                IOHID synthesize (Path A)
                                                  └─ AndroidDriver → Kotlin runner /tap-by-id
                                                                     /tap-at-norm-coord
```

The YAML is platform-agnostic. The driver picks the right strategy for the target platform.

## Tap

### Default tap

```yaml
- tapOn:
    id: "home-increment-btn"
```

**iOS dispatch**:
- If the matched element exposes `accessibilityIdentifier`, the swift `/tap-by-id` route dispatches via IOHID `_XCT_synthesizeEvent` (Path A — fires SwiftUI onTap closure; XCUI `coord.tap` does not).
- Otherwise, falls back to XCUI `element.tap()` (Path B — dispatch only, may not fire some SwiftUI handlers).

**Android dispatch**:
- Kotlin runner `/tap-by-id` → resolves via UiAutomation `findAccessibilityNodeInfosByViewId` → returns center coord → native event synthesize.

### Tap with explicit mode

```yaml
- tapOn:
    id: "tab-home"
    mode: pathA       # force IOHID synthesize (iOS only)

- tapOn:
    id: "tab-home"
    mode: pathB       # force XCUI coord.tap
```

- Path A (IOHID) fires Apple's full native event chain including SwiftUI gesture handlers. Required for any button that uses `onTap` closure rather than `.gesture(TapGesture())`.
- Path B (XCUI) dispatches a touch event. Faster but does not fire SwiftUI Button onClick in some iOS versions.
- Android always uses native event synthesize (no Path A/B distinction).

### Tap by coord

```yaml
- tapOn:
    point: "50%,80%"
```

- nx, ny in viewport normalized [0, 1].
- Bypasses element resolution — direct touch dispatch at coord.
- Brittle; only use when no element identifier is reliable.

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
```

- Iteratively scrolls + checks for element visibility.
- Default timeout 30s.
- Useful for very long lists.

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
- pressKey: BACK
- pressKey: HOME
- pressKey: VOLUME_UP
- pressKey: POWER
```

- Available keys: ENTER / TAB / SPACE / DELETE / BACK / HOME / VOLUME_UP / VOLUME_DOWN / POWER / LOCK / SCREEN_LOCK.
- iOS: maps to XCUIRemote / device interaction methods.
- Android: maps to KeyEvent constants via runner `/press-key`.

## App lifecycle

```yaml
- launchApp:
    appId: com.example.app
    clearState: true
    clearKeychain: true
    arguments: [["--debug-mode"]]

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
    browser: true      # force open in default browser (not target app)
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
