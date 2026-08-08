---
description: Work out why a step failed against a simulator or emulator — read the failure, look at the screen, and decide what to change. Use when a tap, assertion, or flow step fails and the reason is not obvious.
---

# Reading a smix failure

A smix failure is written to be acted on. Before changing anything, read
what it already told you.

## What the failure carries

- **code** — `NOT_VISIBLE`, `TAP_MISSED`, `DRIVER_ERROR` and so on. The
  code says which kind of wrong this is, and they want different fixes.
- **selector** — what was asked for, verbatim.
- **suggestions** — near misses ranked by similarity, with the field each
  matched on. A suggestion at similarity 1.00 on `id` means the element is
  there and something else about the query is wrong.
- **visibleElements** — what was on screen, filtered to what a selector
  could name. If the thing you wanted is not in that list, it was not on
  screen, and no amount of retrying will find it.
- **smix** — the version that produced the message, so a behaviour you hit
  can be checked against the release it came from.

## The three that mean different things

- `NOT_VISIBLE` — the selector resolved to nothing. Compare against
  `visibleElements`: usually a wrong identifier, or a screen that has not
  arrived yet.
- `TAP_MISSED` — the element was found, and the touch landed somewhere
  else. The screen moved between resolving and tapping. Sense again first.
- `DRIVER_ERROR` — the runner or session is not in the state the call
  needs. `smix_use` re-establishes both.

## Look at it

`smix_screenshot` when the tree is not telling you enough — a scrim, an
overlay, or a keyboard covering the target shows up in a picture and not
in a selector query.

`smix_tree` for the full picture when `smix_describe` has summarised away
whatever matters.

## A simulator and an emulator read the same

None of the above is iOS-specific. The codes, the suggestions and
`visibleElements` are filled in by the runner, and the Android runner
fills the same fields — so a `NOT_VISIBLE` means what it means on either.

## From a terminal

`smix run <flow> --debug-output <dir> --format json` writes the same
material to disk, which is what `smix authoring propose` reads when asked
to suggest an amended flow.
