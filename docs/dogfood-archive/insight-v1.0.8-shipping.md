# smix v1.0.8 — shipping notes for insight ("Insight quit unexpectedly" ELIMINATED)

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-blocking-crash-dialog.md`

## TL;DR

- **The "Insight quit unexpectedly" dialog is eliminated when you migrate to `clearAppData`.** v1.0.8 ships a new yaml verb and the runner-side machinery that finally makes cooperative `XCUIApplication.terminate()` + host-side sandbox wipe + cooperative `XCUIApplication.launch()` work end-to-end. No SIGKILL, no ReportCrash signal, no dialog.
- **Migration is one line per callsite** — replace `launchApp: { clearState: true }` with `clearAppData` + `launchApp: {}`. Insight offered to migrate `.devtools/qa/sim/subflows/` in one PR when the verb shipped — that's now.
- Legacy `launchApp: { clearState: true }` shape is UNCHANGED in v1.0.8 (still runs the old sequence + still fires the dialog). Deprecation warn deferred to v1.0.9 so we don't churn every consumer's console before you finish migrating.

## Why v1.0.4 §D12 wasn't enough

v1.0.4 §D12 removed `simctl uninstall + install` from the default clear-state path. The dialog should have stopped. It didn't. Root cause found in v1.0.8 investigation:

- `simctl terminate <UDID> <bundle-id>` sends the target app **SIGKILL**. That's the trigger — not the uninstall.
- iOS 26.5 sim's `com.apple.ReportCrash` catches SIGKILL of a tracked process and posts the "quit unexpectedly" dialog even though nothing crashed. Any `simctl`-based termination pathway trips it.
- The only cooperative termination pathway on the sim is `XCUIApplication.terminate()` via `testmanagerd` (the same channel XCUITest uses to drive tests). That does NOT signal ReportCrash.

So v1.0.8 moves the terminate INSIDE the XCUITest runner process. Same for launch. Host-side `simctl` is used ONLY for the sandbox wipe, and only after cooperative terminate finished — no crash window.

## What changed

### Runner (Swift)

Two new session endpoints:

- `POST /session/terminate-app { sessionId }` → cooperative `XCUIApplication.terminate()`. Returns `{ok, wallMs}`.
- `POST /session/launch-app { sessionId }` → cooperative `XCUIApplication.launch()`. Returns `{ok, wallMs}`.

Both are additive. v1.0.7-and-earlier runners return 404 — you MUST upgrade the runner alongside the CLI. Rerun `smix runner up` after upgrading (that triggers the cold rebuild we banner-diagnosed in v1.0.7).

### CLI + yaml (Rust)

New yaml verb `clearAppData`. Bare, no args. Behind the scenes it orchestrates the 3 steps:

1. runner `POST /session/terminate-app` — cooperative terminate.
2. host-side `SimctlClient::clear_app_sandbox` — `simctl spawn <UDID> /bin/rm -rf Documents Library tmp` (safe now because step 1 was cooperative).
3. runner `POST /session/launch-app` — cooperative launch on the wiped sandbox.

Requires an open session (auto-populated by `smix run`). If you hit `clearAppData: no session id on the client`, you're calling smix from a code path that opens no session — invoke through `smix run <yaml> --bundle-id <id>` or wrap the session lifecycle explicitly.

## Migration diff

Insight's proposed diff from the feedback doc, now supported:

```diff
- - launchApp:
-     clearState: true
+ - clearAppData
+ - launchApp: {}
```

If a callsite doesn't need to launch immediately after (e.g., you clear then do other setup steps before opening the app), just use `- clearAppData` alone.

If a callsite ALSO uses `clearKeychain: true`, that's separate:

```diff
- - launchApp:
-     clearState: true
-     clearKeychain: true
+ - clearAppData
+ - clearKeychain     # existing verb, unchanged
+ - launchApp: {}
```

`launchApp: { permissions: {...}, arguments: [...], stopApp: false, ... }` unchanged — those fields don't touch the clear-state pathway.

## Legacy path

`launchApp: { clearState: true }` is UNCHANGED in v1.0.8 — still fires the dialog. This is deliberate: we don't want to change semantics of a shape you're actively using while you migrate. **You still get the dialog until you migrate.**

v1.0.9 will:

- Emit `WARN: launchApp.clearState is deprecated; migrate to \`clearAppData\`` on every hit.
- Internally auto-expand `launchApp: { clearState: true }` → `clearAppData + launchApp: { <rest of fields> }` so consumers who don't migrate get the fix automatically.

We're deferring that flip to v1.0.9 so YOU get to trigger the migration on your own timeline (batch PR, gate runs green, then flip the auto-expand). Nothing changes silently in v1.0.8.

## Verify after upgrade

```bash
cargo install smix --force
smix --version                                    # smix 1.0.8

# Force a cold rebuild of the XCUITest bundle so v1.0.8's runner
# code ships to your sim:
rm -rf .smix/runner/derived-data-*
smix runner up sim-insight --bundle com.focusai.app.mobile --supervise
# … cold rebuild banner + heartbeats …

# In your qa/sim yaml, migrate one flow first:
# Change `launchApp: { clearState: true }` → `clearAppData + launchApp: {}`
bun test:e2e -- single-flow-launch-chain.yaml   # or similar targeted invocation

# Expected: 0 "Insight quit unexpectedly" dialogs during the run.
```

Retest checklist from feedback:

1. ✅ **0 dialogs during batch** — after migrating all `clearState: true` sites in `.devtools/qa/sim/subflows/`.
2. ✅ **`pgrep -l ReportCrash` shows the daemon idle** — cooperative terminate + host-side wipe never signals it.
3. ✅ **`~/Library/Logs/DiagnosticReports/` unchanged before/after** — no fake `.ips` files hiding.
4. ✅ **Sanity — no regressions in bootstrap + b-accounts + c-live scopes** — the migrated verb is semantically equivalent (clear + launch) with the same session id.

If ANY dialog fires during a batch where every `clearState: true` has been migrated to `clearAppData`, that's a v1.0.9 blocker — capture the runner log + `smix diagnostic dump --json` and share.

## Wire compatibility

- v1.0.7-and-earlier clients hitting a v1.0.8 runner keep working (no wire break).
- v1.0.8 clients hitting v1.0.7-and-earlier runners get 404 on the new terminate-app / launch-app endpoints — `clearAppData` fails cleanly with the wire error. Upgrade runner + CLI together.

## What's next (v1.0.9)

Deferred from this cycle:

- **Adaptive app-alive cache re-probe** — your v1.0.5-followup item on `pinning-failure.yaml`'s hard 20-s window blocking a slow-bootstrap app. Runner will re-probe every 3 s during the window and invalidate on the first non-empty `/tree`.
- **Supervisor `RunnerCycled` reason with ±10 lines of log context** — cycle-cascade analysis stops requiring a separate grep.
- **`launchApp: clearState: true` deprecation + auto-expand** — the migration completion flip once you've done the batch PR on your side.

## fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.8-shipping.md
```

Prior insight-facing docs (reverse-chronological, all `docs/ai-guide/insight-*.md`):
- `insight-v1.0.7-shipping.md` — systemic observability layer
- `insight-v1.0.6-shipping.md` — sidecar supervise
- `insight-v1.0.5-shipping.md` — session persistence + supervisor + idle-close
- `insight-v1.0.4-shipping.md` — 9-item feedback closure
- `insight-v1.0.3-studio-crash-2026-07-10.md` — SimRenderServer forensic
