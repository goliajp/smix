# smix-sdk performance budgets

Regression-catch budgets for the user-facing SDK surface (`App` plus
ergonomic selector helpers). The SDK sits between user test code and
`smix-driver`; it is allocation-light on its own (every hot method
delegates to the driver), but `App::connect_to_runner` is the cold-start
gate for a fresh session and `App::launch` defines the per-app boot
floor.

Run `cargo bench -p smix-sdk --bench perf_gate` for the criterion
baseline.

## Path taxonomy

`App::tap` / `App::fill` / `App::press_key` are the **per-step hot path**
— pure delegation to the driver. `App::connect_to_runner` is the
**per-session cold-start path** — one TCP/HTTP handshake plus driver
construction. `App::launch` is the **per-app boot path** — one simctl
launch shell-out plus a brief settle.

| Path | Hot? | Budget rationale |
|---|---|---|
| `App::tap` / `App::fill` / `App::press_key` (per-method dispatch) | per-step | < 1 ms SDK overhead on top of driver |
| `App::connect_to_runner` (TCP/HTTP handshake) | per-session | < 200 ms incl. retry/backoff |
| `App::launch` (simctl boot + settle) | per-app | < 5 s budget; simctl-dominated |
| `App::wait_for` (poll loop) | per-step | 5 s total budget, delegated to driver |

## Budgets

| Path | Budget | Observed P50 | Headroom |
|---|---:|---:|---:|
| `App::tap` SDK dispatch (driver mocked) | < 1 ms | TBD | TBD |
| `App::connect_to_runner` (loopback mock) | < 200 ms | TBD | TBD |
| `text("...")` / `id("...")` / `role(...)` selector helpers | < 200 ns | TBD | TBD |

## Memory

- Selector helpers (`text` / `id` / `label` / `role` / `role_named`)
  allocate one `String` + the wrapping `Selector` enum slot
  (~64 bytes per helper invocation).
- `App` is a thin handle: one `SimctlDriver` + one `SimctlClient` +
  one `Option<String>` UDID.
- Per-step SDK methods add zero allocations on top of the driver call.

## When to re-measure

- Adding a new public SDK method or selector helper.
- Touching `App::connect_to_runner` handshake / retry behavior.
- After upgrading `smix-driver` or `smix-simctl`.

