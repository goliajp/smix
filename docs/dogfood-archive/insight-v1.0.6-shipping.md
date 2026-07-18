# smix v1.0.6 — shipping notes for insight (upgrade path + delta)

Date: 2026-07-11 (same day as v1.0.5, ~30 min later)
From: smix maintainer (`claude@golia.jp`)
Prior: `docs/ai-guide/insight-v1.0.5-shipping.md` — read that first if you haven't upgraded to v1.0.5 yet.

## TL;DR

- Small CLI patch. Only reason for a new version: `smix runner up --supervise` folds the sidecar spawn + `runner down` teardown into the runner lifecycle. You can drop the separate `spawn(smix runner supervise)` from your `bun test:e2e` bootstrap driver.
- Rust workspace bumped to rust-version = 1.97 as the source-build baseline. Zero impact if you `cargo install` (prebuilt binary).
- Upgrade the same way as v1.0.5: `cargo install smix --force`, `bun add @goliapkg/smix@1.0.6`, `implementation("jp.golia.smix:smix-sdk:1.0.6")`, Swift Package `.exact("1.0.6")`.

## What changed vs v1.0.5

Two additions, no removals:

### `smix runner up --supervise` — sidecar in one command

Before (v1.0.5 pattern from my earlier doc):

```typescript
// separate sidecar spawn
const supervisor = spawn(SMIX_BIN, ['runner', 'supervise'], { ... })
try {
  await runBootstrapFlows()
} finally {
  supervisor.kill('SIGTERM')
}
```

After (v1.0.6):

```typescript
// sidecar folded into up
spawnSync(SMIX_BIN, ['runner', 'up', device, '--bundle', APP_ID, '--supervise'])
try {
  await runBootstrapFlows()
} finally {
  spawnSync(SMIX_BIN, ['runner', 'down'])   // tears down xcodebuild AND the supervisor
}
```

Behavior:
- `runner up --supervise` spawns the supervisor as a detached process after `/health` returns 200. Sidecar stdout/stderr → `.smix/runner/supervise-<UDID>.log`.
- Supervisor pid recorded in `.smix/runner/state.json` under a new `supervisorPid` field. Consumers who parse `state.json` should treat it as optional.
- `runner down` reads the pid, sends SIGTERM (5 s), escalates to SIGKILL if unresponsive. Re-entrant safe: `runner cycle` (called by the supervisor itself on `TEST INTERRUPTED`) skips the self-kill.
- `runner cycle` preserves the flag — if you `up --supervise`, then trigger a cycle (manually or via supervisor), the post-cycle `up` re-attaches supervision automatically.

### rust-version = 1.97 baseline

Workspace `Cargo.toml` bumped from `1.95` → `1.97`. Only matters if you build smix from source. `cargo install smix` gets a prebuilt binary; `bun add @goliapkg/smix` gets prebuilt JS; Swift Package + Maven Central are unaffected.

## What insight can now delete

If your `bun test:e2e` bootstrap driver has any of these v1.0.5-era shapes, delete them:

- **Manual `spawn(smix, ['runner', 'supervise'])`** — folded into `runner up --supervise`.
- **Manual `supervisor.kill('SIGTERM')` in `finally`** — `runner down` cascades.
- **Any code that tries to detect the supervisor's presence via `pgrep`** — trust `state.json`'s `supervisorPid` if you need to know.

Nothing v1.0.5-era breaks. The standalone `smix runner supervise` verb still exists — v1.0.6 doesn't deprecate it. Sidecar mode is a convenience, not a replacement.

## Verify after upgrade

```bash
smix --version                                        # should print `smix 1.0.6`
smix runner up sim-insight --bundle com.focusai.app.mobile --supervise
cat .smix/runner/state.json | jq '.supervisorPid'    # should print an integer
ps -p "$(cat .smix/runner/state.json | jq .supervisorPid)" -o command= | grep supervise
smix runner down                                      # should print `stopping supervisor: pid=…` first
```

If any of those steps don't produce the expected output, the state file is stale or the sidecar spawn was suppressed — grep `.smix/runner/supervise-<UDID>.log` for the reason.

## Nothing else changed

- No wire additions.
- No SDK ABI changes.
- No `Session`, `HttpRunner`, or CLI behaviour changes beyond the `--supervise` flag + `down` cascade.
- No new runbook needed. If you're on v1.0.5 already, the v1.0.5 shipping doc's runbook is still current; just add `--supervise` to your `smix runner up` invocation.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.6-shipping.md
```

Prior insight-facing docs (read `insight-v1.0.5-shipping.md` first if you haven't):
- `insight-v1.0.5-shipping.md` — session persistence + supervisor + idle-close, with the runbook
- `insight-v1.0.4-shipping.md` — closure of the 9-item gate-hardening feedback
- `insight-v1.0.3-studio-crash-2026-07-10.md` — SimRenderServer forensic
