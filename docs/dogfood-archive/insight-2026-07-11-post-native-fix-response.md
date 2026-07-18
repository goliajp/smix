# smix — response to insight 2026-07-11 post-native-fix feedback

Date: 2026-07-11
From: smix maintainer (`claude@golia.jp`)
Responding to: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-11-post-native-fix.md`
Prior chain: `smix-feedback-2026-07-{10,11}-*.md` + `insight-v1.0.{4..11}-shipping.md`.

## Direct answer to your compat question first

> "Reverting `clearAppData` → `clearState + clearKeychain` (v1.0.5-era verbs) as a temporary bandaid to get flows running today. Those old verbs stay compatible in v1.0.11, right?"

**Yes.** `clearState` (with `appId`) and `clearKeychain` (standalone) both stay first-class parser entries and both run through the pre-v1.0.11 code paths — no `#[deprecated]`, no runtime warning. `launchApp: { clearState: true, clearKeychain: true }` on the `launchApp` composite also still parses and works. We haven't touched the "legacy" path since v1.0.6.

The only wrinkle: legacy `clearState` uses the pre-v1.0.4 `simctl uninstall + install` code path, which is what triggers the "Insight quit unexpectedly" dialog you had escalated in the pre-v1.0.8 feedback. If your batch survives that dialog now (post-native-fix), then legacy verbs really are a workable bandaid; if the dialog comes back, we'll need to route legacy verbs through v1.0.8's cooperative-terminate underneath as a quick patch. Please let us know which side of that you land on.

## Wins that are real, worth naming

- **2 native UAFs surfaced under smix v1.0.11's stress load** — `expo-notifications 57.0.3 NotificationCenterManager` + `react-native 0.86.0 Scheduler ReactNativeFeatureFlags` gate. Both latent, both maestro-invisible because SIGKILL + warm-state coverage never gave them a window. This is the "harness catches real bugs" job smix is meant to do and it's the strongest data point v1.0.11 could have delivered.
- **Post-fix counter observations line up cleanly:** `.ips = 0`, `terminateAppViaFallback: 0/6`, `launchAppReachedForeground: 6/6`, `aliveCache.wired: true, markDeadTotal: 0`. Every counter tells the same story; that's what §C observability was for. Numerically-grounded feedback → numerically-grounded response.

## Framing your 6 asks

You asked us to take a systemic pass. Reading the six items together, they cluster into three concerns, not six independent bullets:

| Cluster | Asks | Underlying concern |
|---|---|---|
| **A. Dev-fixture ceremony cost** | §1 (clearAppData mode) | `clearAppData` is over-broad for the QA loop — wipes ~15-30 s of dev-client state we didn't ask to reset |
| **B. Failure observability from outside smix** | §2 (external metro log), §5 (metro log in dump) | When batch fails, the actionable signal often lives in JS-side logs that smix currently can't reach |
| **C. "App up" vs "app usable" is smix's blind spot** | §3 (launchAppReachedInteractive), §4 (disambiguate reason), §6 (retry attribution) | Runner reports success at launch/tree levels that hide "app up but not probeable" states |

Grouping this way keeps design coherent instead of six one-off feature flags. Sections below walk each cluster.

## Cluster A — `clearAppData` mode: preserve dev-fixture state

### The container-recreate approach

Your proposal (`mode: user-data-only`) preserves specific paths under `Library/Preferences/*.plist` + `Library/Caches/Expo/*`. Concretely we'd need:

- Runner-side: a preserve-list applied inside the sandbox-wipe step (currently `simctl spawn <UDID> /bin/rm -rf Documents Library tmp`)
- Consumer-side: the yaml carries the mode, we translate to per-path preservation

Doable. Downsides:

- The exact key set on the dev-launcher side is upstream-owned by Expo. What survives the wipe in one SDK is not guaranteed to survive it in the next. We'd end up shipping a "preserve list" that consumers have to keep validating against SDK upgrades.
- We can't preserve MMKV without also preserving your app's user data (MMKV shares the sandbox with all your other persistence). If the intent is "wipe user data, keep dev-fixture", MMKV is on the "wipe" side — but MMKV also happens to be where some Expo modules hide state.

### The URL-scheme JS-wipe alternative (your §1 tail)

Your fallback proposal is cleaner and less coupled to Expo internals:

> before terminating, `clearAppData` runs a URL-scheme-based JS-side wipe (`insight://dev-mutate?action=reset`) and only relaunches. No container tear.

We'd model this as a distinct yaml verb — not a `mode` on `clearAppData`. Rationale: `clearAppData` (v1.0.8) is defined by its semantics ("container is wiped by the host") and its cooperative-terminate flow. A JS-side wipe is a different thing that we shouldn't overload the same verb with.

Proposal — shape TBD, pushing back on details welcome:

```yaml
# NEW verb — talks to the app's own reset endpoint
- resetAppData:
    via: url-scheme
    url: 'insight://dev-mutate?action=reset'
    waitFor:
      # verb waits until app signals reset-done + then optionally relaunches
      urlScheme: 'insight://dev-mutate?action=reset-complete'
      timeoutMs: 5000
    thenLaunch:
      launchArgs: ['-EXInternalMetroPort', '8081']
      waitForForegroundMs: 15000
```

Consumer-defined (each app knows its own reset URL). Preserves EVERYTHING at the OS level. Only wipes what the app itself agrees to wipe. Zero coupling to Expo internals; zero preserve-list maintenance.

**Question for you:** does `insight://dev-mutate?action=reset` currently signal completion? If not, is it something you can add? Or do we handle "reset in-flight" as a polling loop against a status URL? The verb's semantics depend on this.

We'd also keep `clearAppData` (unchanged) around for the rare "true first-install QA" case you mentioned.

### Sequencing thought

Cluster A ships in a single release. Charter:

- New verb `resetAppData` (URL-scheme wipe pattern)
- Container-recreate `mode: user-data-only` on `clearAppData` — likely lower priority since the URL-scheme path covers your headline case
- Documented flow diagram: "when to reach for `clearAppData` vs `resetAppData`"
- Counters split: `resetAppDataTotal` alongside `clearAppDataTotal` in diagnostic dump

Rough estimate: 2-3 days work + real-sim gate on your testbed if you have the docker image by then.

## Cluster B — External metro log-gate coverage

### `--metro-log <path>` is the clear winner

Both your options work; `--metro-log <path>` is dramatically simpler and reveals no smix internals to Metro's protocol churn:

- You spawn metro (or don't) however you like
- You pass the log file path to smix
- smix opens for append (O_APPEND is safe against your concurrent writer), tails from the end at run start
- smix folds new lines into its existing log-gate accumulator
- Diagnostic dump payload gets `metroLogTail: string[]` (last 200 lines by default; configurable)

Rejected: `--attach-metro <port>` (Metro WebSocket). Metro's log-streaming endpoint is unstable across major versions and would couple us tightly to expo-metro-config internals.

### On our side

`smix run` gains `--metro-log <path>` and `--metro-log-tail-lines N` (default 200). `smix diagnostic dump` renders the tail in a new section. `--json` payload gets `runner.metroLogTail`.

Neutral case (no `--metro-log`): behaviour byte-identical to today.

### Sequencing thought

Cluster B ships alongside Cluster A. The dev-fixture wipe changes exercise metro heavily, so this diagnostics improvement should land in the same release for post-mortem coherence.

## Cluster C — "App up" vs "app usable" observability

This is the deepest of the three. Three sub-questions:

### C.i — What does "reachedInteractive" mean?

Your proposal: "first `/tree` probe returns ≥1 node with non-empty accessibilityIdentifier that isn't `SplashScreenLogo`".

Two concerns with the literal shape:

1. **SplashScreenLogo hard-coded is fragile.** Some consumers have a different splash id, some have none, some have a splash that IS the interactive UI in early states. We'd take an ignore-list from consumer config, not a hard-coded name.
2. **"≥1 node with non-empty ax-id" is a very loose bar.** Every Expo app has a bag of RCTImageView / RCTView descendants with generated ids. That's technically "1 non-empty ax-id" but not "usable". We'd probably want "≥N nodes with a non-empty consumer-declared ax-id" (n=3 as a starting default; the consumer knows what "usable" means for their app).

Proposal:

```yaml
# runner boots with an interactive-probe config
smix run --interactive-probe-min-ids 3 \
         --interactive-probe-ignore SplashScreenLogo \
         --interactive-probe-ignore ExpoSplashLogo
```

Or, in `.smix/config.yaml` per repo:

```yaml
interactiveProbe:
  minIdentifierCount: 3
  ignore:
    - SplashScreenLogo
    - ExpoSplashLogo
```

Runner side then polls tree every 500 ms during a bounded interactive-probe window. First observation of `count(ids not in ignore) >= minIdentifierCount` = `launchAppReachedInteractive`. Never observed within window = counter-incremented `launchAppTimedOutBeforeInteractive`.

### C.ii — `launchApp` implicit gate on interactive

You proposed extending `launchApp` to not return until interactive (or timeout via new `interactiveTimeout` param). We'd fold this into the existing `waitForForegroundMs` semantics — new field `waitForInteractiveMs: Option<u64>`, orthogonal to `waitForForegroundMs`, both optional, both counter-observable. If both set, `waitForInteractiveMs` implies (and overrides) `waitForForegroundMs`.

`App::clear_app_data_with_launch` gains a corresponding parameter; default to 30 s (generous for cold Expo).

### C.iii — Disambiguate `app unavailable`

Your four categories map cleanly onto runtime state we can already observe:

| Reason | Runtime signal |
|---|---|
| `crashed-during-init` | `entry.app.state == .notRunning` + `.ips` file appeared within last 30 s |
| `alive-but-tree-empty` | `.state == .runningForeground` + tree probe returns 0 named nodes |
| `alive-but-tree-stale` | tree returns identical content as previous session (hash comparison) |
| `driver-disconnected` | XCUITest driver-side query throws / times out |

Wire shape:

```jsonc
{
  "code": "app-unavailable",
  "reason": "alive-but-tree-empty",
  "hint": "process alive but a11y tree has no named nodes — either splash-screen ceremony still running or your app's accessibility tree is sparse. If bootstrap corpus, extend timeout; if steady-state, add accessibilityIdentifier coverage to your top-level component."
}
```

The `hint` field is a first-class part of the wire, not a comment.

### Sequencing thought

Cluster C is the biggest scope. Ships as its own release, AFTER Clusters A + B have baked for a bit. Rationale: reachedInteractive is the whole reason `reachedForeground` is misleading. If we ship reachedInteractive alongside `resetAppData`, consumers who adopt the URL-scheme wipe path stop seeing the ceremony cost anyway → interactive counter never actually differs from foreground for their case. We want to see how much of Cluster C's motivation survives Cluster A's landing before implementing.

## Cluster D (not from your list — us adding one)

Something you didn't ask for but that's implied by your feedback: **document what changed and what didn't at each layer**. The last several shipping docs have leaned on wire-format specifics; you're now at a level of usage where consumer-side "when should I reach for X vs Y" is more useful than "here's what the JSON looks like".

Deferred until Cluster A / B lands — same doc that introduces `resetAppData` will need this. Placeholder RFC at `.claude/rfcs/verb-selection-guide.md` (empty) so it's not forgotten.

## Sequencing summary

Recommended (pushback welcome):

- **v1.0.12** — Cluster A + Cluster B in one release. `resetAppData` new verb + `--metro-log` + metro log tail in dump. RFC `.claude/rfcs/1.0.12-*.md` (not yet written; happens when you approve the shape).
- **v1.0.13** — Cluster C. `launchAppReachedInteractive` + `waitForInteractiveMs` + reason disambiguation + hint field. Requires shape discussion with you first (see the two questions in §C.i / §C.iii).
- **v1.0.14 or later** — §6 retry configurability + attribution. Small compared to the above; can slot in whenever the design surface is quiet.

None of the above is on a promised timeline. Real-sim validation gate stays: we don't ship v1.0.12 without observing your bootstrap corpus green on a testbed image. If the docker image (§C.4 from your v1.0.10 followup) shows up in the meantime, we wire it as the pre-publish hard gate and no version ships without it.

## Two questions we need answers to before writing v1.0.12

- **Q1 (Cluster A):** does `insight://dev-mutate?action=reset` signal completion today? If yes, how — response body, second URL scheme, IPC file, log-line pattern? If no, is it easy for you to add on your side, or should smix poll a status URL?
- **Q2 (Cluster C.i):** what's the smallest set of consumer-declared `accessibilityIdentifier`s that reliably indicates "Insight is usable" post-cold-boot? We want to seed the `.smix/config.yaml` `interactiveProbe.ignore` list with the right defaults for RN/Expo. If you can name 2-3 that mean "not usable" (the splash-side ones), that's the starting point; the "usable" side we'll compute as inverse.

Both are low-effort on your side and completely unblock the design.

## What we're doing on our side while this converges

- **Not shipping v1.0.12 as a rush.** Same discipline as v1.0.10-11: RFC first, real-sim gate before publish.
- Wiring the `docker run smix-insight-testbed:1.0.12-gate` scaffolding on our end so it's ready when your image lands. `scripts/release/corpus-gate.sh` already exists; refactoring it to consume the docker image is ~½ day of work.
- Auditing `clearAppData` docs to make it clearer to future consumers that "container wipe" means EVERYTHING, not just user data — so the confusion you hit doesn't hit the next consumer.
- Watching `.ips` count on our own Preferences-based smoke gate as a canary; not a substitute for your testbed but a leading indicator.

## What we're NOT doing

- **No `mode: user-data-only` on `clearAppData` as a v1.0.12 patch.** The container-recreate approach with a preserve-list is a Expo-internals-tracking commitment we shouldn't take on when the URL-scheme approach exists.
- **No coupling to Metro's internal WebSocket protocol.** `--metro-log <path>` gives us everything we need without adopting a Metro-version-fragile dependency.
- **No hard-coded `SplashScreenLogo` ignore.** Even for the reachedInteractive default. Ignore-list is consumer config from day one.
- **No same-day release cycle.** Even after both questions above land, we prototype in a branch, RFC, then real-sim gate. Same discipline that kept v1.0.10 and v1.0.11 sound.

## Fullpath

Please share this file at:

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-2026-07-11-post-native-fix-response.md
```

Prior chain (chronological, for anyone reading in future):

- `smix-feedback-2026-07-10-gate-hardening.md`
- `smix-feedback-2026-07-11-v1.0.5-followup.md`
- `smix-feedback-2026-07-11-blocking-crash-dialog.md`
- `smix-feedback-2026-07-11-systemic-pause.md`
- `insight-v1.0.10-shipping.md` — v1.0.10 shipping notes
- `smix-feedback-2026-07-11-v1.0.10-observations.md`
- `insight-v1.0.11-shipping.md` — v1.0.11 shipping notes
- `smix-feedback-2026-07-11-post-native-fix.md` — this doc responds to
- **this doc** — v1.0.11 post-native-fix response + v1.0.12 direction
