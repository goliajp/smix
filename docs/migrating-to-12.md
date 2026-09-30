# Migrating to smix 12.0.0

12.0.0 is a major release because of two changes to the Rust API. Flows,
the CLI and the SDKs need no change to keep working.

If you only write YAML or drive smix from the CLI, read **What answers
differently** and stop. **Rust API** is for code that calls the crates.

---

## What answers differently

- **`hideKeyboard` on iOS tries different gestures.** The swipe down on the
  keyboard is gone — the keyboard read it as slide typing and could type
  into the field. A drag from the focused field's scroll view into the
  keyboard runs before the two touches outside it. The failure's `tried`
  list names `drag-into-keyboard` where it used to name `swipe-down`; a
  script that matched on the old list needs the new name.
- **An iOS tap in either landscape lands where it was aimed.** In one of the
  two landscapes every tap used to reach the opposite corner of the screen
  and still report success. A flow that passed there only by accident, or
  that pressed a control with `dispatch: 'xcui'` to get around this, can go
  back to an ordinary tap.
- **An Android route that polls keeps to a time limit of its own.** When it
  runs out, the runner stops before its next step and answers
  `route_limit_spent`, naming the step it stopped at and how long each one
  took, rather than letting the host give up on a request that never
  answers. `clearText` passes that reason on.
- **An Android `back` waits for the screen to hold still before the key.**
  Sent while the previous step's navigation is still landing, it used to
  count that navigation as its own and answer `ok: true` for a key the app
  swallowed. It now answers by what the key did, and takes up to 1.5 s
  longer when the screen is still moving.
- **A `back` that did not go back fails with `TIMEOUT`, not `DRIVER_ERROR`.**
  `TIMEOUT` is for the key going in and the screen reading the same for the
  whole wait. `DRIVER_ERROR` stays for a back whose outcome smix could not
  read. The runner's own word (`gaveUp`, `couldNotSee`) is in the message
  now, with its readings.
- **`back` takes `optional:` and `label:`**, as maestro's does. Until now a
  mapping after `back` was accepted and ignored, so `optional: true` did
  nothing. An optional back that did not go back is reported as skipped;
  one smix could not read still fails.
- **An Android `back` inside one Compose activity answers `ok: true`.**
  Closing a screen drawn over another in the same activity used to answer
  `gaveUp`: the runner's quick readings stop above the Compose content. When
  they see no change, the whole tree before the key and after the wait
  decides. Such a back returns after the full 2 s wait rather than as soon
  as the screen changes.
- **`scrollUntilVisible` waits for the list to stop after each swipe**
  before judging the target, so it takes a little longer per swipe and
  makes fewer swipes in the same timeout. A flow that used
  `visibilityPercentage` below 100 only to stop on a tall item can go back
  to the default.
- **A keyboard wait that runs out on iOS** still names the minimization
  setting when it is on, but as a fact about the simulator and not as the
  reason for the timeout: a keyboard has been seen both shown and kept below
  the screen with it on.
- **`hideKeyboard` on iOS succeeds when the keyboard is below the screen**
  (minimized) instead of failing with `keyboard_did_not_close`. Focus stays
  on the field; the answer says which. A flow that pressed Enter to get
  past this can go back to `hideKeyboard`.

## Rust API

`smix_adapter_maestro::Step::Back` carries the step's options:
`Step::Back(BlockOptions)` where it was the unit variant `Step::Back`. Build
a plain one with `Step::Back(BlockOptions::default())` and match it with
`Step::Back(_)` or `Step::Back(opts)`.

`smix_runner_client::route_limits::Route` gains a public field,
`android_looks: u64` — how many polls the Android handler can reach, each
adding one `ANDROID_LOOK_MS` to the time the host waits. `Route` and
`Longest` are now `#[non_exhaustive]`: code outside the crate can read a
`Route`'s fields but cannot build one with a struct literal, and a `match`
on `Longest` needs a wildcard arm. To get the time the host waits for a
route, call `route_wait` (fixed routes) or `wait_for_request` (routes whose
wait the request sets) rather than rebuilding the sum.
