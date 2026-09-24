# Migrating to smix 11.0.0

Most flows need no change. What follows is everything that can make a
flow or a program behave differently than it did on 10.x, in the order
you are likely to meet it.

If you only write YAML, read **What answers differently** and stop.
**Rust API** is for code that calls the crates.

---

## What answers differently

### A key smix does not act on is now a parse error

In `runFlow:`, `repeat:`, `when:`, `while:` and selector maps, a key
smix does not read used to be dropped without a word. The common case
was a condition that did nothing:

```yaml
- runFlow:
    when:
      platform: Android      # read as "no condition at all" on 10.x
    file: android-only.yaml  # so this ran on iOS too
```

That flow ran everywhere and reported green. It now runs where it says
and, if a key is misspelt, stops at parse time naming the key and the
keys that are read there.

Three things that parsed on 10.x and do not now:

| Written | Why it stops |
|---|---|
| `platfrom:`, or any misspelling | Named as unknown, with the keys that are read |
| `when: { optional: true }` | maestro accepts it and never applies it; smix will not pretend to |
| `when: {}`, or `when: { label: x }` | A condition with nothing to check |
| A selector's unquoted `true:` or a numeric key | Same rule, in selector maps |

**What to do:** run your flows with `smix run --check <flow>`. It parses
without a device and names anything that has to change.

### `scrollUntilVisible` stops later than it used to

On 10.x it stopped as soon as the target was in the tree and overlapped
the screen at all — which is how a row crossing the bottom edge, with
its middle below it, ended a scroll and failed the `tapOn` after it with
`CentroidOutOfFrame`.

The rule is now maestro's: the visible share of the element must reach
`visibilityPercentage`, **100% by default**, and the content must have
stopped moving (two looks in the same place).

A flow that relied on the early stop will scroll further than it did.
A flow that genuinely wants a partly-visible target should say so:

```yaml
- scrollUntilVisible:
    element: { id: row }
    visibilityPercentage: 60
```

`visibilityPercentage`, `centerElement`, `timeout`, `label` and
`optional` all take effect now; on 10.x they were dropped in silence.
`speed` and `waitToSettleTimeoutMs` are refused by name — one swipe is a
fixed gesture on both runners.

The swipe-count limit (30) is gone. `timeout` was always the other
limit, and two limits are two stopping rules.

### A step that silently failed now fails

Three Android routes computed whether their events were injected and
wrote the answer somewhere the host does not read; three more discarded
it. A tap whose touch never went in arrived as a passing tap.

All six answer now. **A flow that was passing on an injection that
silently failed will fail where it happens** — this is a flow finding
out about a defect it always had, not a new restriction.

The same applies to two more answers: `/foreground` reports whether the
named package owns the foreground afterwards rather than that an
`am start` ran, and `/clear-text` reports whether the field is empty.

### A node the toolkit never placed is no longer visible

The Compose probe used to report a node's rectangle as
`positionOnScreen + size` — where the layout put it, whether or not any
of it shows — and reported nodes that had been measured but never
placed, the state a lazy list leaves a prefetched row in.

A node like that has only a position it has never had. It is no longer
reported at all, and a half-scrolled row now reports the half that
shows.

**A flow that asserted such a node was visible was passing on something
that was not on screen, and will now fail.** Both halves need the probe
upgraded to take effect (`debugImplementation("jp.golia.smix:smix-probe:11.0.0")`);
an older probe is read exactly as before.

### `back` on Android answers whether anything went back

It used to answer `UiDevice.pressBack()`, whose boolean is neither the
key going in nor the screen changing — a ticking clock satisfied it. It
now injects the key and reads the screen afterwards. The reply carries
`settledBy` (`screenChanged` / `couldNotSee` / `gaveUp` / `notInjected`)
and keeps the old boolean as `injected`, because "the key never went in"
and "it went in and nothing moved" are different problems.

### `setOrientation: portraitUpsideDown` turns the display over

It was emulated as two left rotations, which lands at rotation 1, not 2
— and nothing checked, so the verb reported success for something else.
Every orientation is now read back before the route answers.

### `assertScreenshot` and `takeScreenshot` refuse keys they do not read

maestro's `cropOn` and `thresholdPercentage` used to be walked past, so
a `cropOn` flow compared the whole frame and passed. Both are carried out
now. `label` and `optional` on either verb, and on `takeScreenshot` any
key but `path` / `name` / `annotate` / `cropOn`, stop at parse time by
name. A baseline recorded without `cropOn` is a whole frame; if you add
`cropOn` to an existing check, record its baseline again.

### A runner that could not answer is no longer "not visible"

`when: { visible: … }`, `when: { notVisible: … }` and `assertNotVisible`
took any failure to look as the element being absent — a runner that
answered half a body skipped the block, and the run went green. They now
fail at once as `DRIVER_ERROR`. In `smix run --format json`,
`failure.judgesTheScreen` says which kind a failure is: `true` is an
answer about your app, `false` is smix unable to look.

### `pressKey: lock / volumeUp / volumeDown` on iOS fails by name

These were skipped on every platform. They are pressed on Android now,
and on iOS the step fails saying why: XCUIDevice has no lock button, the
simulator has no volume buttons, and on a physical iPhone this has not
been measured. A cross-platform flow presses them under a platform
condition:

```yaml
- runFlow:
    when:
      platform: Android
    commands:
      - pressKey: volumeUp
```

---

## Rust API

### `Permission`, not `SimctlPermission`

`LaunchAppOptions.permissions`, `App::set_permission` and
`App::set_permissions` take the cross-platform `Permission`:

```rust
// 10.x
opts.permissions = vec![(SimctlPermission::Camera, PermissionAction::Grant)];

// 11.0
opts.permissions = vec![(Permission::Camera, PermissionAction::Grant)];
```

The whole path from the yaml key to the backend was typed as the iOS
enum, so `storage` — implemented in the Android backend — could not be
named from a flow. `Permission::from_simctl` is gone with its last
caller.

### The scroll surface moved

```rust
// 10.x
driver.scroll(&selector, direction).await?;

// 11.0
smix_driver::scroll_until(&driver, &selector, direction, &ScrollUntil {
    reach: Reach::default(),      // wholly visible
    timeout: Duration::from_secs(20),
}).await?;
```

`Driver::scroll` is gone; `Driver` gains `confirm_on_screen`.
`App::scroll` and `AppLike::scroll` take a `&ScrollUntil`. There were
three host-side loops and the fix for the defect above had only ever
been in one of them; there is now one, in `smix_driver::scroll_until`,
with the stop rule as a pure function
(`smix_host_coord_resolver::verdict`).

### Step shapes

`Step::RunFlowConditional` and `Step::RunFlowInline` carry
`when: Option<FlowCondition>`, `env` and `opts: BlockOptions` in place of
`when_visible` / `when_not_visible`.

`Step::Repeat` carries `times` / `while_` / `while_expr` / `opts`, and
**`RepeatMode` is gone** — the enum encoded an either-or that 11.0
removes, since maestro runs a body while the condition holds *and* the
count is unspent.

`Step::RunScript` carries `when` / `env` / `opts`.
`Step::ScrollUntilVisible` carries `until` and `opts`.

### `AppLike` has a new required method

```rust
fn platform(&self) -> smix_driver::Platform;
```

`when: { platform: … }` has to know what it is running on. Every
`AppLike` implementation must answer.

### `POST /scroll` is gone from the iOS runner

Scrolling to an element is one loop on the host. The runner-side loop
behind this route was a second implementation with its own idea of
"visible", and nothing called it. `RunnerScrollSelector` and
`ScrollResponse` go with it.

### `Step` is `#[non_exhaustive]`

Outside `smix-adapter-maestro`, a `match` on `Step` needs a `_` arm.
Two variants are new, `RememberBounds` and `AssertBoundsUnchanged`. From
here on a new flow verb is a minor release for you, not a major one.

`Step::PressKey` holds a `KeyName` (which gains `Back`), not a `String`;
`RunError::UnknownKey` is gone because an unknown key is a parse error
now. `KeyName::from_name` is the one reader;
`smix_cli::act::parse_key_name` is gone.

### Screenshots take one `ScreenshotCheck`

```rust
// 10.x
app.assert_screenshot(&baseline, max_hamming).await?;

// 11.0
app.assert_screenshot(&ScreenshotCheck {
    baseline: &baseline,
    compare: ScreenshotCompare::Hash { max_hamming },
    masks: &[],
    crop: None,
}).await?;
```

`ScreenshotCompare::Pixels { min_match_percent }` is maestro's
`thresholdPercentage`. `AssertScreenshotOutcome` gains
`MatchedPixels { percent }` and is no longer `Eq`. `AppLike` and `Driver`
gain a required `pixels_per_point`.

### Runner client and capsule

- `HttpRunnerClient::double_tap` and `long_press` are gone, with the iOS
  runner's `/double-tap` and `/long-press`. Use
  `double_tap_at_norm_coord` / `long_press_at_norm_coord`; both return
  the runner's `TapAtCoordResult`, which gains the held-touch bounds,
  `press()` and `complete`.
- `tap_landed_within` takes a `ChainCoverage`.
- `smix_capsule::runner_android::up_with` and `up_with_takeover` are
  replaced by `up_with_options(root, serial, port, &UpOptions)`.
  `parse_resumed_package` is gone; `smix_adb::parse_resumed_activity` is
  the one reader.
- `Driver::confirm_on_screen` returns `Result<bool, ExpectationFailure>`:
  a check the runner could not answer is an error, not a confirmation.

### Leases and cleanup

- `smix_lease::Resource` has a new variant, `Emulator { avd, console_log }`,
  and `Resource::Runner` has two optional fields, `bundle` and `log`.
  `AdbClient::start_emulator_on` takes the path to write the emulator's
  console to.
- The iOS runner's record is the device's lease.
  `smix_capsule::runner::down` and `down_including_unrecorded` take a port
  and no workspace root; `runner_state::{read, write, clear, Platform}`
  are replaced by `runner_state::find` and `legacy_evidence`.
- Cleanup takes no workspace root: `CleanupExecutor::execute`,
  `reconcile::execute`, `Leased::acquire` and `App::hold_device_lease`
  drop the parameter.

### Failure and tree fields

`A11yNode` has `window` and `unreadable_windows`; `FailureInit` and
`ExpectationFailure` have `visible_total`, `windows` and
`unreadable_windows`. A struct literal needs them (`None` / empty);
`FailureInit { ..Default::default() }` needs nothing.

---

## What did not change

Physical-device rules are unchanged: a device must be registered before
it can be addressed, destructive actions are refused per device until
allowed once, and a capability a phone does not have is a loud error.

Device records and leases are where 4.0 put them, with one addition:
the iOS runner's record, which used to be a file in the checkout, is now
the device's lease in the machine directory. A checkout's old file is
still read, as evidence only. No migration command is needed for this
release.
