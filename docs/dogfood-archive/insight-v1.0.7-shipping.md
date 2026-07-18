# smix v1.0.7 — shipping notes for insight (systemic response to v1.0.5 followup)

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-v1.0.5-followup.md`
RFC: `.claude/rfcs/1.0.7-observability-layer.md` (internal planning; source of truth for the design)

## Framing — what we did with your feedback

You reported three things:

- **Item A** — cold recompile after version bump exceeds 5 min, spawnSync 300 s timeout tripped.
- **Item B** — `xcrun simctl spawn exited 2: No such file or directory` mid-flow. No visibility into which simctl call, which argv.
- **Item D.3** — supervisor's `RunnerCycled` JSON events may have been buffered when the flow crashed.

We treated these as three symptoms of one system gap: **smix has been opaque about its own runtime**. Every subprocess failure returns `exited N: <stderr>` without the argv that failed, so you can't self-diagnose without waiting for a new patch. Long operations don't emit progress. Streaming events don't flush.

v1.0.7 ships a runtime observability layer that closes all three points plus the underlying gap.

## What lands in v1.0.7

### The item-B root cause (direct fix — retest immediately)

**`launchApp: clearState: true` was passing `"rm"` to `simctl spawn`**. On iOS 17+ sims, `simctl spawn` uses `posix_spawn` inside the sim OS and does NOT run PATH resolution. Bare `"rm"` fails `NSPOSIXErrorDomain code 2: No such file or directory` — this was the item-B failure surfacing on every `launchApp: clearState: true` in your bootstrap.

Fixed to `/bin/rm`. Also fixed `/usr/bin/defaults` in the locale-read path. Every `simctl spawn` inside smix now uses an absolute path; new spawn additions go through an internal helper that asserts on the leading `/`.

Please re-run your `bun test:e2e` bootstrap after upgrading — the ENOENT should be gone.

### The systemic response — subprocess argv in every error

Even after v1.0.7 the fix above, YOU would have been able to diagnose it yourself if the error message had told you which argv failed. So we changed the error itself.

`SimctlError::NonZeroExit` now carries the full argv + wall time. The Display impl surfaces every argument. Before:

```
error: sdk: FAIL [DRIVER_ERROR]: xcrun simctl spawn exited 2: An error was
encountered processing the command (domain=NSPOSIXErrorDomain, code=2):
The operation couldn't be completed. No such file or directory
```

After (v1.0.7):

```
error: sdk: FAIL [DRIVER_ERROR]: xcrun simctl spawn <UDID> /bin/rm -rf
/Users/USER/Library/Developer/CoreSimulator/Devices/<UDID>/data/Containers/Data/Application/<data-uuid>/Documents
/Users/.../Library /Users/.../tmp exited 2 (312ms): The operation
couldn't be completed. No such file or directory
```

Now you can grep the log and know exactly what smix asked simctl to do. If it fails again on a different path, you file a precise repro with the argv.

### The systemic response — subprocess ring buffer + diagnostic verb

Every `xcrun simctl` invocation records to a global ring buffer (capped at 128). New CLI verb:

```bash
$ smix diagnostic dump
=== runner runtime snapshot ===
uptime:         81412ms
sim health:     healthy
supervisor pid: 91234

=== open sessions (2) ===
  9F3E8B1A-...                           com.focusai.app.mobile                   openedAtMs=1720689042000 lastActivatedAtMs=1720689042000
  ...

=== runner-side subprocesses (last 0 of 0) ===

=== client-side subprocesses (last 20 of 47) ===
            312ms  code=  2  simctl spawn <UDID> /bin/rm -rf ...  err="No such file or directory"
            410ms  code=  0  simctl get_app_container <UDID> com.focusai.app.mobile data
            201ms  code=  0  simctl terminate <UDID> com.focusai.app.mobile
            ...
```

- `smix diagnostic dump` prints the runner's snapshot AND the client-side simctl ring.
- `smix diagnostic dump --json` for machine consumption (e.g. your qa/sim/runner.ts can capture on flow failure and attach to gate report).
- Legacy runners (v1.0.6 and earlier) return 404 on `/diagnostic/dump`; the CLI degrades gracefully to client-side ring only.

Wire it into your bootstrap driver's failure path:

```typescript
try {
  await runBootstrap()
} catch (e) {
  const dump = spawnSync(SMIX_BIN, ['diagnostic', 'dump', '--json'], {
    encoding: 'utf8',
    timeout: 5_000,
  })
  logger.error('smix diagnostic dump on bootstrap failure', {
    err: e.message,
    dump: JSON.parse(dump.stdout),
  })
  throw e
}
```

### Streaming discipline (fixes item D.3)

`smix runner supervise` now flushes stdout after every `RunnerCycled` JSON event. Your parser sees the event immediately even when the outer flow crashes fast right after a cycle.

### Cold-rebuild banner (fixes item A observability)

`smix runner up` now detects warm vs cold rebuild by inspecting `.smix/runner/derived-data-<UDID>/`:

```
runner starting: udid=... port=22087 pid=52341 — COLD REBUILD expected up to
10 minutes (first run after upgrade compiles the XCUITest bundle for smix
1.0.7). Log: .../runner-<UDID>.log. Timeout 300s.
runner up: xcodebuild still working (30s elapsed)
runner up: xcodebuild still working (60s elapsed)
...
runner up: http://localhost:22087/health = 200
```

Every 30 s during a cold rebuild you get a heartbeat. Consumers watching the process's stdout see progress instead of a silent stall. Your 600 s timeout is still recommended — the banner just tells everyone what to expect.

## How to upgrade

Same as prior versions:

```bash
cargo install smix --force                       # CLI + Rust SDK
bun add @goliapkg/smix@1.0.7                      # TS SDK
implementation("jp.golia.smix:smix-sdk:1.0.7")    # Kotlin SDK
.package(url: "...", .exact("1.0.7"))             # Swift SDK
```

**crates.io publish state**: partial as of this doc — v1.0.4/5/6 same-day publishes hit a rate limit. Remaining crates finish publishing after the reset (~40 min from this note). `smix 1.0.7` is fully available from npm + Maven Central + Swift Package immediately; `cargo install smix` may pull 1.0.6 for a brief window before falling back to 1.0.7. If you see 1.0.6 in `smix --version`, wait 30 min and retry — no other action needed.

## Retest checklist

1. **Item B — `simctl spawn exited 2`**. Upgrade to v1.0.7, rerun `bun test:e2e` bootstrap. Expected: no ENOENT on `launchApp: clearState: true`. If it fires on a different path, the error message now includes the argv — capture and share.
2. **Item A — cold rebuild banner**. Force a cold recompile (`rm -rf .smix/runner/derived-data-*`), then `smix runner up`. Expected: `COLD REBUILD expected up to 10 minutes` banner + heartbeats every 30 s.
3. **Item D.3 — supervisor flush**. Run supervise sidecar; kill the outer flow immediately after a `TEST INTERRUPTED`. Expected: `{"event":"RunnerCycled",...}` reaches your parser even on fast-crash.
4. **New — `smix diagnostic dump`**. Run after any failed flow. Expected: table of recent simctl calls + open sessions + supervisor pid. Wire it into your gate driver's failure path.

## What's next (v1.0.8+)

- **App-alive cache adaptive re-probe** — currently v1.0.5 §D2 marks the target dead for a hard 20 s window. Your item B secondary observation (`pinning-failure.yaml` fails with `unknown` a11y tree) points at this window blocking a slow-bootstrap app. v1.0.8 will change the cache to re-probe every 3 s during the window and invalidate on the first non-empty `/tree`. Deferred because the fix needs a background task loop on the runner side and I want the observability layer live first so we can measure the impact.
- **Supervisor `RunnerCycled` reason detail** — currently just the matched log line. v1.0.8 will include the surrounding 10 lines of runner-log context so cycle-cascade analysis stops requiring you to grep the log yourself.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.7-shipping.md
```

Prior insight-facing docs (read in reverse chronological order for context):
- `insight-v1.0.6-shipping.md` — sidecar supervise + down cascade
- `insight-v1.0.5-shipping.md` — session persistence + supervisor + idle-close
- `insight-v1.0.4-shipping.md` — feedback closure of 9 items §A-§I
- `insight-v1.0.3-studio-crash-2026-07-10.md` — SimRenderServer forensic
