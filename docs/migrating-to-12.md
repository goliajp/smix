# Migrating to smix 12.0.0

12.0.0 is a major release because of one change to the Rust API. Flows,
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
- **A keyboard wait that runs out on iOS** still names the minimization
  setting when it is on, but as a fact about the simulator and not as the
  reason for the timeout: a keyboard appears with it on as well.

## Rust API

`smix_runner_client::route_limits::Route` gains a public field,
`android_looks: u64` — how many polls the Android handler can reach, each
adding one `ANDROID_LOOK_MS` to the time the host waits. `Route` and
`Longest` are now `#[non_exhaustive]`: code outside the crate can read a
`Route`'s fields but cannot build one with a struct literal, and a `match`
on `Longest` needs a wildcard arm. To get the time the host waits for a
route, call `route_wait` (fixed routes) or `wait_for_request` (routes whose
wait the request sets) rather than rebuilding the sum.
