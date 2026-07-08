# smix-adapter-maestro

Maestro YAML adapter for smix — parses Maestro test flow YAML and
translates each `Step` into a `smix-sdk` action call, so any consumer
of Maestro YAML flows can shell out to smix instead of the Maestro
Kotlin CLI.

## Usage

```bash
SMIX_RUNNER_PORT=22087 \
SMIX_UDID=<udid> \
SMIX_BUNDLE_ID=com.example.app \
  smix-adapter-maestro run path/to/flow.yaml
```

The CLI reads the env vars, parses the yaml, walks each step, dispatches
to `smix-sdk` actions, and reports a junit-xml-compatible report on
stdout.

## Design

`smix-adapter-maestro` is a thin translation layer: the yaml schema is
Maestro's, `smix-sdk` is the runtime. Each `Step` variant maps to one or
more `smix-sdk` API calls (tap / fill / clear / wait / press_key / etc.).
The adapter holds zero business knowledge.

For the yaml schema, see the Maestro project documentation. For the
supported subset, see the `Step` enum in `src/lib.rs`.
