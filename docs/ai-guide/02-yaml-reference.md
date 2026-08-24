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
    arguments: ["--debug", "--env=prod"]      # launch args

- killApp                                # force-quit current app
- killApp: com.acme.other               # force-quit specific
- stopApp                                # graceful background
- clearState                             # wipe state without restart
- clearKeychain                          # wipe keychain (iOS only)

# v1.0.27 — delete specific NSUserDefaults keys (iOS only). Surgical
# alternative to clearState when one persisted value poisons the next
# run (canonical case: expo-dev-launcher re-delivers its stored deep
# link after every JS bundle load — delete its key between stopApp
# and the next launch to neutralize the replay at the source).
# Contract: "ensure keys absent" — already-absent keys succeed.
# Terminate the app first; running processes cache defaults in-memory.
- clearUserDefaults:
    keys:
      - "expo.devlauncher.pendingDeepLink"
    bundleId: com.acme.app               # optional; default = flow appId
```

### Assertions (read-only checks; do not change app state)

```yaml
- assertVisible: "Welcome"               # text literal
- assertVisible:
    id: "home-counter-label"             # accessibility identifier
- assertVisible:
    text: "Error"
    optional: true                       # if absent, step is "skipped" not "failed"

- assertNotVisible: "Error"              # element must NOT be on screen
- assertNotVisible:
    id: "modal-overlay"

- assertTrue: ${1 > 0}                   # ==, !=, <, <=, >, >=, &&, ||, !, .contains()

- assertScreenshot: "home.png"           # visual regression — full-frame 64-bit dhash
- assertScreenshot:
    path: "home.png"                     # baseline path, relative to the flow file
    threshold: 5                         # max hamming distance (default 5)
```

`assertScreenshot` auto-records the baseline on the first run and diffs
against it afterwards. `mask:` regions are accepted but not yet applied
(the dhash compares the full frame); a run warning says so.

`assertTrue` reads `${…}` with a small expression engine, not a JS
runtime — comparison, boolean operators, parentheses and `.contains()`,
and nothing else. It can read `output.*`, which is written by
`extractWithAI` and by `runFlow: {as: name}`; both store **strings**, so
`${output.total > "100"}` compares text, not numbers. Comparing a
string to a number is refused rather than converted, because an
assertion that answers on a reading you did not intend is worse than
one that stops.

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
    point: "50%,80%"                     # fraction of the viewport, NOT pixels
                                         # `"0.5,0.8"` is the same point; `%` is optional
                                         # pixels are refused — a flow written in them
                                         # runs on one screen size

# Several taps on one element, spaced by a number you state.
#
# `repeat` around `tapOn` sends a request per tap, and each one costs a
# synthesised event round trip (~400 ms on iOS 26.5) — so the interval
# is whatever that cost, and a gesture gated on a short inter-tap window
# cannot be driven. This packs the touches into one event timeline.
- repeatTap:
    id: "hidden-trigger"
    times: 10
    intervalMs: 80                       # optional; runner default
    holdMs: 50                           # optional; how long each touch stays down

- doubleTapOn: "Reset"
- longPressOn:
    id: "list-row-3"
    duration: 1500                       # ms
    captureDuring: true                  # optional; write PNGs of the held state
                                         # needs duration >= 800 (see below)

```

### Text input

```yaml
- inputText: "alice@example.com"         # types into focused field, appending
- inputText:
    id: "form-email-input"               # names a field, so it replaces
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

# `over:` — swipe INSIDE one element, by shares of ITS box.
#
# The two forms above take shares of the SCREEN, which is what you reach
# for when there is nothing nameable to swipe between. When there is
# something nameable, `over:` names it and the numbers stop meaning
# anything about the display:
- swipe:
    over: { id: "view-timeline" }
    from: 0.3                            # three tenths along, halfway across
    to: 0.8

- swipe:
    over: { id: "view-timeline" }
    from: { x: 0.1, y: 0.5 }             # both axes, when the drag is not along one
    to: { x: 0.9, y: 0.5 }
```

A bare number is the share along the axis with the cross axis centred —
a drag along a strip has one interesting axis, and guessing the other as
`0` would run along its edge.

**Percent strings are refused here.** `"30%"` of an element and `"30%"`
of the screen are the two things this form exists to keep apart, and one
spelling meaning both is how a flow ends up measuring the display again.

**It does not fall back to the screen.** If the element is not there, the
step fails and says so — swiping across the middle of whatever is showing
would do something the flow did not ask for and report it as done.

Reported by a consumer who had a nameable timeline and still had to
measure it: 45.3–50.5% of screen height on Android, 47.7–53.2% on iOS,
49% taken as the overlap, and a note that a device of a different shape
would need measuring again.

### Keyboard / hardware keys

```yaml
- pressKey: ENTER                        # ENTER / TAB / SPACE / DELETE / ESCAPE
- pressKey: HOME                         # iOS home button
- pressKey: VOLUME_UP                    # skipped on the iOS simulator
- back                                   # navigation back — not a key press
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
      - waitForAnimationToEnd: 500

# Repeat while NOT visible (poll for appearance)
- repeat:
    while:
      notVisible:
        id: "home-result-label"
    commands:
      - waitForAnimationToEnd: 200

# Retry on failure
- retry:
    maxRetries: 3
    commands:
      - tapOn: "Flaky Button"
      - assertVisible: "Confirmed"

# Run a sub-flow (file include)
- runFlow: "../subflows/launch-fresh.yaml"

# Run a sub-flow conditionally (same `runFlow` verb + `when:` gate)
- runFlow:
    when:
      visible: "Need Login"           # enter only when this is visible
    file: "../subflows/login.yaml"

# Inverse gate (v1.0.24) — enter only when the selector is NOT visible.
# Idempotency pattern: skip a ceremony whose end state is already reached.
- runFlow:
    when:
      notVisible: { id: "qa-bubble" }
    file: "../subflows/enter-qa-mode.yaml"

# Inline body instead of a file (`commands:`); `when:` gates identically.
- runFlow:
    when:
      visible: "Open in"
    commands:
      - tapOn: "Open"
      - waitForAnimationToEnd

# `visible` / `notVisible` are mutually exclusive within one `when:`.
# The gate selector supports the full selector table, including
# `fallback:` chains with `ocrText` (OCR fires for the check).

# Run inline JS (evaluated in the maestro JS context)
- evalScript: |
    output.userId = output.someResponse.id
- runScript: "../scripts/setup.js"
```

### Wait / sync

```yaml
# Fixed sleep — NOT an XCTest quiescence wait. smix no-ops XCTest's
# internal idle-wait for performance (SmixQuiescenceSwizzle); this verb
# has always been a bounded pause at the runtime layer.
#
# On Android a run zeroes the animation scales before it starts, so
# this verb has little left to wait for; `--animations` restores the
# device's own settings and with them the original reason for it.
#
# On iOS nothing is quietened — no host-side lever exists — so this
# verb keeps its full value there.
- waitForAnimationToEnd                  # bare form: ceiling 400 ms (maestro-compat)
- waitForAnimationToEnd: 500             # integer: ceiling 500 ms
- waitForAnimationToEnd:
    timeout: 5000                        # map form: ceiling 5000 ms (maestro-canonical)

# The number is a CEILING in all three forms, never a sleep. The verb
# samples the screen and returns as soon as two frames match, so a still
# screen costs two captures and not the number written here. It used to
# sleep, which charged for an animation whether or not one ran; this note
# said "sleep N ms" for longer than that was true, and a consumer wrote
# `waitForAnimationToEnd: 500` meaning "pause here" on the strength of it.
#
# There is no bare sleep verb, deliberately. To wait for something, name
# it: `extendedWaitUntil` for an element. The keyboard is an element —
# `extendedWaitUntil: { visible: { role: "keyboard" } }` — which is the
# wait people most often reach for a pause instead of, because the keys
# are not in the tree and `describe` leaves the keyboard out of its
# summary. See "role: keyboard" in the selectors guide.
#
# If the thing you are waiting for really is not in the tree, that is a
# gap worth reporting rather than routing around with a pause.
#
# A screen that never settles is a warning, not a failure — so is a window
# in which the device refused every capture (see CAPTURE_BACKPRESSURE in
# the errors guide). The verb waits refusals out inside its own ceiling.

- extendedWaitUntil:
    visible: "Loading complete"
    timeout: 30000

- extendedWaitUntil:
    notVisible: "Spinner"
    timeout: 10000

# On timeout, extendedWaitUntil auto-captures a screenshot + tree JSON
# to `.smix/timeouts/` (or `--debug-output` dir when set) and appends
# the written paths to the failure hint — no flag needed.
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
- `runFlow` with `when: { visible: ... }`: include a sub-flow only when a condition holds at runtime (e.g., "if Need Login is shown, run the login subflow").
- `runFlow` with `when: { notVisible: ... }` (v1.0.24): inverse gate — enter only when the selector is NOT visible ("run the ceremony unless its end state is already on screen").
- Skipped conditionals emit `STEP N: … → SKIPPED: <reason>` to stderr with the checked selector + evaluation outcome, so batch logs show WHY a gate short-circuited.
- Step lines are printed as each step starts, so the last one in the log
  is the step that was in flight. A failure also names itself: the error
  reads `step N (verb): …`, and a `STEP N: verb → FAILED` line joins the
  skipped one. A subflow's inner step is named once — the `runFlow` that
  contains it does not claim the failure.

## OCR behavior in verbs

`ocrText:` parses anywhere a selector does, standalone or inside a
`fallback:` chain. Which verbs actually fire Vision (iOS) / ML Kit
(Android) for it is one table, in
[03-selectors](03-selectors.md#11-fallback-chain) — generated from the
one the code decides by. A verb that does not read it says so and says
what to write instead; none of them quietly matches nothing.

The verbs that fire OCR, and what they do with it:

- `extendedWaitUntil.visible` — polls tree + OCR per iteration until timeout.
- `tapOn` with `fallback: [..., ocrText]` — polls the whole chain within `SMIX_TAP_OCR_POLL_MS` (default 3000 ms) before failing, closing tap-vs-mount races.
- `scrollUntilVisible` — probes tree + OCR between swipe strokes (off-screen items dropped from a degraded a11y tree are still found by OCR once scrolled into view).
- `runFlow.when.visible` / `when.notVisible` — the gate check fires OCR.

Cost note: one OCR call is ~500 ms on-sim. Order `fallback:` chains cheapest-first (`id` → `text` → `ocrText`) so tree hits pre-empt Vision cost.

### `SMIX_AUTO_OCR_FALLBACK=1` (env opt-in)

Auto-lifts every bare-string selector `visible: 'X'` to `fallback: [text: 'X', ocrText: 'X']` at parse time. Strings containing top-level `|` (regex OR) split per alternative for the OCR tiers: `'A|B'` → `fallback: [text: /A|B/i, ocrText: 'A', ocrText: 'B']`. Accepted truthy values: `1`, `true`, `TRUE`, `yes`.

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
