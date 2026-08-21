# smix-contract

What an app owes, with an id, so coverage can be reconciled.

The requirement is usually already written down — in the comment above a
flow, in a ticket, in a design note. What it does not have is an
identity, and without one nothing can answer "is this covered on both
platforms" except a person reading two test suites side by side and
remembering.

This crate is the identity and the arithmetic. It reads contracts and
per-platform claims and answers three sets: nobody claims, one platform
claims, both claim.

## What it does not say

**It reports who claimed, never who verified.** Those are different
words. A claim says a test suite means to cover a requirement. It does
not say the test is good, that it passed, or that the two platforms'
tests check the same thing — the last of which is not mechanically
decidable. Claiming otherwise would make this one more cheap signal
standing in for the thing to be proven, which is the failure this
project keeps finding in its own code.

## Format

```yaml
- id: CTR-0001
  statement: Pausing notifications from the camera card, and taking it back
```

Refused, each by name and with where it was read:

- an entry with no `id` — it cannot be claimed by anything
- an entry with no `statement`, or an empty one — a present-but-empty
  field passes a check for the field and stands for nothing
- the same `id` twice — one id pointing at two requirements makes every
  claim on it ambiguous
- a claim naming an id no contract carries — a mistyped id leaves the
  requirement it meant looking unclaimed while somebody believes they
  are covering it

## Status

New in 6.6. The reconciliation half and the per-platform claim forms
land across that version's checkpoints.
