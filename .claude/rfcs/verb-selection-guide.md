# Verb selection guide — clearAppData vs resetAppData vs clearState

Written: 2026-07-11 (v1.0.14 Cluster D)
Location: `.claude/rfcs/verb-selection-guide.md`
Insight ask (from their v1.0.12 answers, implicit): "the last several shipping docs have leaned on wire-format specifics; consumer-side 'when should I reach for X vs Y' is more useful".

Three verbs today wipe or reset app state. They're not interchangeable. Which one to reach for depends on **whether you want to preserve dev-fixture state**, and **whether the app itself can signal reset completion**.

## Decision tree

```
Q1: Are you running against a Release/Ad-Hoc build (no dev-launcher, no Metro connection)?
    ├── YES → use `clearAppData` (container-wipe). Nothing to preserve; simplest.
    └── NO (dev-client build) → Q2

Q2: Does your app expose a URL-scheme + reset primitive on its own side (e.g., `insight://dev-mutate?action=reset`)?
    ├── YES → use `resetAppData` (URL-scheme JS-wipe). Preserves dev-launcher metro URL,
    │          Metro bundle cache, dev-tools connection. Your app's `mmkv.clearAll()` runs
    │          in-process; dev-fixture state survives. THIS IS THE HIGH-VOLUME QA-LOOP CASE.
    └── NO → Q3

Q3: Do you have code that resets your app via native API + can add a URL scheme handler?
    ├── YES → add the URL scheme + a completion log line, then use `resetAppData`.
    │          That's a 10-line consumer-side change and permanently removes the 15-30 s
    │          dev-client ceremony cost from every reset. Reference impl:
    │            src/debug/dev-url-handler.ts (in insight's repo, roughly):
    │              case 'reset':
    │                await mmkv.clearAll();
    │                console.log('[insight-dev] reset-complete token=' + genUuid());
    │                return response.json({ ok: true });
    └── NO → Q4

Q4: Do you need a first-install QA case (activate a fresh tenant / verify onboarding path)?
    ├── YES → use `clearAppData`. Container wipe is intentional here — first-install IS the case.
    └── NO → use `clearState` (v1.0.5-era legacy). Only reach for this if the pre-v1.0.8
             `simctl uninstall + install` path is what you specifically need (rare — the
             "Insight quit unexpectedly" dialog risk is real on the legacy path).
```

## Comparison matrix

|                                     | `clearAppData` (v1.0.8) | `resetAppData` (v1.0.14)  | `clearState + clearKeychain` (v1.0.5 legacy) |
|-------------------------------------|-------------------------|---------------------------|-----------------------------------------------|
| **How it wipes**                    | host-side simctl `rm -rf Documents Library tmp` inside sandbox | fires URL scheme; app decides scope | `simctl uninstall <bundle> + install <path>` |
| **Wipes app's user data**           | ✅                        | ✅ (app-defined)          | ✅                                            |
| **Preserves dev-launcher metro URL**| ❌                        | ✅                        | ❌                                            |
| **Preserves Metro bundle cache**    | ❌                        | ✅                        | ❌                                            |
| **Preserves keychain**              | ❌ (paired w/ clearKeychain) | app decides             | ❌ (paired w/ clearKeychain)                  |
| **First-install semantics**         | ✅                        | ❌                        | ✅                                            |
| **Cooperative terminate/launch**    | ✅ (XCUIApplication)      | N/A (no terminate/launch) | ❌ (SIGKILL under simctl uninstall)          |
| **`.ips` / ReportCrash risk**       | Low (v1.0.11 wait-for-fg) | Effectively zero          | Was high pre-v1.0.11 native fixes; TBD now   |
| **Runner HTTP round-trips**         | 2 (`/session/terminate-app` + `/session/launch-app`) | 0 (all host-side)         | 0 (all host-side; runner sees an activation storm) |
| **Wall-clock cost**                 | ~1-2 s (terminate + wipe + launch + wait_for_foreground) | ~50-500 ms + your app's reset time | ~2-5 s (uninstall + install + first-boot) |
| **Requires session open**           | ✅                        | ❌                        | ❌                                            |
| **Requires `--metro-log <path>`**  | ❌                        | Only for `waitFor: { logLinePattern }` — otherwise fire-URL-and-return | ❌ |
| **When to reach for it**            | First-install QA smoke; container-schema migration debug | Day-to-day QA loop; every non-first-install iteration | Only when the pre-v1.0.8 path is specifically required (rare) |

## Rule of thumb

Insight's QA loop of "run bootstrap batch × 3 flows × 6 launches per run" wants `resetAppData` for **every launch except the first**. The first launch of a batch (or a fresh sim) can use `clearAppData` to establish a known-clean baseline; every subsequent launch pays a fraction of the wall-clock cost with `resetAppData`.

## Migration crib

If your yaml today looks like:

```yaml
# pre-v1.0.14, container-wipe on every launch
- clearAppData
- clearKeychain
- launchApp: {}
```

and every launch pays the 15-30 s dev-client ceremony cost, refactor to:

```yaml
# v1.0.14+ — first launch container-wipe (fresh baseline), rest URL-scheme JS-wipe
- clearAppData:                            # first flow only — establishes baseline
    launchArgs: ["-EXInternalMetroPort", "8081"]
- clearKeychain
- launchApp: { waitForForegroundMs: 15000 }

# … subsequent flows in same batch reach for the fast path:
- resetAppData:
    via: url-scheme
    url: 'insight://dev-mutate?action=reset'
    waitFor:
      logLinePattern: '\[insight-dev\] reset-complete token='
    timeoutMs: 5000
- launchApp: { waitForForegroundMs: 15000 }
```

Requires:
- `smix run --metro-log /tmp/metro.log` (for `waitFor.logLinePattern` — otherwise use `waitFor: { sleepMs: 500 }` as a soft fallback).
- App-side URL scheme + `[dev] reset-complete token=<uuid>` log line (10-line consumer-side change, see insight's Q1 answer in their v1.0.12 answers).

## Anti-patterns

- **Don't** reach for `clearState` in v1.0.14+ code. Only kept for compatibility with v1.0.5-era yaml — the pre-v1.0.8 `simctl uninstall + install` path is why the "Insight quit unexpectedly" dialog fired in insight's v1.0.7-v1.0.10 cycles. Use `clearAppData` if you need first-install semantics.
- **Don't** use `resetAppData` for first-install QA. The whole point is that dev-fixture state survives — that includes the app's own "first-run splash was already seen" flags. Use `clearAppData` for that.
- **Don't** use `waitFor: { logLinePattern }` without also passing `--metro-log <path>`. The verb best-effort falls back to a 500 ms sleep, which is silent. If you're relying on a completion signal, you need the log tail active.

## Wire-shape defaults reference

| yaml field                | wire field                          | default             |
|---------------------------|-------------------------------------|---------------------|
| `resetAppData.timeoutMs`  | `Step::ResetAppData.timeout_ms`     | `5000`              |
| `resetAppData.waitFor`    | `Step::ResetAppData.wait_for`       | `None` (no wait)    |
| `resetAppData.via`        | (ignored; today only URL-scheme)    | `url-scheme`        |
| `clearAppData.launchArgs` | `Step::ClearAppData.launch_args`    | `[]`                |
| `clearAppData.launchEnv`  | `Step::ClearAppData.launch_env`     | `{}`                |
| `launchApp.waitForForegroundMs` | `Step::LaunchApp.wait_for_foreground_ms` | `None` (dispatch-and-return) at yaml layer; `15000` at `App::clear_app_data` default |
