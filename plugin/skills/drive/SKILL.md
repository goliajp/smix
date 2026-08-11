---
description: Drive an iOS Simulator or Android emulator — pick a device, bring the runner up, look at the screen, act on it. Use when asked to run, tap, type into, screenshot, or check anything in an app on a simulator or emulator.
---

# Driving a simulator with smix

## Before anything else

Nothing drives until a device is bound to this session.

1. `smix_devices` — what is available, with UDIDs and state.
2. `smix_use` with the UDID and the app's bundle id. This boots the device
   if needed, starts the runner, and opens the session the other tools go
   through. It takes the length of a build the first time.

Calling a sense or act tool before that gets refused by name, not by a
connection error — if you see that refusal, the fix is `smix_use`.

To switch devices mid-session, call `smix_use` again. To stop, `smix_release`.

## Look before you act

The screen is not what you remember it being. Sense first:

- `smix_describe` — the screen as a summary, best for deciding what to do.
- `smix_tree` — the full accessibility tree, when a selector is not matching
  and you need to see what is actually there.
- `smix_find` — does this selector resolve, yes or no. Cheap; use it to
  check an assumption before acting on it.

Then act: `smix_tap`, `smix_fill`, `smix_swipe`, `smix_scroll`,
`smix_press_key`.

`smix_fill` replaces what the field holds. Coming back to a form and
filling a field again leaves the second value, not both concatenated —
which used to be the failure mode, and it is invisible in a password
field, so it read as a login rejecting a correct password.

## Selectors

Prefer an accessibility identifier — it survives copy changes and
translation. `{"id": "submit-button"}`. Failing that, `text` or `label`.

A tap reports what it landed in. If it says `TAP_MISSED`, the coordinate
was resolved from a screen that had moved by the time the touch arrived —
sense again and retry rather than tapping harder.

## Screenshots

`smix_screenshot` when the tree is not telling you enough — a scrim, a
keyboard, or an overlay covering the target is visible in a picture and
invisible to a selector query. It is also what to reach for when a tap
reported success and nothing seems to have happened.

## On an Android emulator

The tools above are the iOS Simulator path: `smix_devices` and `smix_use`
go through simctl, and there is no emulator in what they list. Android is
driven from the terminal, and everything below is a real command.

```bash
smix sim register android --udid emulator-5554 --kind emulator \
  --runner-port 22091            # says which store it wrote to
smix runner up emulator-5554 --platform android --runner-port 22091
```

`--platform android` is not optional and does not default: without it the
runner comes up through Xcode, on a device that has none. The first
`runner up` on a machine extracts the runner project that ships with smix
and builds its instrumentation APK, which takes a gradle build's worth of
time; after that it is already there. An Android SDK is required, the way
the iOS side requires Xcode.

Then sense and act as usual, naming the serial:

```bash
SMIX_RUNNER_PORT=22091 smix tree --device emulator-5554
SMIX_RUNNER_PORT=22091 smix tap "id:submit-button" --device emulator-5554
smix runner down --platform android --device emulator-5554
```

Two things that look like device problems and are not:

- `smix capsule up` refuses an emulator, and says so. It is simulator
  machinery — the Simulator.app guard, `simctl boot`, the `/live` capture
  — and none of it has an emulator counterpart. `runner up` above is the
  whole bring-up.
- A screen that never goes idle, a video player above all, does not stop
  the tree from being readable. smix waits a bounded moment for the
  accessibility stream to settle and then reads regardless, which is why
  it works where `adb shell uiautomator dump` refuses with "could not get
  idle state".
- **Do not run `uiautomator dump` while the runner is up.** One process
  owns UiAutomation, so the dump throws `already registered!` — and it
  leaves `/sdcard/ui.xml` holding the *previous* dump, so a caller that
  reads the file gets a screen from minutes ago and taps coordinates
  that moved. Somebody landed in system Settings twice before working
  that out. Use `smix tree`; if you must dump, stop the runner first
  (`smix runner down --platform android --device <serial>`).

## The same thing from a terminal

Every tool here has a command form, and they are interchangeable —
`smix find`, `smix tap`, `smix fill`, `smix tree`. Nothing in this plugin
can do something the CLI cannot; use whichever suits the moment.

If the user has no device registered yet, `smix doctor` prints the next
command to run at every point, and `smix init --device <UDID> --app ./App.app`
registers one and installs the app in a single step.

## Before you take a device somebody else may be using

A machine can have several simulators up, and the ones that are not
yours look exactly like the ones that are. `smix runner list` says which
runners exist here, on which ports, and whether the ledgers know about
them; `smix lease owner <device>` answers whether smix booted that one
(exit 0) or nobody here did (exit 3).

Ask before you start a runner on a device you did not boot. A runner
started on somebody's simulator takes over the app they had open — this
is not hypothetical, it is why those two commands exist.
