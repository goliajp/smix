# RFCs

Internal design-decision records that scope each release. Each RFC is the source of truth for what a version's charter includes, why the decisions were made, and where the alternative paths were considered.

RFCs are **living planning docs** until the release ships; after ship they are **frozen history** — treat them as read-only unless a follow-up fix requires an addendum, in which case add a new dated section at the end rather than rewriting.

## Index

- [`1.0.2-runner-boundary-contract.md`](./1.0.2-runner-boundary-contract.md) — activation storm root-cause + pixel-forensic on the "hub-form.png" anomaly (2026-07-09).
- [`1.0.4-sim-health-and-backpressure.md`](./1.0.4-sim-health-and-backpressure.md) — studio-protection release. SimRenderServer stress fix + full closure of the 2026-07-10 gate-hardening feedback (§A-§I) + lifecycle safe-exit primitives (2026-07-11).
- [`1.0.5-supervisor-and-persistence.md`](./1.0.5-supervisor-and-persistence.md) — DRAFT. Session persistence across XCTest lifecycle (§E ask 2) + host-side supervisor daemon (§D6) + idle-close 60s + real-sim smoke gate.

## Cadence

- RFCs are numbered by the release they land in (`<version>-<slug>.md`).
- Skip numbers reflect "no design-decision-record needed for that version" — v1.0.0/v1.0.1/v1.0.3 shipped without RFCs because their scope was small and captured in CHANGELOG + PR descriptions. v1.0.2 was the first release complex enough to warrant one.
- Each RFC's status line at the top tracks: `DRAFT` → `IMPLEMENTING` → `SHIPPED` → (optionally) `REVISED after ship`.

## When to write one

Write an RFC when the release scope has any of:

- Multiple interacting decisions where the ordering matters.
- A wire-shape decision that other SDKs will implement to.
- A forensic finding that changes the problem definition mid-flight.
- A "we thought X but discovered Y" pivot worth recording so the next reader doesn't repeat the wrong branch.

Skip the RFC when the release is a straight bug-fix or a mechanical bump that CHANGELOG can carry alone.

See [`docs/roadmap.md`](../roadmap.md) for the version cadence.
