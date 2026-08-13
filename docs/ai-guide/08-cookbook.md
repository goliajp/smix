# 08 — Cookbook (recipes)

> Copy-paste patterns for things you'll write many times. Each recipe is a self-contained YAML fragment + commentary.

## Cross-platform YAML (iOS + Android, single file)

```yaml
# Header — use `app:` logical key (not `appId:`)
app: myapp
---
# 1. Any platform-only setup step — mark optional so cross-platform runs skip it silently
- tapOn:
    id: "some-ios-only-btn"
    optional: true

# 2. The actual test
- tapOn: { id: "tab-home" }
- assertVisible: { id: "home-counter-label" }
- tapOn: { id: "home-increment-btn" }
- assertVisible:
    id: "home-counter-label"
    text: "1"
```

```bash
# Run on iOS
smix run --device <iosudid> --platform ios --apps-config apps.yaml \
  --runner-port 22087 --no-launch flow.yaml

# Run on Android
smix run --device emulator-5554 --platform android --apps-config apps.yaml \
  --runner-port 28080 --no-launch flow.yaml
```

`apps.yaml`:
```yaml
apps:
  myapp:
    ios:     { bundleId: com.example.app }
    android: { package: com.example.app, activity: .MainActivity }
```

`activity` is an override, and most apps do not need it: omitted, smix
asks the device's package manager which activity the launcher starts.
Set it when an app has more than one entry point and a flow wants a
particular one.

## Login flow (text input + submit)

```yaml
appId: com.example.app
---
- tapOn: { id: "tab-form" }

- tapOn: { id: "form-name-input" }       # focus the field
- inputText: "Alice"

- tapOn: { id: "form-email-input" }
- inputText: "alice@example.com"

- tapOn: { id: "form-password-input" }
- inputText: "secret123"

- hideKeyboard                            # important: keyboard may cover submit
- tapOn: { id: "form-submit-btn" }

- assertVisible: { id: "form-submitted-label" }
```

**Why hideKeyboard before submit**: soft keyboard covers the bottom 40% of the screen. Submit button often gets pushed below the keyboard. `hideKeyboard` retracts the keyboard so the button is reachable.

## Modal interaction (Compose / SwiftUI sheet)

```yaml
- tapOn: { id: "tab-modal" }
- tapOn: { id: "modal-open-sheet-btn" }

# At this point a BottomSheet / .sheet is open
- waitForAnimationToEnd                      # let the sheet finish animating in
- assertVisible: { id: "modal-sheet-dismiss-btn" }
- tapOn: { id: "modal-sheet-dismiss-btn" }

- waitForAnimationToEnd                      # let it dismiss
- assertNotVisible: { id: "modal-sheet-dismiss-btn" }
```

**Compose gotcha**: BottomSheet content + AlertDialog buttons need their own `.semantics { testTagsAsResourceId = true }`. If you write a new screen, add this on each modal content root + per AlertDialog button. See [03-selectors.md](03-selectors.md) common pitfalls.

## Stacked modals (3-level)

```yaml
- tapOn: { id: "tab-stacked" }
- tapOn: { id: "stacked-open-l1-btn" }
- waitForAnimationToEnd
- tapOn: { id: "stacked-l1-open-alert-btn" }
- waitForAnimationToEnd
- tapOn: { id: "stacked-l2-continue-btn" }
- waitForAnimationToEnd
- tapOn: { id: "stacked-l3-pay-btn" }

# After all 3 dismiss, trail label should read "completed"
- waitForAnimationToEnd
- assertVisible:
    id: "stacked-trail-label"
    text: ".*completed.*"
```

## Deep navigation (multi-level nav stack)

```yaml
- tapOn: { id: "tab-deepnav" }

# Drill down
- tapOn: { id: "deepnav-l1-movies-btn" }
- tapOn: { id: "deepnav-l2-action-btn" }
- tapOn: { id: "deepnav-l3-item3-btn" }
- tapOn: { id: "deepnav-l4-edit-btn" }

# At L5, edit + save
- tapOn: { id: "deepnav-l5-text-input" }
- eraseText
- inputText: "MyEdit"
- tapOn: { id: "deepnav-l5-save-btn" }

# Save pops back to L1, state propagated
- waitForAnimationToEnd
- assertVisible: { id: "deepnav-l1-saved-label", text: "saved: MyEdit" }
```

## Long list — scroll to specific row

```yaml
- tapOn: { id: "tab-heavylist" }

# Approach 1: jump-to-row via numeric input (fastest)
- tapOn: { id: "heavylist-jump-input" }
- inputText: "5000"
- hideKeyboard
- tapOn: { id: "heavylist-jump-btn" }
- waitForAnimationToEnd
- assertVisible: { id: "heavylist-row-5000-title" }

# Approach 2: scrollUntilVisible (slower but no helper required)
- scrollUntilVisible:
    element: { id: "heavylist-row-9999-title" }
    direction: DOWN
    timeout: 60000
- tapOn: { id: "heavylist-row-9999-btn" }
```

## OCR fallback for unlabeled buttons

```yaml
- tapOn: { id: "tab-ocr" }

# Vision OCR finds "Submit" via on-screen text rendering
- tapOn: { ocrText: "Submit", recognition_level: accurate }
- assertVisible: { id: "ocr-last-tapped", text: "tapped: Submit" }
```

## Permission grant (declarative)

```yaml
appId: com.example.app
---
- setPermissions:
    camera: allow
    photos: allow
    notifications: allow
    location: allow
- launchApp:
    clearState: true
- tapOn: { id: "tab-perm" }
- tapOn: { id: "perm-camera-btn" }

# Should NOT see a permission dialog since we pre-granted
- assertNotVisible: { text: "Allow.*to access your Camera" }
- assertVisible: { id: "perm-status-label", text: ".*granted.*" }
```

## WebView eval

```yaml
- tapOn: { id: "tab-webview" }
- waitForAnimationToEnd               # let WebView load inline HTML

# Set input + submit via JS
- webViewEval: |
    document.getElementById('user-input').value = 'alice';
    submitForm();

# Read back the result
- webViewEval: |
    document.getElementById('form-result').textContent
# Returns "submitted=alice"
```

## Cross-locale text (no testid)

```yaml
- tapOn: { id: "tab-localized" }

# Tap "Submit" (en), "送信" (ja), "Enviar" (es) — whichever is current
- tapOn:
    localizedText:
      en: "Submit"
      ja: "送信"
      es: "Enviar"
```

## Anchor / relative selection

```yaml
# Tap the button immediately to the right of "Email" label
- tapOn:
    role: button
    rightOf: "Email"

# Tap the gear icon offset slightly from a known anchor
- tapOn:
    anchorRelative:
      anchor: "Header"
      dx: 0.45
      dy: 0
```

## Conditional flow (login if needed, else skip)

```yaml
- runFlow:
    when:
      visible: "Log in"                  # enter only when login is shown
    file: "../subflows/login.yaml"

# Inverse gate — run a setup ceremony only if its end state isn't
# already on screen (idempotent across a batch):
- runFlow:
    when:
      notVisible: { id: "qa-bubble" }
    file: "../subflows/enter-qa-mode.yaml"
```

## Retry flaky step

```yaml
- retry:
    maxRetries: 3
    commands:
      - tapOn: { id: "home-show-alert-btn" }
      - assertVisible: { id: "home-alert-ok-btn" }
```

## Sub-flow include (DRY)

`subflows/launch-fresh.yaml`:
```yaml
appId: com.example.app
---
- launchApp:
    clearState: true
    clearKeychain: true
```

main YAML:
```yaml
- runFlow: ./subflows/launch-fresh.yaml
- tapOn: { id: "tab-home" }
```

## iOS-only / Android-only sections

For YAMLs that primarily run cross-platform but have a few platform-specific bits:

```yaml
# Both
- tapOn: { id: "tab-deeplink" }

# iOS-only — openLink to myapp://...
- openLink:
    link: "myapp://home/details/42"
- assertVisible: { id: "deeplink-target-label", text: ".*home/details/42.*" }

# Android-only — back navigation (its own verb, not a key press)
- back
```

## Photograph something that hides itself

A control bar that appears on tap and disappears a few seconds later
outlives neither a second command nor the turn between two tool calls.
The usual workaround is to change the app so it stays up long enough to
photograph, which means the thing you photographed is not the thing that
ships.

Take the frame in the same call as the tap:

```bash
smix tap id:player-surface --then-screenshot /tmp/controls.png
# tapped: id:player-surface — frame via runner 91 ms later, 84213 bytes to /tmp/controls.png
```

Over MCP, the same thing is `smix_tap_then_screenshot`, which answers
with the delay and then the PNG.

What this buys is not wire speed. A tap is about 336 ms and a frame from
the runner about 88 ms — both together fit inside a UI that lives three
seconds. What it removes is the round trip between two calls, which is
where the time actually went.

Two things to know:

- **A tap that fails writes nothing.** A frame taken after a tap that
  did not land is a picture of the screen nothing happened on, and it
  looks exactly like evidence.
- **It needs a selector the tree can resolve.** `point:` and `ocrText:`
  are dispatched without resolving a target, so there is nothing to say
  about where the touch landed. A `fallback:` chain is fine — the first
  layer that is on screen is the one tapped.

## Performance: skip launchApp for fast iteration

YAMLs that don't need a fresh app state can avoid the `launchApp` cost (3-5s for cold start):

```bash
smix run --no-launch ...    # skip launchApp step entirely
```

Use when:
- Running many small YAMLs back-to-back against the same app
- Debugging a single YAML (rerun without restart)

## Performance: pre-warm the runner

The first `smix run` on a freshly-up runner pays ~2s of XCUITest cold start. Subsequent runs reuse. Don't tear down + bring up between YAMLs in a smoke loop:

```bash
# WRONG (slow):
for f in *.yaml; do
  smix runner up <udid> --bundle com.example.app
  smix run "$f"
  smix runner down
done

# RIGHT (fast):
smix runner up <udid> --bundle com.example.app
for f in *.yaml; do smix run "$f"; done
smix runner down
```

## testid naming for a new screen

If you add a screen to the app under test, stick to `<screen>-<element>-<kind>`:

- `foo-screen` — container
- `foo-title-label` — text
- `foo-submit-btn` — button
- `foo-input-name` — text input

## Expo dev-client: deep-link replay after JS reloads

If your app-under-test is an Expo dev-client build and your flows drive
state through custom-scheme deep links (`myapp://…`), you will
eventually see a link you sent earlier get **re-delivered after a later
JS bundle reload** — re-opening a panel or re-firing an action over the
screen your next step asserts.

**Mechanism** (expo-dev-launcher source): any custom-scheme URL that
arrives while the React host is not running — including the window
between a dev-client relaunch's two boots (embedded file bundle, then
the metro bundle) — is stashed in `EXDevLauncherPendingDeepLinkRegistry`,
an **in-memory** registry inside the app process. The NEXT React host
start consumes it via `getLaunchOptions` and injects the URL as the
router's initial URL. It is your own in-flight URL re-emerging one boot
later, not stale persisted state.

**What does NOT work**:

- `clearUserDefaults` — the registry is in-memory; there is no persisted
  key to delete.
- A hypothetical runner-side "drain the queued URL" — the registry is
  private in-process state; nothing outside the app (XCUITest, simctl)
  can reach it.
- Flow-side "neutralizer" URLs before/after the relaunch — the queued
  URL is always delivered after the boot, so it always lands after
  anything you send; the ordering race is structural.

**What works** (pick per cost):

1. **Process-level relaunch** — `stopApp` then `launchApp` instead of a
   JS-level reload. Terminating the process destroys the in-memory
   registry. Costs the dev-launcher ceremony on the next launch (can be
   15–30 s on a dev-client); right when your ceremony budget allows it.
2. **App-side replay gate** — dev-mode code that tags flow-sent links
   (e.g. a nonce query param) and drops any second delivery of the same
   nonce. Zero runtime cost, needs app cooperation; the most durable
   option for a QA-instrumented app.
3. **Overlay-tolerant assertions** — accept that the replayed action may
   fire and make the subsequent asserts robust to it (e.g. a terminal
   `close-panel` + text tiers that don't require exclusive screen
   ownership). Works today with zero code, at the cost of flow-author
   vigilance.

## See also

- [02-yaml-reference.md](02-yaml-reference.md) — full grammar
- [03-selectors.md](03-selectors.md) — all selector forms
- [06-fixtures.md](06-fixtures.md) — a testTag layout example
- [07-errors.md](07-errors.md) — when something goes wrong
