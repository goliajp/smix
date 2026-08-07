# 07 — Error codes + remediation

> Every error smix can surface, what triggered it, and exactly what to do. Optimized for "I just saw error X, what's next?"

## Failure shape

smix errors come back as structured JSON:

```json
{
  "ok": false,
  "code": "ELEMENT_NOT_FOUND",
  "message": "tap_by_id: element not found — id=\"home-incremnt-btn\"",
  "hint": "runner XCUIQuery returned no match; check id spelling or wait for screen to settle",
  "step": "tapOn",
  "step_index": 3,
  "visibleElements": [
    { "id": "home-counter-label", "text": "0" },
    { "id": "home-increment-btn", "text": "+1" },
    { "id": "home-reset-btn",     "text": "Reset" }
  ],
  "suggestions": [
    "home-increment-btn (id, exact match to a sibling — likely typo in your selector)"
  ]
}
```

Read **`code`** first to triage. Read **`hint`** + **`suggestions`** to know the fix.

## Error codes

### ELEMENT_NOT_FOUND

**Trigger**: a selector matched zero elements after implicit-wait timeout.

**Common causes**:
1. Typo in the testid/text
2. Element off-screen (LazyRow / LazyColumn / scrollable tab bar)
3. Element in a separate window (Compose modal / iOS sheet) without testTagsAsResourceId properly enabled
4. Screen has not finished rendering (need `waitForAnimationToEnd` before)
5. Wrong screen — you forgot to navigate first

**Fix**:
- Check `suggestions` block — often shows the typo or close match
- For off-screen: prepend `scrollUntilVisible` or `swipe` step
- For modal: see [03-selectors.md](03-selectors.md) Compose modal pitfalls
- For timing: add `waitForAnimationToEnd` or `extendedWaitUntil`
- For wrong screen: dump `smix tree | head -30` to see what's actually showing

### NOT_VISIBLE

**Trigger**: element exists in tree but is occluded / off-screen / has 0 width-height / `isHidden=true`.

**Fix**:
- Element may be covered by a system popup → `smix system-popups` to check
- May be below fold → `scrollUntilVisible`
- May be hidden by overlay → check `assertNotVisible: { id: "modal-overlay" }` first

### NOT_ENABLED

**Trigger**: element matched but `isEnabled=false` (button disabled, text field readonly, etc.).

**Fix**:
- The app has a guard. Satisfy the precondition first (e.g., fill the form before clicking Submit).
- Check app state — `smix describe` shows enabled state of visible interactives.

### AMBIGUOUS

**Trigger**: a selector requiring uniqueness (e.g., `tapOn: { text: "Edit" }`) matched multiple elements.

**Fix**:
- Use a more specific selector: add `id:`, `nth:`, or spatial modifier (`inside:`, `near:`).
- Example: `tapOn: { text: "Edit", inside: "modal" }`.

### TIMEOUT

**Trigger**: an `extendedWaitUntil` or implicit-wait exceeded its budget.

**Fix**:
- Increase the timeout if the operation is genuinely slow (cold app launch, network call).
- If something's stuck, dump `smix tree` to see what's actually happening.
- If a system popup is blocking, `smix system-popups` + `smix system-popup-action` to dismiss.

### ASSERTION_FAILED

**Trigger**: `assertTrue` JS expression evaluated to falsy.

**Fix**:
- Print the eval'd context: in your `evalScript`, log `output.*` values to see what was actually computed.
- Often: an earlier `runScript` set a different field than you expected.

### APP_NOT_RUNNING

**Trigger**: target app process exited or never launched.

**Fix**:
- Check app's crash log: `smix sim exec <udid> spawn ~/Library/Logs/CoreSimulator/<udid>/system.log | tail -50`.
- For iOS, also `xcrun simctl spawn <udid> log show --last 5m --predicate 'eventMessage contains "<bundle-id>"'`.
- Verify `launchApp` step ran (or that the app is installed: `smix sim list-apps <udid>`).

### SIMULATOR_NOT_BOOTED

**Trigger**: simulator UDID exists but is shutdown.

**Fix**: `smix sim boot <udid>` then retry.

### TAP_MISSED

**Trigger**: the touch was synthesised, and the point it landed on was
not inside the element the selector matched.

`tapOn` resolves a selector against the a11y tree, takes the matched
element's centre, and synthesises a touch there. Between those two
steps the screen can move — a list settles, a banner appears, a sheet
finishes presenting — and the coordinate then belongs to something
else. The tap still happened; it happened somewhere you did not mean.

The message names both sides: what was aimed at, and what the point
turned out to be inside.

**Fixes**:
- Wait for the screen before tapping (`extendedWaitUntil`, or
  `waitForAnimationToEnd` if you are running with `--animations`)
- If it reproduces on a still screen, the element's frame is wrong
  rather than stale — capture `smix tree --json` and check the frame

**Escape hatch**: `SMIX_TAP_HIT_MISMATCH=warn` downgrades this to a
warning for a whole run. It exists so an existing suite can be moved
over gradually; a run under it reports success for taps that missed.

**What this does NOT catch**: an element covered by something
transparent to the a11y tree. A scrim over your button contains the
tapped point too, so the check passes and the touch still may not reach
the button. Nothing recovers this, public or private: a scrim that is
transparent to accessibility leaves no trace in the tree, so every
signal derived from that tree — snapshot fields and live hit-tests
alike — is blind to it in the same way. See "tap returns `ok: true`
but state doesn't change".

### DRIVER_ERROR

**Trigger**: catch-all for runner-side / IO / unexpected failures. Read the `message` for specifics.

**Common subtypes**:
- `runner unreachable`: runner crashed or wrong port — re-run `smix runner up`
- `socket timeout`: runner alive but slow (XCUITest hung) — `smix down` then re-up
- `webview-bridge unreachable`: WebView-hosting screen never visited; navigate first OR `adb forward tcp:28081`
- `JSON decode failed`: runner version mismatch — rebuild runner

## Common failure patterns + fixes

### "Cannot find 'FooScreen' in scope" (iOS build)

You added a Swift file to your Xcode project's sources folder but did not add it to `<YourApp>.xcodeproj/project.pbxproj`.

**Fix**: add 4 entries (use canonical alpha-sort order):

```text
1. PBXBuildFile section:   <UUID> /* FooScreen.swift in Sources */ = ...
2. PBXFileReference:        <UUID> /* FooScreen.swift */ = ...
3. PBXGroup:                <UUID> /* FooScreen.swift */,
4. PBXSourcesBuildPhase:    <UUID> /* FooScreen.swift in Sources */,
```

Generate UUIDs via `openssl rand -hex 12 | tr a-f A-F`.

### tap returns `ok: true` but state doesn't change

First check whether you got `TAP_MISSED` — since v2.0.0 a tap that
lands outside the element it aimed at says so. If the tap is reported
as landing correctly and the app still did nothing, the cause is one of
these:

**The element is covered.** smix cannot see this: the a11y snapshot
carries no z-order, so a scrim over your button contains the tapped
point exactly as the button does. `smix tree --json` shows both; the
one drawn later wins and the tree does not say which that is.

**The element is not a touch responder.** An image or a label inside a
button is in the tree and takes no touches. Aim at the ancestor that
handles the gesture.

**Compose interop.** Some Compose `Button` `onClick` lambdas don't fire
reliably when a heavy `AndroidView` interop component (MapView,
PreviewView) is in the same composition. The engine dispatches but the
lambda is not invoked.

**Workaround**: extract heavy `AndroidView` into a separate Composable holder.

### WebView eval flakes on first invocation

Known flake when the WebView shim hasn't been initialized yet.

**Workaround**: navigate to the WebView-hosting screen at least once before exercising `webViewEval` to warm up the bridge. Re-running the flow usually passes the second time.

### Tap on tab bar misses (Android LazyRow)

Tab is in a `LazyRow` that lazy-renders off-screen items. The tap-by-id dispatcher won't find an off-screen testid.

**Fix**:
```yaml
# Swipe the tab bar before tapping the far-right tab
- swipe:
    start: "95%,8%"   # right edge of tab bar
    end: "5%,8%"
    duration: 200
- tapOn: { id: "tab-wizard" }   # now visible
```

### Android emulator dropping after long-running test

The emulator can go offline (`adb: device offline`) under load. Re-check:
```bash
adb -s emulator-5554 reconnect
adb -s emulator-5554 shell getprop sys.boot_completed
```

### YAML works in maestro but fails in `smix run`

Either:
1. Verb not supported (uncommon — only `assertScreenshot` is currently deferred).
2. Selector form not yet implemented (rare — file an issue).
3. Default direction differs — explicitly set `direction: DOWN` if you relied on maestro's default.

### "permission denied" mid-flow

Permission dialog appeared and was not dismissed. smix has `setPermissions` for declarative grant; also see `system-popups` + `system-popup-action` for one-shot dismiss.

## Debug recipes

### Capture full trace for a failing step

```bash
RUST_LOG=info smix run ... 2>&1 | tee /tmp/trace.log
```

### Snapshot the screen at point of failure

```bash
# In a separate terminal during the run, OR right after failure:
smix sim exec <udid> io screenshot /tmp/fail.png && open /tmp/fail.png
```

### Get full a11y tree at failure point

```bash
smix tree --json > /tmp/tree.json
jq . /tmp/tree.json | less
```

### See what the runner has been doing

```bash
# iOS runner stdout/stderr goes to:
ls -la ~/Library/Logs/smix-runner/<udid>/

# Android runner log (from instrumentation start):
cat /tmp/smix-runner-instrument.log
```

### Sanity check the testid you expect exists

```bash
smix find id:home-increment-btn && echo "yes" || echo "no"
```

### Verify the YAML schema independently

```bash
smix run --dry-run flow.yaml   # if supported (else parse with yamllint)
```

## See also

- [01-quickstart.md](01-quickstart.md) §common first-run errors — table of install/boot issues
- [08-cookbook.md](08-cookbook.md) — patterns that avoid common errors
