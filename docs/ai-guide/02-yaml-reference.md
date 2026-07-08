# 02 — YAML reference

> Every YAML verb smix supports, with a copy-pasteable example. smix accepts maestro-compatible YAML; if a flow works under `maestro test`, it should also work under `smix run` modulo platform-specific extensions.

## File layout

A smix YAML file has two parts: **header** (optional) and **flow** (a YAML list of steps).

```yaml
appId: com.example.app             # iOS bundle id OR Android package
# OR for cross-platform:
app: myapp                         # logical key resolved via --apps-config apps.yaml
---                                # YAML doc separator (header ends, flow begins)
- launchApp:
    clearState: true
- assertVisible: "Hello"
- tapOn: "Submit"
```

The `app:` form (cross-platform) needs `--apps-config <path>` flag. The `appId:` form picks a literal id (use this when you want a single platform).

## The verb set (organized by purpose)

### App lifecycle

```yaml
- launchApp                              # no args: foreground default app
- launchApp:
    appId: com.acme.app                  # override yaml-level appId
    clearState: true                     # wipe NSUserDefaults / shared prefs
    clearKeychain: true                  # iOS keychain wipe
    arguments: [["--debug"], ["--env=prod"]]   # launch args

- killApp                                # force-quit current app
- killApp: com.acme.other               # force-quit specific
- stopApp                                # graceful background
- clearState                             # wipe state without restart
- clearKeychain                          # wipe keychain (iOS only)
```

### Assertions (read-only checks; do not change app state)

```yaml
- assertVisible: "Welcome"               # text literal
- assertVisible:
    id: "home-counter-label"             # accessibility identifier
- assertVisible:
    text: "Submit"
    enabled: true                        # element must be enabled
- assertVisible:
    text: "Error"
    optional: true                       # if absent, step is "skipped" not "failed"

- assertNotVisible: "Error"              # element must NOT be on screen
- assertNotVisible:
    id: "modal-overlay"

- assertTrue: ${output.userCount > 0}    # JS expression eval (runs in evalScript context)

- assertScreenshot:                      # visual regression (deferred — not yet shipped)
    baseline: "home.png"
    threshold: 0.05
```

### Tap / touch actions

```yaml
- tapOn: "Submit"                        # text literal
- tapOn:
    id: "home-increment-btn"
- tapOn:
    text: "Item.*"                       # regex pattern (anchored)
    nth: 2                               # 0-indexed within matches
- tapOn:
    text: "Continue"
    optional: true                       # skip if not visible (no error)
- tapOn:
    point: "50%,80%"                     # viewport-normalized coord (X%,Y%)

- doubleTapOn: "Reset"
- longPressOn:
    id: "list-row-3"
    duration: 1500                       # ms

- tapAtPoint: "50%,50%"                  # legacy verb, prefer `tapOn: { point: ... }`
```

### Text input

```yaml
- inputText: "alice@example.com"         # types into focused field
- inputText:
    id: "form-email-input"               # target field by selector first
    text: "alice@example.com"

- eraseText: 5                           # backspace N chars (default 50)
- copyTextFrom:
    id: "form-result-label"              # copies result to clipboard env
- pasteText                              # paste clipboard into focused field
- setClipboard: "hello"                  # set clipboard programmatically
- hideKeyboard                           # dismiss soft keyboard
```

### Scrolling

```yaml
- scroll                                 # scroll viewport down (default direction)
- scroll:
    direction: UP                        # UP / DOWN / LEFT / RIGHT

- scrollUntilVisible:
    element:
      text: "Row #5000"
    direction: DOWN
    timeout: 30000                       # ms

- swipe:
    direction: LEFT
    duration: 400

- swipe:
    start: "10%,50%"
    end: "90%,50%"
    duration: 400
```

### Keyboard / hardware keys

```yaml
- pressKey: ENTER                        # ENTER / BACK / TAB / SPACE / etc.
- pressKey: HOME                         # iOS home button
- pressKey: VOLUME_UP                    # Android hardware
- pressKey: POWER
```

### System / device controls

```yaml
- setOrientation: LANDSCAPE_LEFT         # PORTRAIT / LANDSCAPE_LEFT / LANDSCAPE_RIGHT / PORTRAIT_UPSIDE_DOWN
- setLocation:
    latitude: 35.6812
    longitude: 139.7671
- travel:
    points:
      - { latitude: 35.0, longitude: 139.0 }
      - { latitude: 36.0, longitude: 140.0 }
    speedMps: 50
- setPermissions:                        # iOS / Android permission grants
    camera: allow
    location: allow
    notifications: allow
```

### Deep links

```yaml
- openLink: "myapp://home/details/42"
- openLink:
    link: "https://example.com"
    autoVerify: true
    browser: true                        # force open in browser instead of app
```

### Recording (video output)

```yaml
- startRecording: "trace.mp4"
- stopRecording                          # writes path passed to start
- takeScreenshot: "step5.png"
```

### Control flow

```yaml
# Repeat fixed count
- repeat:
    times: 5
    commands:
      - tapOn: "Increment"

# Repeat while condition
- repeat:
    while:
      visible: "Loading..."
    commands:
      - wait: 500

# Repeat while NOT visible (poll for appearance)
- repeat:
    whileNotVisible:
      id: "home-result-label"
    commands:
      - wait: 200

# Retry on failure
- retry:
    maxRetries: 3
    commands:
      - tapOn: "Flaky Button"
      - assertVisible: "Confirmed"

# Run a sub-flow (file include)
- runFlow: "../subflows/launch-fresh.yaml"

# Run a sub-flow conditionally
- runFlowConditional:
    when:
      visible: "Need Login"
    commands:
      - runFlow: "../subflows/login.yaml"

# Run inline JS (evaluated in the maestro JS context)
- evalScript: |
    output.userId = output.someResponse.id
- runScript: "../scripts/setup.js"
```

### Wait / sync

```yaml
- waitForAnimationToEnd                  # blocks until UI quiescent
- waitForAnimationToEnd:
    timeout: 5000

- extendedWaitUntil:
    visible: "Loading complete"
    timeout: 30000

- extendedWaitUntil:
    notVisible: "Spinner"
    timeout: 10000
```

### Media (file upload simulators)

```yaml
- addMedia:                              # iOS Photos / Android Gallery seed
  - "/path/to/test-photo.jpg"
```

### WebView JS bridge

```yaml
- webViewEval: |
    document.getElementById('user-input').value = 'alice';
    submitForm();
    document.getElementById('form-result').textContent

# Returns the JS expression result (last-expression value). Async support via
# evaluating a Promise-returning expression — handled by the smix runner.
```

## Selector forms inside steps

Wherever a step takes a selector (tapOn, assertVisible, etc.), the selector may be:

- **String** literal → text match
  `tapOn: "Submit"`
- **Object** → typed selector with modifiers
  `tapOn: { id: "tab-home" }`
  `tapOn: { text: "Sub.*", nth: 1 }`
  `tapOn: { ocrText: "Done" }` (Vision/ML Kit)
  `tapOn: { anchor: "Header", below: "Title" }` (spatial)
  `tapOn: { point: "50%,50%" }` (coords)
  `tapOn: { fallback: [ {id: "foo"}, {text: "Foo"} ] }` (first hit wins)

Full selector taxonomy is in [03-selectors.md](03-selectors.md).

## Conditional + optional patterns

- `optional: true` on tapOn/assertVisible: step never fails the flow; it is skipped silently if the element is absent (used for cross-platform yamls where a step is iOS-only or Android-only).
- `runFlowConditional` with `when: { visible: ... }`: include a sub-flow only when a condition holds at runtime (e.g., "if Need Login is shown, run the login subflow").

## Output variables (between steps)

- `evalScript` and `runScript` write to `output.*` namespace.
- Subsequent steps read via `${output.foo}` interpolation.
- Useful for chaining assertions on dynamic content (id from API, name from form input, etc).

## Cross-platform YAML conventions

When writing a YAML that should run on both iOS + Android via `--platform ios|android`:

1. Use `app: <logical-key>` (cross-platform) instead of a literal `appId:` (single-platform).
2. Add `--apps-config apps.yaml` (or your own resolver file).
3. Mark platform-only steps `optional: true`.
4. Use `id:` selectors (mirror via testTag) over `text:` (i18n drift) over `ocrText:` (slow).

## Exit codes (from `smix run`)

| Code | Meaning |
|---|---|
| 0 | success |
| 2 | YAML parse error |
| 3 | runtime SDK failure (sim/app problem mid-flow) |
| 4 | unknown key / unknown direction (bad verb / arg) |
| 5 | runFlow cycle / file IO |
| 6 | runner unreachable |

A non-zero exit code is paired with a structured JSON error written to stdout (see [07-errors.md](07-errors.md)).
