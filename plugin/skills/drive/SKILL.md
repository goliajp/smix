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

## Selectors

Prefer an accessibility identifier — it survives copy changes and
translation. `{"id": "submit-button"}`. Failing that, `text` or `label`.

A tap reports what it landed in. If it says `TAP_MISSED`, the coordinate
was resolved from a screen that had moved by the time the touch arrived —
sense again and retry rather than tapping harder.

## The same thing from a terminal

Every tool here has a command form, and they are interchangeable —
`smix find`, `smix tap`, `smix fill`, `smix tree`. Nothing in this plugin
can do something the CLI cannot; use whichever suits the moment.

If the user has no device registered yet, `smix doctor` prints the next
command to run at every point, and `smix init --device <UDID> --app ./App.app`
registers one and installs the app in a single step.
