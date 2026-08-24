# Migrating to smix 8.0.0

No verb was renamed and no CLI flag was removed. `swipe` gained an
`over:` form (see the 8.0.0 changelog); nothing that worked in 7.x reads
differently as YAML. If you write flows and run `smix`, the two things
that changed are behaviours, not syntax — read "What answers differently"
below and you are done.

If you depend on the Rust crates, four signatures changed. Each is
mechanical.

---

## What answers differently

### A port that reaches another device is refused

`smix run --device <A> --runner-port <P>` now asks who actually holds
`P` before it does anything: `adb forward --list` on Android, the process
listening on the port on Apple. If `P` reaches a different device, the
command stops and names both sides:

```
refusing to drive port 28090: port 28090 reaches emulator-5554, and the
command named emulator-5556. Whoever holds the forward decides which
device is driven, not the flag — so this would have acted on
emulator-5554.
```

Before this, `--device` was not checked against the port at all, and a
forward could point a named device's commands at a different machine.

Two cases deliberately do **not** refuse. If nothing on the machine can
be asked (a tunnel, a test double), the run proceeds and says on stderr
that the pairing went unverified. If you named no device and nothing
holds the port, the connection attempt reports "no runner is listening",
which is the more useful sentence.

### `inputText` on iOS refuses when nothing has focus

It used to report success — characters sent, no warnings, no field
changed — while Android refused the same flow by name. Both platforms
now refuse, in the same words. A flow that relied on the old behaviour
was not typing anywhere; add a `tapOn` for the field first.

### `waitForAnimationToEnd` tells "could not look" from "still moving"

Screen-capture backpressure inside the wait is absorbed and retried. A
capture that never succeeds is reported as such, rather than as the
screen still being in motion — the two used to be the same answer.

### `set_animations_quiet` no longer succeeds by default

Only relevant if you implement `DeviceControl` yourself. The trait's
default returned `Ok(())` without doing anything; it now refuses by
name. Override it with something real, or let the refusal stand.

---

## Rust API

### `App::connect_to_runner` and friends take the device you named

```rust
// 7.x
let app = App::connect_to_runner(22087).await?;
let app = App::connect_to_runner_android(28090).await?;
let app = App::connect_to_runner_lazy(22087);

// 8.0
let app = App::connect_to_runner(22087, Some(&udid)).await?;
let app = App::connect_to_runner_android(28090, Some("emulator-5554")).await?;
let app = App::connect_to_runner_lazy(22087, Some(&udid));
```

`None` keeps the old behaviour of not checking, and says so on stderr
when nothing can be asked. There is deliberately no unchecked overload:
one would become the call everybody uses.

### `Lease::resources` is `Vec<Row>`

A ledger row written by a newer smix now parses instead of stopping the
command dead, so the element type carries "something I cannot name":

```rust
// 7.x
for r in &lease.resources { match r { Resource::Runner { .. } => … } }

// 8.0
for r in lease.known_resources() { match r { Resource::Runner { .. } => … } }
```

`known_resources()` iterates what this binary understands;
`unnamed_kinds(&lease)` names the rest. Say which one you mean — "all of
them" and "the ones I understand" used to be the same expression, and
that is how a teardown came to look complete while skipping a row it
could not read.

### New variants on exhaustive enums

`Step::SwipeOver`, `Slot::SwipeOverTarget`, `Seen::ProcessGone`,
`CleanupAction::CannotClose`, `RunnerTransportError::WrongDevice`,
`FailureCode::CaptureBackpressure`. Exhaustive matches need the new arm.

`CleanupAction::CannotClose { kind }` carries no action because there is
none — it is in the plan so a teardown cannot report itself complete
while a row it could not read goes unmentioned. Executing it yields
`Outcome::Failed`.

### `smix_capsule::runner_view::attribute` takes one more argument

The live-pid set, so an Android runner row can be attributed rather than
answering `not-probed` forever.
