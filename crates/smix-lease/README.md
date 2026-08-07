# smix-lease

The device resource ledger: who holds a device, what they opened on it,
and what is owed when they die without closing it.

smix could always tear down what it started — as long as it was the one
doing the tearing. Everything else (Ctrl-C on `xcodebuild`, closing
Simulator.app, an IDE restart, a CI timeout, another agent's `pkill`)
killed the process holding a runner, a recording and a booted simulator,
and nothing remembered those three things had existed. The runner died by
SIGKILL rather than the SIGINT that lets `testmanagerd` end the session
cleanly, so macOS put up a crash-report dialog; the recording lost its mp4
trailer; the simulator stayed booted.

This crate is the ledger that fixes the direction of the question. Instead
of "what running process looks like mine", the next smix command asks
"what did the last holder write down" — and closes exactly those things by
the graceful path the holder never got to take.

## What it does

- `Lease` / `Resource` — one file per device under `.smix/leases/`, recording
  each runner (iOS and Android), supervisor sidecar, screen recording, and
  boot the holder performed.
- `assess(&Facts) -> Admission` — pure. Granted / Denied / Reclaimable.
- `plan_cleanup(&Lease) -> Vec<CleanupAction>` — pure. The closes owed, in
  reverse of the order they were opened, with one exception: a supervisor
  goes first, because its job is to restart a runner it finds dead.
- `may_shut_down(Option<&Lease>) -> bool` — pure. Whether this workspace is
  entitled to turn the device off, which is not the same question as
  whether it knows how to address it.
- `store` — the I/O half: atomic writes, `ps`-backed process probes.

Executing a cleanup plan lives in `smix-capsule::reconcile`, which owns the
graceful teardowns this crate only names.

## Three decisions worth knowing

**Identity is (pid, start time), not a pid and not a command line.** A pid
outlives its process and gets reissued; every concurrent `smix run` shares
a command line. Only the pair pins a process, so every row carries it and
nothing is signalled without re-verifying it first.

**Occupancy is judged by resources, not by the holder.** `smix runner up`
spawns `xcodebuild` into its own process group and returns, so the holder
is gone within seconds while the runner keeps the device for hours.
Judging by the holder alone would let the next command treat a working
session as an orphan and tear it down.

**But a recording is not occupancy, and neither is a supervisor.** Only a
runner speaks for "somebody is using this device". A recording is a
session's *output*: count it as occupancy and a session killed
mid-recording leaves a `simctl` child writing into a file nothing will
ever send the SIGINT that makes it playable. A supervisor is the thing
that restarts a session, not the session.

## When this is the wrong crate

| You want | Use |
|---|---|
| To perform the teardown, not decide it | `smix-capsule::reconcile` |
| Runner process handles (`state.json`) | `smix-capsule::runner_state` |
| Device addressing (alias → UDID) | `smix-simctl::registry` |
