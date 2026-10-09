# Migrating to smix 13.0.0

13.0.0 is a major release because a public Rust enum gained a variant.
Flows, the CLI and the SDKs need no change to keep working.

If you only write YAML or drive smix from the CLI, read **What answers
differently** and stop. **Rust API** is for code that calls the crates.

---

## What answers differently

- **An Android emulator's animation scales are put back when a run ends.**
  `smix run` sets `window_animation_scale`, `transition_animation_scale` and
  `animator_duration_scale` to zero before its first step, as before, and now
  puts back what it found when the run ends — after a failed flow and after
  Ctrl-C too. A scale the emulator did not have is deleted rather than set
  to `1`. An emulator that earlier versions left at zero stays at zero until
  you set it: the first run of 13.0.0 takes what it finds as the device's
  own settings. If your own scripts restore any of the three after a run,
  they can stop.
- **A physical Android device's animation settings are left alone.** The
  run prints one line saying so and runs the flow with the phone's own
  settings. Earlier versions set them to zero on a phone and left them
  there.
- **A hidden Android view is not visible.** A View set `GONE` or
  `INVISIBLE` is no longer reported by the probe, so `assertNotVisible`
  passes on it and a tap is no longer judged to have landed on a hidden
  layer laid out over its target. A flow that skipped `assertNotVisible`
  on Android to get around this can run it on both platforms again, and
  `SMIX_TAP_HIT_MISMATCH=warn` set for that reason can go.
- **`runner up` on Android fails sooner, and says why.** A window of another
  package in front of the app is reported after three seconds, by package
  and window; an instrumentation that ends during bring-up stops the wait
  at once, and a UiAutomation connection another client holds is named.
  `runner up` does not dismiss anything on the screen.
- **`SMIX_RUNNER_UP_TIMEOUT_SECS` applies on Android** (default 180 s), and
  on both platforms a value that is not a whole number of seconds is an
  error.
- **A `scrollUntilVisible` that runs out of time** gives the time spent,
  the number of looks at the screen and how long each took; it no longer
  says a target it saw once was "still moving".

## Rust API

- `smix_capsule::android_foreground::Step` has a new variant, `Covered { by }`,
  and is `#[non_exhaustive]`: add a wildcard arm to a `match` on it.
- `DeviceControl::set_animations_quiet(serial, false)` on Android puts back
  the values recorded when `set_animations_quiet(serial, true)` was called,
  rather than writing `1`; with nothing recorded it writes nothing.
- New: `smix_lease::held` (the per-device record of settings a run holds)
  and `smix_sdk::android_serial_is_emulator`.
