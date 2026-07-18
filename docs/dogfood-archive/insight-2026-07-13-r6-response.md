# smix response — round-6 (pending link is in-memory) + v1.0.27 field results

Date: 2026-07-13
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-13-r6-pending-link-is-in-memory.md`
No release accompanies this response — it's an analysis + docs update; no code change is warranted (reasoning below).

## TL;DR (3 lines per Q10)

- **Correction accepted, and it closes the question definitively**: the registry being in-memory means NO runner-level verb can address this replay class — not `clearUserDefaults`, and not the "drain the queued URL" shape either. The registry is private in-process state; XCUITest and simctl cannot reach another process's memory. The honest toolkit answer is a decision table, now in the public cookbook.
- **v1.0.27 empirical gate: PASS.** b34 12/12 + log gate 7/7 + zero retries with D2's live on-screen confirm active across all your wait/scroll/tap shapes — that was the last open validation item from round 5. D2 is done.
- **`clearUserDefaults` keeps its place** — as you said, for persisted-flag poisoning (any NSUserDefaults key that leaks state across flows). No change needed.

## Why "drain the queued URL" can't exist (so we don't leave it half-open)

Your suggested shape was runner-side: "after a launch verb, optionally drain/ignore the first `UIApplicationLaunchOptionsURLKey` delivery." Walking the mechanism you documented:

1. The URL sits in `EXDevLauncherPendingDeepLinkRegistry` — a plain `var` inside the app process.
2. Delivery happens when dev-launcher itself calls `getLaunchOptions` at the next React host start — entirely inside the app.
3. Nothing in the XCUITest boundary (element queries, event synthesis, app lifecycle calls) or the simctl boundary (spawn/openurl/container access) can read or mutate that variable, and the delivery isn't an observable OS event smix could intercept.

The only externally-reachable lever is **process death**: `stopApp` (cooperative terminate) destroys the registry with the process. So the class IS closable today with smix primitives — `stopApp → launchApp` instead of a JS-level reload — at the dev-launcher ceremony cost your flows deliberately avoid. That tradeoff belongs to you, and your flow-side handling (b32/b33/b34 green) is a legitimate resolution.

## What we did instead

The mechanism + decision table is now in the public cookbook (`docs/ai-guide/08-cookbook.md` § "Expo dev-client: deep-link replay after JS reloads") — your source-reading, generalized so the next expo-dev-client consumer doesn't spend five batches re-deriving it:

1. Process-level relaunch (`stopApp → launchApp`) — registry dies with the process; costs the ceremony.
2. App-side replay gate (nonce-tagged links, drop second delivery) — your `0c04566f` shape; most durable for a QA-instrumented app.
3. Overlay-tolerant assertions — your current stable green; zero code, flow-author vigilance.

Plus the explicit "what does NOT work" list (clearUserDefaults / runner-side drain / neutralizer URLs) with the reasons, so nobody re-asks.

## v1.0.27 field results — acknowledged

- **D2 (live on-screen confirm): b34 zero-regression across every selector shape you run** — including the OCR-only below-fold chains and overlay-tolerant text tiers, which were the two shapes the frame∩viewport-not-isHittable design decision protected. That closes round-5 Ask 13 empirically. Nothing further pending on it.
- **D3 (supervisor health trigger)**: noted, no action needed on your side.
- **bacc5 / ~08:25 interference**: thank you for having already classified and discarded it. The process-check-before-runner-ops rule is now hard-encoded on this side; it will not recur.

## Where this leaves the arc

Every smix-side item from rounds 1–6 is closed or empirically validated:

| Item | State |
|---|---|
| OCR-in-verb (extendedWaitUntil / tapOn / scroll / runFlow gates) | validated b19–b34 |
| Timeout auto-capture / elementTypeRaw / snapshot headers | validated, in daily triage use |
| runFlow when.notVisible idempotent ceremonies | batch backbone since b19 |
| Auto-OCR env + regex-OR split | in use |
| iOS 26.5 alert-button promotion | validated |
| lastInteractiveNamedIds + probe ignore tuning | in use, defaults de-consumered in v1.0.26 |
| clearUserDefaults | shipped; repurposed for persisted-flag poisoning |
| Live on-screen confirm (Ask 13) | **validated b34** |
| Supervisor health trigger (b24 class) | shipped |
| Deep-link replay (Ask 12) | closed as "outside any runner boundary"; cookbook decision table published |

No open smix asks. File the next round whenever something new surfaces.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-2026-07-13-r6-response.md
```
