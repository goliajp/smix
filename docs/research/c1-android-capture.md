# C1 — Android event-capture obtainability (v2.10 recorder)

Can Android capture a tap/input event stream equivalent to iOS
`EventRecorder`, reconstructible into `smix-authoring-ir::IRAction`, from
inside the runner's existing instrumentation — no app-side service?

## Reference: what iOS captures

`swift-bridge/SmixRunnerUITests/EventRecorder.swift` swizzles
`XCAXClient_iOS.handleAccessibilityNotification:fromElement:payload:` — it taps
the OS-level **accessibility-notification** stream, NOT raw touch coordinates.
Each notification carries a binary-plist payload from which it lifts
`selectorHints` / `frame` / `elementType` / tap-centre `location`. Discriminators
seen: `hidevent`(1028)→tap, `firstresponder`(1018)→focus, `usertesting`(4002)→snapshot.
So the iOS leg is a **semantic AX-event** recorder, not an input-layer one.

The target IR (`crates/smix-authoring-ir/src/lib.rs:33`) is
`IRAction = Tap | Fill | Clear | PressKey | Swipe | GoBack | WaitFor |
HideKeyboard`. `WaitFor` is synthesised at generation time (a playback gate),
so it is not a captured action on any platform. The capture question is about
the other seven.

## Falsification rubric

Fixed before evidence — each axis is judged by whether it can passively deliver
a reconstructible stream, and the per-action verdict by whether a documented
Android event carries enough to rebuild that `IRAction`.

- **Axis-1 OBTAINABLE (AccessibilityEvent stream)**: a documented API delivers
  a passive `AccessibilityEvent` stream to the runner's existing instrumentation
  (no app-side `AccessibilityService`/manifest), AND at least Tap+Fill+Clear map
  to distinct event types carrying target identity. **NOT-OBTAINABLE-1**: no
  passive stream reaches instrumentation, or the events carry no target identity.
- **Axis-2 (UiAutomator)** OBTAINABLE iff `UiWatcher`/`UiDevice` expose a passive
  event stream (not poll/query). NOT-OBTAINABLE-2 iff query-only.
- **Axis-3 (instrumentation touch hook)** OBTAINABLE iff androidTest
  `Instrumentation` can intercept the SUT's `MotionEvent`s. NOT-OBTAINABLE-3 iff
  the process boundary blocks the SUT's input.
- **Per-action** (for the winning axis): OBTAINABLE iff a documented event type
  carries what the `IRAction` needs (target id + value); PARTIAL iff reconstructible
  but lossy; NOT iff no event represents it.
- Overall **NOT-OBTAINABLE** requires the full axis enumeration below to show no
  axis yields a passive, target-bearing stream (no-ceiling-words).

## Evidence

### Axis 1 — `UiAutomation` AccessibilityEvent stream — OBTAINABLE (core set)

- **Passive stream, inside the runner boundary**: `android.app.UiAutomation`
  (Android API) exposes `setOnAccessibilityEventListener(OnAccessibilityEventListener)`
  — a callback fired for every `AccessibilityEvent` the instrumentation's
  UiAutomation observes. The runner **already holds and configures**
  `inst.uiAutomation` and mutates `serviceInfo`
  (`android-runner/.../RunnerTest.kt:61-65`, `import android.app.UiAutomation:15`),
  so registering a listener is within the existing instrumentation — this is the
  clean structural equivalent of iOS's AX-notification swizzle, and critically it
  is **NOT** an app-side `AccessibilityService` (no service app, no manifest
  `<service>`, no app-side permission — the caller's boundary worry is void).
- **Event → IRAction map** (documented `AccessibilityEvent` types):
  | AccessibilityEvent | IRAction | carries |
  |---|---|---|
  | `TYPE_VIEW_CLICKED` | Tap | source node → viewIdResourceName (id) + bounds |
  | `TYPE_VIEW_TEXT_CHANGED` | Fill (non-empty) / Clear (→empty) | source id + text/before-text |
  | `TYPE_VIEW_FOCUSED` | (focus, like iOS 1018) | source id |
  | `TYPE_VIEW_SCROLLED` | Swipe | scroll delta — direction lossy |
  | `TYPE_WINDOW_STATE_CHANGED` | (GoBack/nav inference) | window transition, indirect |
- So **Tap / Fill / Clear are OBTAINABLE** — distinct event types, each with the
  source node's `viewIdResourceName` (the selector id the resolver already uses).

### Axis 2 — UiAutomator — NOT-OBTAINABLE

`UiWatcher`/`UiDevice` are active: watchers run only when `runWatchers()` fires
(on `waitForIdle`/query), and `UiDevice` is query/act, not a passive event
source. No stream to reconstruct a sequence from. Ruled out as a capture axis
(it stays the *act/query* layer).

### Axis 3 — instrumentation MotionEvent hook — NOT-OBTAINABLE

androidTest `Instrumentation` runs in the test process; the SUT (app under test)
is a separate process. Instrumentation touch callbacks see only events dispatched
*by the test*, not the user's `MotionEvent`s on the SUT. The process boundary
blocks raw-input capture of the subject. Ruled out.

### Per-action portability (the winning Axis-1)

- **Tap / Fill / Clear** — OBTAINABLE (above). This is the minimal portable set,
  and it already matches what the iOS leg reliably lifts (tap centre + text).
- **Swipe** — PARTIAL: `TYPE_VIEW_SCROLLED` fires but reports scroll position
  deltas, not a gesture vector; direction is inferable, distance/velocity lossy.
- **PressKey** — NOT via Axis-1: hardware/soft key presses do not emit a
  per-key `AccessibilityEvent` with the key identity; only their *effect* (text
  change / focus move) surfaces.
- **GoBack / HideKeyboard** — indirect only (`TYPE_WINDOW_STATE_CHANGED` /
  window-content transitions); not a first-class captured action.

## VERDICT

VERDICT: PARTIAL — Android CAN capture the core portable recording set
(Tap / Fill / Clear) via `UiAutomation.setOnAccessibilityEventListener` inside
the runner's existing instrumentation (no app-side AccessibilityService), the
clean structural equivalent of iOS's AX-notification swizzle; Swipe is lossy
(scroll deltas, not a gesture vector) and PressKey/GoBack/HideKeyboard have no
first-class AccessibilityEvent — those are a recorded gap, not captured. The
other two axes (UiAutomator = query-only; instrumentation hook = process
boundary) yield no passive stream.
