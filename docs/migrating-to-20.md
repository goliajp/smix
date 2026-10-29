# Migrating to smix 20.0.0

20.0.0 is a major release because a public Rust struct gained two public
fields. Flows, the CLI and the SDKs need no change to keep working.

If you only write YAML or drive smix from the CLI, nothing changes:
`--settle-max-ms`, `--settle-frames` and `--give-up-when-still-ms` are new,
and without them a run behaves as before, apart from waits that look more
often in their first second and iOS fills that go in one request.

---

## Rust API

- `smix_adapter_maestro::FlowArgs` has two new public fields:
  `settle: Option<smix_driver::settle::Settle>` and
  `give_up_when_still: Option<std::time::Duration>`. Code that builds
  `FlowArgs` with a struct literal adds both; `None` for each keeps the
  earlier behaviour.
- New in `smix_driver`: `settle::Settle`, `settle::until_settled`,
  `poll::Stillness`, and the `Driver` methods `set_settle` and
  `set_give_up_when_still`, both with a default that does nothing, so an
  outside `Driver` keeps compiling.
- New in `smix_sdk`: `App::with_settle` and `App::with_give_up_when_still`.

## Runners

`POST /fill` on the iOS runner takes `typingFrequency` (1 to 200 keys a
second, 200 if left out), and a host at 20.0.0 sends the whole text in one
request at 10. An iOS runner from an earlier release ignores
`typingFrequency` and types the whole text at its own speed, which a React
Native field can drop keys at. Bring the runner up from 20.0.0.
