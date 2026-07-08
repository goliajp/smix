# smix-metro-log

Metro / expo log tail + ring buffer + signal await for [smix](https://github.com/goliajp/smix).

Powers the yaml verbs `expect.signal` and `expect.signals` in `smix-adapter-maestro` and the CLI flag `--expect-log-clean`.

## Model

```
metro dev server ──WS──▶ MetroLogTail ──▶ ring buffer ──▶ await_signal
                                      └▶ allowlist  ──▶ assert_clean
```

- Ring buffer retains the last `retain_secs` seconds of entries (default 300)
- `await_signal` scans at 25 ms intervals; three window shapes: `SinceRun`, `SinceMs`, `LastMs`
- `await_signals` supports `Any` (unordered) and `Strict` (in-order) semantics
- `assert_clean` returns non-allowlisted warn/error entries (empty = clean)

## Non-goals

- Multi-source log merging (single source per `.smix/config.json`)
- Structured JSON log field querying (regex over `message` only)
