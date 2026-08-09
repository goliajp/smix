# Flows that need a retry sometimes, and what was tried

A flow listed here still runs, and a FAIL — both attempts failing — still
fails the gate. What it buys is that a FLAKE from these flows does not,
because their instability is measured, attributed as far as anyone has
managed, and written down here rather than rediscovered each release.

This is the fallback the v3.0 cold plan sanctioned: "if it cannot be
fixed, mark the flow known-unstable and remove it from the gate
explicitly — the removal goes in the decision log, never a silent skip."

Adding a row means filling in every column. `corpus-gate.sh` reads the
flow names; `scripts/dev/known-unstable-scan.py` refuses a row with no
measured rate or no attempts, because "known" without a number is just a
flow someone got tired of.

| Flow | Symptom | Measured rate | Attempts | Since |
|---|---|---|---|---|
| `nav-accessibility-and-back` | `assertVisible` after `back` finds the departing screen still up: `NOT_VISIBLE` on `com.apple.settings.primaryAppleAccount`, with `navigationBar id="Accessibility"` still in the tree. Passes on retry. | ~10% of runs (2/20, 1/10, 1/10, 0/10 across builds) | 4, all measured — see below | 2026-08-10 |

## `nav-accessibility-and-back` — what was tried

`back` reports success before the pop transition lands. Four attacks,
each measured over 10–30 corpus runs:

1. **The absent-bar shortcut.** `if !bar.exists { return true }` treated a
   navigation bar that could not be found as arrival, and during a pop
   the bar blinks out. Replaced with `NavigationSettle`, which requires
   three consecutive absences. Measured: 9/10 before, 9/10 after — no
   change. The line was a real defect and not this one.

2. **`noIdentity`.** If the title cannot be read before the tap, the
   handler sleeps and reports success without looking. Never
   implemented: the `settledBy` diagnostic showed every failure
   reporting `titleChanged`, so this branch was not involved.

3. **Requiring the departing bar to be gone.** The signal `back` watches
   is "a different title is visible", and during a pop both bars exist,
   so `firstMatch` can return the destination's while the old screen is
   still up. Gating on `navigationBars[before].exists == false` measured
   **2/30 clean** — far worse. Reverting only the decision left it at
   **2/10**; the rate recovered to 10/10 only when the extra query was
   removed too. **The criterion was innocent; one more accessibility
   round trip per 50ms poll was not.**

4. **Polling less often.** If an extra look hurts, look less: 50ms →
   250ms. Measured **1/11 clean** against a 10/10 baseline. Slowing down
   is worse too, so whatever the mechanism is, "observation pressure" is
   not a description a slower cadence relieves.

A fifth thing was tried and is not in the list because it changed no
code: an isolated probe that navigated in and back forty times, twice,
reading `back`'s reported branch against what was on screen a moment
later. Eighty navigations, zero misses. The flow only fails inside a
21-flow batch, which is consistent with the `X-Tree-Snapshot-Wall-Ms`
header's own note that the accessibility pipeline slows across a batch.
A probe that starts fresh each time cannot reach the state that matters,
so the diagnostic was moved into the client instead, where it reads the
branch at the moment of the real failure.

What is known: the transition is sensitive to being watched, in both
directions, and `back`'s answer and the caller's next read can disagree
about the same screen. What is not known: why.

The next attempt should not be a fifth threshold in the same handler.
Two rounds of tuning without moving the needle is the point at which the
rule here is to stop and decompose instead; this had four.
