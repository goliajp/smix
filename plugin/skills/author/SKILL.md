---
description: Turn a session of manual driving into a flow that can be re-run. Use when asked to record what was just done, write a test for it, or make a repeatable check out of a sequence of taps.
---

# Turning a session into a flow

What was driven by hand disappears when the conversation ends. A flow is
the same sequence as a file, and `smix run` replays it.

## Record what is on screen

`smix authoring record <out.yaml> --app-id <bundle-id> --duration-secs 10`

It samples the accessibility tree for the duration and writes a flow of
`assertVisible` steps for the identifiers that stayed put. Two things
decide whether the result is useful:

- **Record from the state worth asserting.** If a keyboard is up, its
  identifiers are on screen and get recorded, and they will be gone when
  the flow replays from a fresh launch. Dismiss it first.
- **Pass `--app-id`.** Without it the flow names a placeholder app and
  cannot be run back without an edit.

## Run it back

`smix run <out.yaml> --device <alias>`

A recording that does not replay is worth knowing about immediately —
it usually means something transient was recorded as if it were stable.

## Flows are maestro-format yaml

They are ordinary files: read them, edit them, keep them in the repo next
to the code they cover. A recorded flow is a starting point, not a
finished test — the assertions it captures are the ones that were visible,
not necessarily the ones that matter.

## On Android

`smix authoring tap-record` records the actions themselves, not just what
was visible, because the Android runner emits them directly. On iOS that
leg is not wired yet, so recording there captures assertions.
