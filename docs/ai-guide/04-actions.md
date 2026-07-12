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
    point: "50%,80%"
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
