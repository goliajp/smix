# Migrating to smix 17.0.0

17.0.0 is a major release because two public Rust traits gained a required
method. Flows, the CLI and the SDKs need no change to keep working.

If you only write YAML or drive smix from the CLI, nothing changes: `pinch`
is new and nothing that worked before answers differently. **Rust API** is
for code that implements smix's traits.

---

## Rust API

- `smix_driver::Driver` has a new required method
  `async fn pinch(&self, pinch: &smix_input::Pinch) -> Result<(), ExpectationFailure>`.
  A `Driver` implemented outside smix must add it. There is no default: an
  implementation that cannot pinch should return an `ExpectationFailure`
  that says so.
- `smix_adapter_maestro::AppLike` has the same new required method. Test
  doubles that implement it add a `pinch` that records or ignores the call.
- `smix_adapter_maestro::Step` has a new variant `Pinch { at, spans,
  duration_ms, pan }`, and there is a new `PinchAt` enum. `Step` was already
  `#[non_exhaustive]`; a `match` with a wildcard arm keeps compiling.
- New in `smix_input`: `Pinch`, `Pan`, `PinchSpans`, and the constants
  `PINCH_SCALE_SPAN`, `PINCH_DEFAULT_DURATION_MS`, `PINCH_DEFAULT_PAN_MS`,
  `PINCH_DURATION_RANGE`.
- New in `smix_runner_client`: `HttpRunnerClient::pinch_at_norm_coord`.
- New in `smix_sdk`: `App::pinch`.

## Runners

Both runners serve a new route, `POST /pinch-at-norm-coord`. A host at
17.0.0 talking to a runner from an earlier release gets a 404 for `pinch`
and nothing else changes; bring the runner up from 17.0.0 to pinch.
