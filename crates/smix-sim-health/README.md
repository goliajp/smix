# smix-sim-health

Sim-side liveness sense layer.

Watches:

- `SimRenderServer` pid (macOS + iOS Simulator)
- `xcodebuild` test-host pid
- Last `/health` response age (fed by the runner client)
- Rolling window of `simctl io screenshot` wall times

State machine: `Healthy` → `Degraded` → `Dead` (and back). Transitions
broadcast on a `tokio::sync::broadcast` channel. Consumers subscribe;
the crate itself does not act — actions live in the runner client, the
simctl client, and the maestro adapter.

This is a **stone** crate — business-unaware. Its ABI is stable inside
`1.x`; additive-only.

Since smix 1.0.4.
