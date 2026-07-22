# Changelog

All notable changes to the `smix` workspace are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) at the wire, ABI, and CLI surface.

## [2.0.0] — Unreleased

The first major release: the v1 accretions are consolidated into one deliberate surface. Breaking; `smix migrate` rewrites v1 flow yaml, and the runner keeps answering wire schema 1 so v1.x clients still negotiate.

### Breaking

- **Sessions are mandatory** — the iOS driving surface always operates inside a runner session; the loose per-request app binding is gone.
- **Wire schema negotiation** — the runner reports the schema versions it speaks on `/health` (`[1, 2]`); clients negotiate the newest shared version instead of assuming one.
- **`SMIX_*` escape-hatch env vars removed** — behaviour that was env-toggled is now the default or a config field; unknown `SMIX_*` vars warn by name.
- **Selector model merged** — the `Modifier`/`Modifiers` split and the dual `open_url` forms collapse into the single selector model the resolver actually implements.
- **`smix-recorder-ir` renamed to `smix-authoring-ir`** — the crate name now matches what it holds.
- **smix-server needs no database and no valkey.** Stream sessions and the capturing set both live in an embedded store. `DATABASE_URL` and `REDIS_URL` are no longer read; setting either prints a line saying so, rather than being ignored, so nobody keeps a postgres or a valkey running for a server that stopped connecting to them. `SMIX_SERVER_STORE_ROOT` says where the store lives (default `.smix/server`).
- **Unknown keys in a selector mapping are refused.** They used to be read past in silence, which is how every spatial modifier could be dropped without a single flow failing. A flow carrying a key smix does not implement — `enabled:` was one, documented and never wired — now fails to parse, naming the key and listing the legal set.
- **YAML verb table frozen at v2** — verb renames land through `smix migrate`; identity rows that shadowed maestro aliases (`doubleTapOn`, `longPressOn`) are gone, so canonical maestro spellings survive the codemod.
- **The Android launch activity is resolved, not assumed** — every Android launch used to run `am start -n <pkg>/.MainActivity`, which is what an app generated from a template is called and what almost nothing else is; an AOSP app, or one whose entry point had been renamed, did not start and the response said nothing about why. The package manager is asked instead — `cmd package resolve-activity` on the host side, `getLaunchIntentForPackage` in the runner — and `.MainActivity` remains only as the answer when both come back empty, so no device that worked before is worse off. `apps.yaml`'s `activity:` key is honoured as an override; it had been parsed, defaulted and dropped since it was added. **Callers who implement `DeviceControl` themselves must change a signature**: `launch_with_args` takes an `activity: Option<&str>`, and `LaunchAppOptions` and `Flow` each carry a `launch_activity` field, so struct literals need the extra member. Flow yaml is unaffected.

### Added
- **Single-shot verbs can name a device** — `smix tap`, `find`, `wait-for`, `fill`, `press-key`, `scroll`, `hide-keyboard`, `tree`, `describe`, `system-popups`, `system-popup-action`, `run-script` and the four `smix authoring` actions take `--device`. They read `--port` and `SMIX_RUNNER_PORT` and stopped there, so the `.smix` registry's per-sim `runnerPort` was unreachable from them: in a workspace with a sim registered on 22088, `smix run` dialled 22088 and `smix tap` dialled 22087. `--device` here finds the port and nothing else — a runner is a process on a port, so the port already says which device the call reaches.
- **Relational operators in assertTrue expressions** — `<`, `<=`, `>` and `>=`. The expression grammar's tightest comparison was `==`, so the documented `assertTrue: ${output.userCount > 0}` failed to lex. Two numbers compare by value and two strings in order; comparing a string to a number is refused rather than converted, because an assertion that answers on a reading you did not intend is worse than one that stops. Note that `output.*` always holds strings — `extractWithAI` and `runFlow: {as: name}` are its only writers.
- **An explicit regex form for text selectors** — `text: { regex: "^Help$" }`. `Pattern` has serialized to `{regex, flags}` all along, but the yaml side read `text:` as a string, so a mapping fell through and only a `|` in a plain string ever produced a pattern. `^Help$` was matched as eight literal characters and found nothing. Metacharacter detection was deliberately not widened: `Delete?` and `3.5` are ordinary labels.

- `smix sim register <alias> --udid <UDID>` — creates and populates the `.smix/sims.json` device registry (previously there was no bootstrap for alias-form device refs).
- `smix system-popup-action <popup-id> <button-id>` — CLI verb for the runner's `/system-popup-action` route.
- Bare `- eraseText` parses with maestro's default of 50 characters.
- `POST /tap` returns the matched element's label, frame, app frame, and per-stage timings at the top level (`TapResult`); the previous nested emission deserialized to empty and the resolve mode's frame never actually arrived.
- FFI driving surface (`SmixDriver` / `SmixSession` / `CancelToken`) — the Swift and Kotlin SDKs drive through one Rust wire client instead of three per-language HTTP clients.
- `llms.txt` / `llms-full.txt`, generated from the verb table and guides, gated for freshness at ship.
- `inputText: { id, text }` types into a named field, not just the focused one — the SDK's targeted fill existed all along; only the yaml wiring was missing.
- `openLink: { link: <url> }` mapping form. maestro's `browser:` / `autoVerify:` options are refused loudly rather than accepted and ignored.
- `webViewEval` is accepted alongside `webviewEval` / `webview_eval`.
- Bare `- killApp` and bare `- clearState` act on the current app, resolved from the last launched bundle.
- `App::connect_to_runner_lazy` — build a client without a startup health probe, for hosts whose lifecycle starts before the runner's.
- `smix runner up --platform android` / `smix runner down --platform android` bring the Kotlin runner up and down: install the instrumentation APK, forward the port, start the server, wait for `/health`. Every adb call names its device with `-s`.

### Fixed
- **The tap routes resolve id and label** — `/tap`, `/double-tap` and `/long-press` decoded only `selector.text`, so `dispatch: daemonProxy` — the escape hatch for React Native views whose `Pressable` swallows the ordinary gesture path — could only ever address an element by its visible label, which is exactly what an RN `testID` is not. The actions guide has documented that pairing with an `id` since it was written and it had never worked. `text`'s meaning is byte-for-byte what it was; `id` and `label` are new and exact.
- **smix authoring suggest searches every readable string** — a bare-string search looked at `text`, `value` and `title` only, and a real iOS tree carries labels: a captured Settings tree has 33 non-empty labels and no text or title at all, so the example printed in the command's own help returned nothing on the platform smix exists for.
- **Underscored arrow key names** — `pressKey: ARROW_UP` and its three siblings. The guides write key names in SCREAMING_SNAKE, which worked for `VOLUME_UP` and not for the arrows, so a reader following the convention landed on "unknown key".

- **Spatial selector modifiers reach the resolver.** `near`, `below`, `above`, `leftOf`, `rightOf`, `inside`, `ancestor`, `nth`, `first` and `last` are implemented in `Modifiers`, honoured by the resolver and documented in the selector guide, and the yaml parser read none of them — every arm built `Modifiers::default()`, and `tapOn` honoured one, spelled `index:`. A dropped modifier cannot fail, it only widens the match, so a flow written to disambiguate silently resolved against everything. `nth:` (the documented spelling) and `index:` both work. Verified on a real simulator: an assertion anchored the true way passes and the same assertion anchored the wrong way fails, which is the only evidence that separates "the modifier arrives" from "the modifier is used".
- **CLI state moved out of hand-written JSON.** The device registry, runner handles, capsule records and the three diagnostic buffers were six JSON files written with unchecked `std::fs::write`; they are one embedded store under `.smix/kv/` now. An existing `.smix/sims.json` (or runner/capsule state) is imported on first use and **left on disk** — downgrading to a pre-2.1 smix still finds it. `smix diagnostic store` prints the whole store as JSON, so state stays as readable as `cat` made it.
- **Concurrent `smix sim register` no longer loses an alias.** Registering read the whole registry, inserted a row and wrote the whole file back; two processes doing that at once kept only the later write. Each record is written on its own now, behind an exclusive lock on the store.
- **The iOS and Android runners stop overwriting each other.** Both wrote `.smix/runner/state.json` through their own copy of the path helper, so bringing one up replaced the other's record and `smix runner down` tore down whichever had won. `down` with no arguments still means iOS and `--device` still means Android.
- **Damaged state is reported instead of read as absent.** Every load used `.ok()?` or `let Ok(x) = .. else { return }`, so a corrupt file was indistinguishable from no file — and the next write erased the evidence. Corruption now names the key it could not read.
- **Diagnostic writes that fail say so.** The subprocess ring, the resetAppData counters and the flow attempts discarded their write results. They still never fail the `xcrun simctl` call they accompany — that was always the right call — but a failure is now visible instead of silent.
- **A flow's own `appId:` opens its session.** `run_flow` opened the session and foregrounded before parsing the yaml, using only the CLI bundle — which defaulted to the placeholder `com.example.app`. The README's own quickstart form, `smix run flow.yaml --device X` with no `--bundle-id`, could not drive a real app at all.
- **Running a flow against an app that is not installed** reports `APP_NOT_RUNNING` naming the bundle and what to type next, instead of a raw `xcrun simctl get_app_container … NSPOSIXErrorDomain code=2`.
- **The MCP server survives a runner that is not up yet.** MCP clients launch their server at client startup, almost always before `smix runner up`; it died there, leaving a dead server for the whole session. It now connects on first use and reports the runner state then.
- **A runner that was never reached is reported unreachable, not dead.** `last_seen_ms = 0` produced "runner died mid-session", sending first-time users after a crash that never happened.
- **`back` waits for the navigation bar to change** instead of sleeping a fixed 500 ms and reporting success unconditionally; an end-to-end flow measured 1811 ms → 1433 ms.
- **The Android runner reads the whole request body.** Unread POST bytes prefixed the next request line on a keep-alive connection, producing `HTTP verb {}GET unhandled`.

## [1.0.27] — 2026-07-13

Two issues that flows had been working around at the yaml level move to the runner level where they belong: per-key user-defaults deletion (deep-link replay-state neutralization) and live on-screen visibility confirmation.

### `clearUserDefaults` verb: per-key NSUserDefaults deletion

expo-dev-launcher persists the most recent custom-scheme deep link and re-delivers it after EVERY JS bundle load — always AFTER any URL the flow sends post-relaunch, so flow-side neutralizer URLs and post-relaunch closes all lose the ordering race. App-side replay gates misclassified legit deep links and failed whole batches.

New verb — surgical, host-side, ordering-race-free (runs between terminate and relaunch, no app cooperation needed):

```yaml
- stopApp
- clearUserDefaults:
    keys:
      - "expo.devlauncher.pendingDeepLink"   # whatever key your investigation names
    bundleId: com.acme.app                   # optional; default = flow appId
- launchApp
```

- iOS: `simctl spawn <udid> defaults delete <bundle> <key>` — goes through the sim's cfprefsd so the deletion is coherent with the app's next launch (host-side plist editing would race the cache).
- Contract: "ensure keys absent" — already-absent keys (or a missing domain) are success, not errors.
- Android: explicit unsupported error (SharedPreferences has no host-side per-key path; `clearAppData` remains the whole-store option).
- The generic decision split per the three-layer model: smix owns the deletion capability; WHICH keys encode replay state is app knowledge that stays with the test author.

### Tree-tier visibility agrees with tapOn (live on-screen confirmation)

Under iOS 26.5 + RN 0.86 Fabric, XCUITest SNAPSHOTS report below-the-fold elements with drifted in-viewport frames and `visible=true`. The resolver's frame∩viewport filter passes them, so `extendedWaitUntil` false-greened, `scrollUntilVisible` returned without scrolling, and `tapOn` then honestly failed on the same selector — three verbs, two answers.

Fix — live on-screen confirmation at the driver layer:

- When a tree probe matches a node, the driver issues ONE live `/find` with the new `requireOnScreen: true` flag, using the matched node's `identifier` (else `label`) as the probe handle. The live XCUI query re-resolves current layout — live frames don't drift.
- Confirmed ⇒ hit (one extra live query per successful wait, ~ms). Refuted ⇒ treated as not-yet-visible: `wait_for` keeps polling, `scrollUntilVisible` keeps swiping (exactly the state another swipe fixes), `find`/`runFlow.when`/`wait_for_not_visible` return false.
- On `wait_for` timeout after refuted confirms, the failure hint states it explicitly: "the a11y tree matched this selector but the LIVE on-screen check refuted it every time … use scrollUntilVisible … or an ocrText tier."
- **Frame∩viewport, deliberately NOT `isHittable`** — hittability is false for elements under floating overlays (QA bubbles), which are genuinely visible and assertable; hittability-strict checks would false-negative overlay-tolerant assertions.
- Matched nodes with neither identifier nor label can't be live-confirmed → tree verdict stands (pre-v1.0.27 semantics; OCR tiers remain the play for handle-less degraded trees). Live-probe transport errors also let the tree verdict stand — a flaky probe must not turn a real hit into a miss.
- The live `/find` fast path (simple literal Text selectors) now also requires on-screen, so `find`-based paths agree without a second round-trip.
- Android untouched: its tree is live per-node-refreshed (`AccessibilityNodeInfo.refresh()`), so snapshot drift doesn't arise.

### Regex Text selectors no longer burn the live-route budget

Found while wiring the live-confirmation work: `can_use_find_route` admitted regex-pattern Text selectors, but `Pattern::Regex` serializes as an object the runner's `/find` decode rejects with 400 — a regex Text selector dispatched there would burn the full transport-retry budget (~8 s) and fail with a DriverError instead of evaluating. Regex patterns now host-resolve like other complex shapes.

### Supervisor health-unreachable auto-cycle

A dogfood batch observed a runner death with no `** TEST INTERRUPTED **` banner (warm derived-data reuse after a downgrade sync) — the log-marker supervisor sat through it. `smix runner supervise` now also probes `GET /health` every ~10 s; 3 consecutive failures (~30 s unreachable) triggers a cycle through the same cooldown + storm accounting as the log markers, emitting `{"event":"RunnerCycled","reasonMatched":"health-unreachable x3", …}`.

### Wire compatibility

- `POST /find` gains optional `requireOnScreen` (absent = pre-v1.0.27 exists-only). `FindHandler` typealias gains the param (runner + CLI ship together via the version-mismatch gate).
- `Step::ClearUserDefaults` new enum variant; `clearUserDefaults` new verb key — additive.
- `DeviceControl::user_defaults_delete` new trait method with an explicit-error default impl (Android inherits the honest unsupported error).
- CLI flags unchanged.

### Ship gate

- 81 parser (+4 clearUserDefaults) + 90 runtime_mock (+2 dispatch) tests green; 360 swift-bridge tests green (now a NON-BYPASSABLE ship.sh gate — a stale test asserting the pre-v1.0.11 aliveCache contract had sat failing unnoticed for 15+ releases because this suite was never in the gate; corrected).
- Full workspace + Swift build green.
- Real-sim smoke: wait/scroll/tap agreement + clearUserDefaults key deletion on Preferences.

## [1.0.26] — 2026-07-13

**Systematic polish sweep** — audit of the v1.0.14 → v1.0.25 rapid-patch arc for consumer-specific design leakage, iOS/Android parity drift, and documented-but-unimplemented yaml shapes. No single-consumer feedback drove this cycle; the charter was "sweep everything before moving on."

### De-consumer-ization (generic semantics replace hardcoded values)

- **Interactive-probe default ignore list** (Swift runner): the bundled default contained a specific consumer's bundle id. Replaced with the generic semantic — the target app's OWN bundle id is now always merged into the ignore set dynamically at probe time (the application root node carries `identifier == bundleId` on every app; counting it toward `minIdentifierCount` was a semantic bug, not a per-consumer config concern). Default ignore is now `["SplashScreenLogo"]` (generic Expo splash artifact) only. Consumers with explicit `.smix/config.yaml interactiveProbe.ignore` are unaffected; consumers relying on defaults get strictly more correct behavior.
- **`.ips` snapshot heuristic** (`smix run` crash attribution): the no-bundle-id fallback matched a hardcoded consumer app name. Now matches ALL `.ips` files when the bundle is unknown — the before/after diff around the flow run already time-bounds relevance; name-filtering was both consumer-specific and lossy.

### iOS/Android parity

- **Android runner `/tree` snapshot-freshness headers** — `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` now emitted by the Kotlin runner (v1.0.23 shipped them iOS-only).
- **Android runner version string** — `SmixRunner.VERSION` had frozen at an old build id (`v6.0-c3b`) for multiple releases while the workspace advanced; `/health` lied about the running version. Now tracks the workspace version, and `scripts/release/ship.sh` gates on BOTH the Kotlin `VERSION` and the gradle `mavenCentralVersion` matching the ship version — the drift class is mechanically closed.
- **`elementTypeRaw` documented as iOS-only** (`wire-format.md`): Android has no `XCUIElement.ElementType` numeric; payloads omit the field and deserialize to the default `1`. Android's equivalent triage signal is the full a11y class name in `rawType` plus the runner-emitted `role`.

### Documented-but-unimplemented yaml shapes (docs promised, parser rejected)

- **`tapOn: { dispatch: xcui | daemonProxy }`** — docs described a `mode: pathA|pathB` tap override that never parsed. Implemented as `dispatch:` with the real mechanism names: `xcui` = XCUIElement-anchored dispatch (required for SwiftUI `.sheet`/`.alert`/`.confirmationDialog`/`.fullScreenCover` dismiss bindings; requires `id:`; cross-platform via the runner's id-anchored tap), `daemonProxy` = XCTRunnerDaemonSession synthesize (stubborn RN Pressables; iOS-only). This is the GENERIC surface for what the `v2-*` fixture-namespace auto-routing heuristics did for smix's own selftest — those heuristics remain (scoped to smix's own id namespace, zero third-party blast radius) but are frozen; new yaml uses `dispatch:`.
- **`waitForAnimationToEnd: { timeout: N }`** — maestro-canonical map form now parses (docs showed it; parser only accepted bare/integer forms).
- **`anchorRelative:`** — accepted as alias of `anchored:` in `tapOn` and fallback-chain elements (3 docs pages promised the alias since v5.20).
- **`runFlowConditional:`** — was documented as a verb name in 2 places but never parsed (it's the internal enum variant name). Docs corrected to the canonical `runFlow: { when: …, file|commands: … }` shape.

### Docs alignment sweep

- `02-yaml-reference.md` — `waitForAnimationToEnd` described as "blocks until UI quiescent" (wrong since ever; it's a fixed sleep). Rewritten. Added `runFlow.when.notVisible`, OCR-in-verbs section, `SMIX_AUTO_OCR_FALLBACK` + `SMIX_TAP_OCR_POLL_MS` env docs.
- `04-actions.md` — OCR fallback tap + poll semantics, OCR-aware scrollUntilVisible, real `dispatch:` surface replacing the fictional pathA/pathB.
- `05-cli.md` — `--dry-run`/`--check`, `--retry`, `--debug-output` flags + env var table.
- `06-fixtures.md` — the `- fixture:` verb host-app contract (the `qa-bubble-toggle` toggle id, chip testIDs, completion-signal registry shape) was only documented in non-public notes; now in the public guide with the registry JSON verified against `smix-fixture`'s actual wire shape.
- `verb-parity.md` — version-at-freeze refreshed, `extendedWaitUntil` poll cadence corrected (250 ms, not 100 ms), OCR-in-fallback + snapshot-header rows updated.
- `wire-format.md` — `elementTypeRaw`, `role`, snapshot-freshness headers documented.
- `08-cookbook.md` — conditional-flow recipe corrected to the real verb shape + `notVisible` idempotency example.

### Test-infra fix

- v1.0.23's `SMIX_AUTO_OCR_FALLBACK` parser tests mutated process env under a Mutex — which serialized the env-touching tests against each other but still raced every OTHER parallel test parsing a bare-string selector (observed as flaky failures on two fixture tests). Replaced with a thread-local override seam (`set_auto_ocr_fallback_override`, `#[doc(hidden)]`); tests no longer touch process env at all. 3× consecutive full-bucket runs green.

### Ship gate

- 77 parser tests (+5 new: dispatch ×3 shapes, anchorRelative alias, waitForAnimationToEnd map form ×2) + 88 runtime_mock tests (+3 new dispatch-routing tests) green.
- Full workspace green; Swift build + Android Kotlin compile green.
- Real-sim smoke on Preferences (iOS) for the probe-ignore change.

## [1.0.25] — 2026-07-13

Two follow-up fixes to the v1.0.24 D1/D2 OCR-visibility work, from validating a conditional-gate ceremony (`runFlow when.notVisible: qa-bubble file: qa-gate-passcode.yaml`) on a real batch:

### `SMIX_AUTO_OCR_FALLBACK` splits regex-OR strings per alternative on OCR tier

Pre-v1.0.25, bare-string `visible: 'A|B'` under the v1.0.23 auto-lift became:
```
fallback: [text: /A|B/i, ocrText: 'A|B']
```
Text tier's regex worked; OCR tier's literal `"A|B"` was never on screen — Apple Vision doesn't interpret pipes. Observed in the field: a launch flow used `visible: 'Log in to MyApp|Device'` (legacy regex OR), the auto-lift silently misfired OCR, and the launch chain timed out at 45 s.

Fix: split on top-level `|` and emit one `OcrText` tier per alternative AFTER the single regex text tier:
```
'A|B'       → fallback: [Text('/A|B/i'), OcrText('A'), OcrText('B')]
'A|B|C'     → fallback: [Text('/A|B|C/i'), OcrText('A'), OcrText('B'), OcrText('C')]
'Sign In'   → fallback: [Text('Sign In'), OcrText('Sign In')]      # unchanged, no pipe
```

Tree tier still covers "either A or B" in one probe (cheap); OCR now has real strings per alternative. Character classes (`[A|B]`) and escaped pipes (`\|`) are respected — split only on top-level pipes. Empty alternatives (`|A|` → just `A`) filtered.

4 new parser tests locked (`parse_visible_bare_string_regex_or_splits_ocr_per_alternative`, `..._no_pipe_unchanged`, `..._three_alternatives`, `..._empty_alternatives_filtered`).

### Skipped diagnostic emitted to stderr per step

v1.0.24 improved the `RunStepReport::Skipped { reason }` string to include the selector + evaluation outcome, but the reason only surfaced in `--debug-output/step-N.json`. Under `stdio: inherit` consumers (a harness invoking `spawnSync(SMIX_BIN, ..., { stdio: 'inherit' })`) the diagnostic was invisible.

Fix: emit `STEP N: <verb-summary> → SKIPPED: <reason>` to stderr after each Skipped step. Non-Skipped outcomes stay quiet — no noise for the happy path.

For a batch of flows each short-circuiting on `runFlow when.notVisible: qa-bubble`, consumers now see:
```
STEP 3: runFlow qa-gate-passcode.yaml (conditional) → SKIPPED: runFlow when.notVisible visible=true ({ id="qa-bubble" }); skipped subflow qa-gate-passcode.yaml
```

### Wire compatibility

- The auto-lift shape change only affects yaml parsed under `SMIX_AUTO_OCR_FALLBACK=1`. Pre-v1.0.25 (and OCR-off) parses unchanged.
- Stderr lines are emitted only when a Skipped outcome occurs. Silent for Ok / ExpandedSubflow / errored outcomes.
- CLI / Rust wire types / other subsystems byte-identical to v1.0.24.

### Ship gate

- 70 total parser tests green (+4 new tests, Mutex-serialized env-touching helper reused).
- 119 workspace test-result-ok buckets green (unchanged bucket count).
- CLI smoke on `visible: 'Log in|Device'` under `SMIX_AUTO_OCR_FALLBACK=1` — dry-run parses to 3-tier fallback as spec.

## [1.0.24] — 2026-07-12

**`runFlow.when.visible` + `inputText` silently no-op — fixed.** Root cause: the tree-only visibility check in `runFlow.when.visible` silently drops `Selector::OcrText` at the tree resolver, so a `fallback: [text, ocrText]` gate under iOS 26.5 + RN 0.86 Fabric a11y drop returns false and the whole conditional body is skipped without any signal. `inputText` never fires because the conditional never enters. 3 fixes land:

### `runFlow.when.visible` fires OCR when selector contains OcrText

Runtime dispatch for `Step::RunFlowInline` and `Step::RunFlowConditional` now goes through `check_selector_visible` (introduced in v1.0.23 as the shared "probe once via tree + OCR" primitive) instead of tree-only `App::find`. When the gate selector contains any `OcrText` sub-selector, OCR fires; when it doesn't, tree resolver runs unchanged. Fast path preserved for OCR-free gates.

Concrete scenario from the field:
```yaml
- runFlow:
    when:
      visible:
        fallback:
          - text: 'For internal testers only'
          - ocrText: 'For internal testers only'
    commands:
      - tapOn: { id: 'qa-passcode' }
      - inputText: '0429'
```
Pre-v1.0.24: `text:` misses under Fabric drop → OCR silently skipped → gate returns false → whole body skipped → `inputText` never fires → passcode field stays empty → 45 s wait inside the (never-entered) conditional appears as ~0 s STEP wall.

Post-v1.0.24: OCR fires on the `ocrText` layer → gate returns true → body runs → `tapOn` + `inputText` deliver as expected.

### `runFlow.when.notVisible` inverse gate

Idempotency pattern: "only enter the conditional if the target state hasn't been reached." Enables:
```yaml
- runFlow:
    when:
      notVisible:              # ← NEW — enter gate only if not already past it
        id: 'qa-bubble'
    file: enter-qa-mode.yaml
```
- Runtime: fires the conditional when the selector is NOT visible.
- Same OCR-aware `check_selector_visible` under the hood — no wire divergence from the `visible` gate.
- Mutually exclusive with `when.visible` at parse time (both Some → parse error with clear message).
- Same `#[serde(default, skip_serializing_if = "Option::is_none")]` treatment — additive on the enum.

### Better `runFlow` Skipped diagnostic

Pre-v1.0.24 message: `runFlow when.visible=false; skipped inline body (5 steps)`. Told consumers nothing about WHAT was checked. Now the reason includes the selector's `describe_selector` form:
```
runFlow when.visible=false ({ fallback=[{ text="For internal testers only" }, { ocr_text="For internal testers only" }] }); skipped inline body (5 steps)
```
For `notVisible`:
```
runFlow when.notVisible visible=true ({ id="qa-bubble" }); skipped subflow enter-qa-mode.yaml
```
Consumers get selector shape + evaluation outcome in one line — sufficient to grep the runner log + know if the gate misfired.

### Wire compatibility

- `Step::RunFlowConditional.when_not_visible` + `Step::RunFlowInline.when_not_visible` new fields, both `Option<Selector>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Pre-v1.0.24 serialized flows deserialize with `when_not_visible: None` = unchanged behaviour.
- `runFlow.when.notVisible` parser is additive on the accept-set. Any yaml that parsed before still parses.
- Runtime `evaluate_run_flow_gate` shared method centralizes the gate evaluation for both variants and both gate types.
- No wire / HTTP surface changes.
- CLI / other subsystems byte-identical to v1.0.23.

### Ship gate

- 3 new parser tests: `parse_run_flow_conditional_when_not_visible`, `parse_run_flow_inline_when_not_visible`, `parse_run_flow_when_visible_and_not_visible_together_rejects`. 66 total parser tests green (+3 new).
- Full workspace `cargo check` + `cargo test` green (119 test-result-ok buckets, unchanged bucket count).
- Real-sim empirical validation pending on the next dogfood batch — the failing case (a qa-gate ceremony wrapped in `runFlow.when.visible` with `fallback: [text, ocrText]`) is ready to test.

## [1.0.23] — 2026-07-12

**4 new fixes extending the v1.0.22 OCR-runtime work** to `tapOn` / `scrollUntilVisible`, plus a snapshot-freshness diagnostic and an ergonomic bare-string auto-OCR opt-in. The v1.0.22 OCR-fallback path was validated on real workload (a bootstrap batch that passed 3/3 on v1.0.22 vs 0/3 on v1.0.21 with the same yaml), and the auto-captured `.smix/timeouts/*.png` cut triage-per-failure from 4-5 turns down to 1.

### `tapOn: fallback` implicit poll window when OcrText present

Pre-v1.0.23 `run_tap` fallback was one-shot: try every sub-selector once, exit. On iOS 26.5 + RN 0.86 Fabric the tap moment often races the app's post-transition mount — OCR misses because Vision snapshots BEFORE the target text is visible, not because it isn't there.

Fix: when the fallback contains any `OcrText` sub-selector, the whole chain is now polled within `SMIX_TAP_OCR_POLL_MS` (default 3000 ms) at 250 ms cadence. First hit wins. Fast path (no OCR anywhere) unchanged: single pass, no poll — pre-v1.0.23 semantics preserved for tree-only selectors.

The failure hint now names the poll budget for consumer visibility: `v1.0.23 poll budget (SMIX_TAP_OCR_POLL_MS, default 3000 ms) activates automatically when Fallback contains OCR — bump it if your post-transition mount is slower than 3 s.`

### `scrollUntilVisible` fires OCR between scroll strokes

Pre-v1.0.23 `Step::ScrollUntilVisible` delegated to `driver.scroll`, whose tree-only resolver couldn't see off-screen targets in degraded a11y trees (RN 0.86 Fabric LazyColumn/LazyRow drops off-screen items on iOS 26.5).

Fix: new adapter path `scroll_until_visible_with_ocr` — activates when the selector contains any `OcrText`. Per iteration: probe via `App::find` (tree) AND `App::find_by_text_ocr` (OCR); first hit stops the scroll. `scroll_screen(direction)` swipes between iterations. 30-swipe budget + 20 s wall (matches driver's `driver.scroll`). Fast path (no OCR anywhere) unchanged.

Shared helper `check_selector_visible` used by both the tapOn poll and the scroll poll — single implementation of "probe this selector once via tree + OCR".

### `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` response headers on /tree

Consumers debugging `--all` batch snapshot drift want a signal that the runner is (or isn't) doing fresh work. Fix — two additive headers, wire body byte-identical:

- `X-Tree-Snapshot-Refresh-Count` — cumulative /tree successful serves since runner boot (monotonic UInt64). Consumers subtracting between calls detect stalls.
- `X-Tree-Snapshot-Wall-Ms` — how long THIS `snapshotHandler` invocation took end-to-end. Trending upward across a batch = XCUITest bogging down under sustained JS reload pressure.

An alternative proposal — adding `snapshotAgeMs` / `snapshotRefreshCount` to the JSON body under a wrapping `root` field — was rejected: a wire body wrap would break every existing consumer that parses the top-level as an A11yNode. Headers are additive — pre-v1.0.23 consumers see zero change; new consumers add a single header read.

### `SMIX_AUTO_OCR_FALLBACK=1` bare-string auto-lift

In a real consumer corpus, every flow spelled out the 3-line `visible: fallback: [text, ocrText]` form. Fix — env-opt-in `SMIX_AUTO_OCR_FALLBACK=1` lifts bare-string `visible: 'X'` to `visible: fallback: [text: X, ocrText: X]` at parse time. Reduces yaml boilerplate ~40% for degraded-tree callers; portable back down to versions with less-degraded trees (bare form still parses without the env).

Accepted truthy values: `1`, `true`, `TRUE`, `yes`. Anything else (including unset) leaves bare strings as `Selector::Text` — pre-v1.0.23 semantics preserved.

Reading the env at PARSE time (not RUN time) keeps the emitted Selector shape stable across a flow — you can't have "sometimes this yaml parses to Text, sometimes to Fallback" depending on runtime state, which would violate the parser's determinism contract.

4 new parser tests locked (`parse_visible_bare_string_default_stays_text`, `..._with_env_lifts_to_fallback`, `..._with_env_true`, `..._with_env_zero_stays_text`) + Mutex-serialized to survive Cargo's default parallel test execution.

### Wire compatibility

- `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` headers additive — pre-v1.0.23 consumers see zero change.
- `SMIX_AUTO_OCR_FALLBACK` env-off ⇒ bare-string `visible: 'X'` still parses to `Selector::Text`.
- tapOn / scrollUntilVisible without any OCR in the selector: fast path preserved — no polling, no perf change.
- CLI / other subsystems byte-identical to v1.0.22.

### Ship gate

- 119 test-result-ok buckets across the workspace green (63 parser + 56 elsewhere; +4 new parser tests for the auto-lift).
- Full workspace `cargo check` green + Swift build green.
- Real-sim empirical validation pending on the next dogfood batch — failing cases for both tapOn OCR (a force-update Skip flake) and scrollUntilVisible OCR (an off-screen deeplink panel) are ready.

## [1.0.22] — 2026-07-12

**iOS 26.5 + RN 0.86 Fabric tree-degradation triage upgrade.** On Xcode 26.6 + iOS 26.5 sim + RN 0.86 New Arch (Fabric), `GET /tree` returns every child under the app root with empty `identifier` and empty `label` — nodes visibly showing (e.g.) a login button that carries JSX `testID` + `accessibilityLabel` + `accessibilityRole="button"` + `accessible={true}`. Bootstrap flows time out on the first `extendedWaitUntil` regardless of resetAppData / clearAppData choice, and the `fallback: [ocrText]` last resort silently never fires. Three fixes land:

### `extendedWaitUntil.visible.fallback: [ocrText: ...]` now actually calls OCR

Parser accepted `ocrText` in fallback since v1.0.20, but the runtime dispatched every selector through `App::wait_for` which uses the tree resolver — the tree resolver skips `Selector::OcrText` (correct behavior in isolation; OCR is meant to be dispatched at the adapter layer). Consumers who spelled `fallback: [id, text, ocrText]` got 45 s of pure `/tree` polls and never a single Vision call.

New adapter method `wait_for_visible_with_ocr` splits the fallback per poll iteration:
- Tree-resolvable sub-selectors (Id / Text / Label / Role / LocalizedText / Anchor / AnchorRelative / Focused / Point) fire via `App::find`.
- `OcrText` sub-selectors fire via `App::find_by_text_ocr`.
- First hit wins. OCR members run LAST in each iteration so tree hits pre-empt OCR cost.
- Standalone `Selector::OcrText` at top level: polls `find_by_text_ocr` on the same 250 ms cadence as the driver.
- Fast path: selectors without any `OcrText` anywhere still delegate to `App::wait_for` unchanged.

Timeout emits a per-layer trace: `L1 id=btn-…: MISS; L2 text=Log in: MISS; L3 ocrText=Log in: MISS`.

### Screenshot + tree JSON always captured on `extendedWaitUntil` timeout

Pre-v1.0.22 required `--debug-output <dir>` to get a fail PNG + tree snapshot. Consumers debugging a tree-degradation regression in CI didn't have that wired; every timeout left them blind. Now every `extendedWaitUntil` timeout auto-captures both.

Sink resolution:
1. `--debug-output <dir>` if set (same convention as per-step debug).
2. Else `<CWD>/.smix/timeouts/` (repo-scoped triage; already in typical gitignores).
3. Else `~/.local/share/smix/timeouts/`.

File names: `timeout-extendedWaitUntil-<epoch-ms>.png` + `.tree.json`. The written paths are appended to the failure's existing hint (`v1.0.22 timeout capture: screenshot=<path> tree=<path>`) so AI-readable output surfaces them.

Best-effort: any screenshot / tree / I/O error is logged to stderr and does not affect the failure verdict.

### `A11yNode.elementTypeRaw` numeric on wire (partial fix for RN Fabric tree gap)

The root-cause diagnosis: iOS 26.5 XCUITest returns empty `identifier` and empty `label` for RN 0.86 Fabric-mounted views despite the JSX setting `testID` and `accessibilityLabel`. That's an app-side (RN → UIAccessibility bridge) issue, not a smix serializer bug. But smix consumers had no way to see that from the wire: `rawType` was the only exposed type info, and the numeric `XCUIElement.ElementType.rawValue` was lost.

Now `A11yNode.elementTypeRaw: u64` ships on every wire payload. Consumer client-side triage:
- `elementTypeRaw != 1 && identifier == "" && label == ""` ⇒ iOS types this as a real element (`.button`, `.textField`, `.staticText`, ...) but the a11y bridge dropped its name — app-side fix needed (RN 0.86 Fabric accessibility bridge on iOS 26.5).
- `elementTypeRaw == 1` (`.other`) ⇒ plain wrapper view, expected to be nameless.

Consumers can now distinguish "smix bug" from "RN bridge dropped the name" in one field lookup.

Additive on the A11yNode wire; `#[serde(default = "default_element_type_raw")]` returns 1 (`.other`) for pre-v1.0.22 payloads.

### Wire compatibility

- `DiagnosticDumpResponse` unchanged.
- `A11yNode.elementTypeRaw: u64` new (default 1) — pre-v1.0.22 consumers ignoring it see zero behaviour change.
- `extendedWaitUntil` semantics preserved for selectors without OCR anywhere.
- Timeout capture is additive — hint on the failure gets extra lines; the failure code / message / structure otherwise unchanged.

### Ship gate

- 119 test-result-ok buckets across the workspace (all pre-existing + new); no regressions.
- Full workspace `cargo check` green.
- Real-sim empirical validation pending on the next dogfood batch: the failing case — `fallback: [id, text, ocrText]` yaml + a screen where the a11y tree is degraded — is ready. If OCR fires and the tree-JSON capture surfaces at timeout, the first two fixes are proved. The numeric type field informs the app-side fix or the choice to fall through to OCR.

## [1.0.21] — 2026-07-12

**iOS 26.5 UIAlertController button role mapping fixed.** Dogfooding reported `tapOn: { role: button, name: 'Reload' }` (newly-parsing in v1.0.20) regressed 3/3 flows on iOS 26.5 sim — the wire and parser are correct, but iOS 26.5 XCUITest now exposes `UIAlertController` action buttons with `elementType == .other` (rawValue 1) instead of `.button` (rawValue 9). Same failure mode expected for SwiftUI `.confirmationDialog`, `.actionSheet`, keyboard `return`/`done` bar buttons on iOS 26+.

### Swift-side action-container button promotion

Fixed at the perception layer (`swift-bridge/Sources/SmixRunnerCore/TreeRoute.swift`, `nodeToDict`). When emitting a tree snapshot, if a node is inside an `.alert` / `.dialog` / `.sheet` ancestor at any depth AND has a non-empty label AND its own elementType is `.other` (1) or `.staticText` (48), the wire `rawType` is promoted to `"button"`. This preserves `role: button` semantics across iOS versions without requiring per-consumer yaml patches.

- Promotion is **ancestor-scoped**, not global — a `.other` node outside an action container stays `"other"`.
- Promotion requires a **non-empty label** — decorative background views under an alert are not swept up.
- Promotion **never demotes** — a real `.button` (rawValue 9) inside an action container stays `"button"`.
- Nested containers (a sheet inside an alert) don't loop-double-promote; we track a single boolean.

### Wire compatibility

- `rawType` field on the wire is unchanged in shape (still `String`).
- Existing yaml `role: alert` / `role: dialog` / `role: sheet` targeting the container itself is unaffected — we only touch descendant elementTypes, not the container's own.
- Pre-v1.0.21 consumers that were matching alert-buttons via `text:` or `id:` still see the same match — text and id fields aren't touched.
- CLI / adapter parser is byte-identical to v1.0.20.

### Ship gate

- 7 new Swift unit tests in `TreeRouteTests` (`test_serialize_alertOtherChildWithLabel_promotedToButton`, `..._alertStaticTextChildWithLabel_...`, `..._dialogNestedButton_...`, `..._alertOtherChildNoLabel_notPromoted`, `..._otherOutsideActionContainer_notPromoted`, `..._realButtonUnderAlert_stillButton`, `..._sheetOtherChild_...`) — 26 TreeRoute tests total, all green.
- Real-sim empirical verification pending on the next dogfood batch (the failing 3/3 case — an alert-button `role: button, name: 'Reload'` yaml — is ready and will confirm v1.0.21 resolves it).

## [1.0.20] — 2026-07-12

**3 docs/impl gaps closed.** The v1.0.19 observability wins (`lastInteractiveNamedIds` at top-level + `AppUnavailableReason` disambiguation) held up on real workload — bootstrap batch flow completion reached 2/3 passing, with the remaining failure isolated to a downstream app-side native race (`expo::setProperty` / `ConstantDefinition.buildDescriptor`), not smix.

### `extendedWaitUntil.visible` accepts every selector key `tapOn` does

`docs/ai-guide/03-selectors.md` promised `ocrText:` as a first-class selector everywhere. Reality: `visible_to_selector` in `crates/smix-adapter-maestro/src/parser.rs` only accepted `text` and `id`. Fixed — now accepts every base selector form: `text`, `id`, `label`, `role` (+ optional `name`), `ocrText`, `localized_text`, `fallback`.

All 8 verbs that route through `visible_to_selector` benefit at once: `extendedWaitUntil.visible/.notVisible`, `assertVisible`, `assertNotVisible`, `scrollUntilVisible`, `copyTextFrom`, `runFlow.when.visible`, `tapOn.anchored.anchor`.

### `tapOn: {role, name}` + `tapOn: {label}` parse

`Selector::Role` wire type exists (since v5.x); the yaml parser just wasn't wiring it. Fixed — `parse_tap_on` now accepts:

```yaml
- tapOn:
    role: button        # camelCase (wire) or lowercase (docs-friendly) — both work
    name: 'Submit'      # optional Pattern (literal or |-alternation regex)

- tapOn:
    label: 'Home tab'   # accessibilityLabel strict equal (Selector::Label)
```

Role parser tolerates docs-friendly aliases: `role: textfield` → `Role::TextField`; `role: checkbox` → `Role::CheckBox`; `role: heading` → `Role::StaticText` (nearest wire equivalent since iOS/SwiftUI has no `.header` XCUIElement type). Unknown roles emit an actionable error listing every accepted variant.

### `smix run --dry-run` alias for `--check`

`--check` already existed with the exact "parse-only, no runner, no simulator" semantics consumers asked for, but `--dry-run` is the idiomatic name in most CLI tools. Added `--dry-run` as a clap alias for `--check`; output prefix changed to neutral `smix run: parse OK/FAIL <path> (N steps)` so it reads correctly under either name. Also appends a summary line: `smix run: parse OK — N flow(s), M total step(s)`.

### Wire compatibility

- `smix_selector::Role` re-exported at crate root (was `pub use smix_screen::Role` internally) — adapter crates can now `use smix_selector::Role` without pulling `smix-screen` directly.
- All parser changes are additive on the accept-set — no yaml that parsed before still fails.
- Docs updated in `docs/ai-guide/03-selectors.md §4 Role` to enumerate every supported role and note the "role works anywhere a selector map does" broader guarantee.

### Ship gate

- 59 parser tests (5 new: `parse_extended_wait_until_visible_ocr_text`, `..._role_name`, `..._label`, `parse_tap_on_role_name`, `..._role_lowercase_alias`, `..._role_unknown_errors_actionably`, `..._label`) + 25 CLI runner tests + all pre-existing green across touched crates.
- CLI dry-run smoke on 3-step yaml with `tapOn: {role, name}` + `extendedWaitUntil.visible: {ocrText}` + `tapOn: {label}` — parses clean, reports "3 steps, 1 flow".
- Unknown role smoke — emits full accepted-roles list, exit 2.

## [1.0.19] — 2026-07-12

**Post-mortem triage QoL.** Real-workload validation of v1.0.18 confirmed:
- Both v1.0.18 wins (D1 per-session `interactiveNamedIds` + D2 `waitForAnimationToEnd: N`) landed cleanly.
- **`.ips` growth 36→36 across 5 consecutive batches** — native cold-boot crash chain closed decisively (v1.0.14 → v1.0.18).
- Flow depth advanced 6–8 steps in every case; remaining stalls all on target-screen `waitFor { text: … }` (downstream RN Fabric a11y-label propagation, not smix).

### Top-level `lastInteractiveNamedIds` on `/diagnostic/dump`

Per-session `interactiveNamedIds` (v1.0.18) goes with the session when `close-all` teardown fires. Post-batch triage often runs AFTER teardown, so the sample vanishes right when consumers want it.

Wire additions (all `#[serde(default)]`, backward-compat):
- `DiagnosticDumpResponse.last_interactive_named_ids: Vec<String>` — most-recent non-empty sample across all `launchApp` completions since runner boot. Survives session close.
- Swift `SessionRoute.DiagnosticSnapshot.lastInteractiveNamedIds: [String]`; runner-side `LastInteractiveIdsBox` holder updated on every non-empty launch outcome.
- `smix diagnostic dump` (text mode) prints one line: `lastInteractiveNamedIds (N): id1, id2, ...` or `[]  # no launch has completed with a non-empty sample yet`.
- `smix diagnostic dump --json` emits the same field on the top-level `runner` object.

Per-session `sessions[n].interactiveNamedIds` from v1.0.18 remains — this new top-level field is the "last-values-standing" post-teardown observation surface, not a replacement.

### Wire compatibility

- `DiagnosticDumpResponse.last_interactive_named_ids` is `#[serde(default)]` + on a `#[non_exhaustive]` struct — pre-v1.0.19 consumers ignoring it see zero behaviour change.
- No new HTTP routes. No CLI flag changes. No yaml schema changes.

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline v1.0.18 behaviour unchanged; every previous assertion still holds.
- **Verified**: after 1 launch of Preferences, `curl -s -X POST /diagnostic/dump | jq '.lastInteractiveNamedIds'` returns the same 8-name sample as `sessions[0].interactiveNamedIds` and as the launch-app response. After closing that session via `/session/close-all`, `sessions` becomes empty but `lastInteractiveNamedIds` still holds the 8-name sample.

## [1.0.18] — 2026-07-12

**Two QoL additions**, landed alongside real-workload validation of v1.0.17:
- v1.0.17 crash fix confirmed working (0 test_runForever failures, 0 "Failed to get matching snapshot" entries)
- `launchAppReachedInteractive: 6/6` — every launch reached probeable tree
- `.ips` growth 36→36 — native crash triple stays fully closed
- Remaining flow failures **not a smix bug** — RN Fabric a11y-exposure lag during animation (post-tapOn transitions); downstream timeouts + testIDs + `waitForAnimationToEnd` are the knobs

### Per-session `interactiveNamedIds` in `session/list` + `/diagnostic/dump`

Previously only surfaced on `/session/launch-app` response body. The counter alone doesn't tell "probe fired on dev-bubble" from "probe fired on splash-screen artifacts."

Wire additions (all `#[serde(default)]`, backward-compat):
- Swift `SessionRoute.SessionSummary.interactiveNamedIds: [String]` (default empty).
- Swift `SessionEntry.lastInteractiveNamedIds: [String]` on the session table — updated on every `launchApp` completion.
- `session/list` + `/diagnostic/dump` JSON both now include `sessions[n].interactiveNamedIds`.

### `waitForAnimationToEnd` numeric override + doc

Consumers weren't sure if `waitForAnimationToEnd` was a no-op under `SmixQuiescenceSwizzle.m`. Reality: it never went through XCTest idle-wait in the first place — it's always been a fixed 400 ms `tokio::time::sleep`. Undocumented.

Fix:
- yaml accepts `- waitForAnimationToEnd: 500` (integer = ms sleep). Bare form still parses to 400 ms default (maestro-compat).
- `Step::WaitForAnimationToEnd { duration_ms: u64 }` — struct variant.
- Runtime dispatch sleeps the requested milliseconds.
- Docstring on the variant explicitly names that it's a fixed sleep, NOT XCTest quiescence.

2 new parser tests locked (`bare_default_400ms`, `numeric_override`).

### Wire compatibility

- `SessionSummary.interactiveNamedIds` is `#[serde(default)]` — pre-v1.0.18 consumers ignoring the field see zero behaviour change.
- `Step::WaitForAnimationToEnd` variant became `{ duration_ms }` — consumers of the yaml wire (yaml → Step conversion, not `Step` construction in user code) unaffected. Test fixtures using struct literal `Step::WaitForAnimationToEnd` updated.
- No runner-side HTTP surface changes.

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline: `POST /session/launch-app` still returns `reachedInteractive:true` + 8 sample ax-ids as v1.0.17.
- **Verified**: after launch, `session/list` and `/diagnostic/dump` both surface `sessions[0].interactiveNamedIds: ["Settings","AdditionalDimmingOverlay","com.apple.settings.primaryAppleAccount",…8]`. Same 8-name sample as the launch-app response.

682 workspace cargo tests (+2 new parser tests for D2) + all pre-existing green. No wire regressions.

## [1.0.17] — 2026-07-12

**Hotfix: v1.0.16 introduced a hard-crash mode in the interactive polling loop.** Root cause: `descendants(matching:).element(boundBy: i)` is XCTest-lazy — the element resolves at access time against the CURRENT tree. When the tree shrunk mid-iteration (a `stopApp + openLink dev-launcher` between test phases), XCTest raised an unrecoverable assertion "No matches found for Element at index N" that killed `test_runForever` and the runner process, taking subsequent flows down with it.

**Before this crash surfaced, the v1.0.16 snapshot-refresh DID help** — `force-update.yaml` reached STEP 47/47 vs the previous max of 34, and `.ips` growth stayed at 36 → 36 (native crash triple stays fully closed).

### Walk frozen `XCUIElementSnapshot` instead of live-query enumeration

Replaces:
```swift
_ = try? entry.app.snapshot()
let query = entry.app.descendants(matching: .any)
for i in 0..<query.count {
  let el = query.element(boundBy: i)   // lazy resolution at access → hard-fail on shrink
  ...
}
```

with:
```swift
guard let snap = try? entry.app.snapshot() else { return [] }
collectInteractiveIds(snap.dictionaryRepresentation, ignore, ids, ...)
```

- `snap.dictionaryRepresentation` returns a frozen in-memory tree that we walk recursively, collecting non-empty `accessibilityIdentifier` values not in the ignore list. Same pattern the runner already uses for modal popup collection (see `collectPopupNodes`) and keyboard focus detection (see `FocusedIdentifier.find`).
- `snapshot()` itself still forces XCUITest to re-scrape the a11y hierarchy from scratch (v1.0.16 fix for the Fabric mount-item-drain race). The walk over the returned snapshot is safe against any subsequent tree mutation.
- Pathological-tree stall guard: walk stops at 200 enumerated nodes (guards against runaway lists).

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline: `POST /session/launch-app waitForInteractiveMs:15000` → `HTTP 200, reachedInteractive:true, interactiveNamedIds:["Settings","AdditionalDimmingOverlay",…8]`. Snapshot-walk yields the same result as v1.0.15/v1.0.16 on the working Preferences case.
- **Stress test — 3 rapid terminate + launch cycles** to trigger the tree-shrink race pattern observed in the field. Every cycle returned `reachedInteractive:true` and runner stayed reachable after all cycles. `/health` still returning 200. v1.0.16 in the same scenario would have crashed after 1-2 cycles.

### Wire compatibility

- No wire changes. All v1.0.15 wire shape unchanged.
- Runner-side behavior change is invisible to consumers unless polling was hitting the tree-shrink race, in which case runner-death → runner-alive is the observation flip.

680 workspace tests + all pre-existing tests green.

## [1.0.16] — 2026-07-12

**Hotfix: v1.0.15's interactive polling had a stale-snapshot bug on RN Fabric + iOS 26.5 sim.** The exact race: RN 0.86 Fabric New Arch populates the a11y tree via `RCTMountItemProtocol` as mount items drain, NOT during layout. XCUITest's snapshot cache holds the sparse pre-drain tree, and `descendants(matching:)` returned the same cached snapshot every poll iteration.

### Swift snapshot-refresh in interactive polling

- `launchApp` handler now calls `_ = try? entry.app.snapshot()` on every polling iteration before reading `descendants(matching:)`. Forces XCUITest to re-scrape the a11y hierarchy from scratch, catching mount-item-drain updates.
- No `waitForQuiescenceIncludingAnimations` call — smix's existing `SmixQuiescenceSwizzle.m` already no-ops that private XCTest daemon idle-wait for performance. Snapshot alone forces the invalidation.
- `.smix/config.yaml interactiveProbe:` schema unchanged. Config-driven ignore-list and minIdentifierCount still work as v1.0.15 shipped.

### yaml `launchApp: { waitForInteractiveMs }` marker

- Parser accepts the new field on the map form of `launchApp:`.
- `Step::LaunchApp.wait_for_interactive_ms: Option<u64>` — additive; `#[serde(default)]`.
- Runtime: emits a warning (non-fatal) explaining the SDK launch pathway (`simctl launch --args`) is host-side and can't route to `/session/launch-app`. Consumers who want interactive gating use the `clearAppData` yaml verb instead — its SDK path defaults `wait_for_interactive_ms: Some(30_000)` since v1.0.15. Full first-class routing lands in a follow-up release that unifies the two launch pathways.

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline reproducibility check — the v1.0.16 snapshot-refresh doesn't regress the working Preferences case that v1.0.15 shipped on:

```
POST /session/launch-app  {sessionId, waitForForegroundMs:15000, waitForInteractiveMs:15000}
→ HTTP 200
→ reachedInteractive:true
→ interactiveNamedIds:["Settings","AdditionalDimmingOverlay",
                       "com.apple.settings.primaryAppleAccount", …8]
```

Real-world validation (a consumer bootstrap batch on RN Fabric + iOS 26.5) happens downstream — launch-fresh flows migrate to `clearAppData` (which gets the interactive probe with v1.0.15's default and now v1.0.16's snapshot-refresh) and rerun.

### Wire compatibility

- No wire changes (v1.0.15 wire shape unchanged).
- Runner-side behavior change is invisible to consumers unless the polling loop was hitting the stale-snapshot case; when it was, the observation flip is (a) v1.0.15 always saw `reachedInteractive:false` on Fabric or (b) v1.0.16 sees `reachedInteractive:true` once the tree actually populates.

680 workspace tests + all pre-existing tests green.

## [1.0.15] — 2026-07-11

**Interactive-probe polling + app-unavailable reason disambiguation + retry attribution — the v1.0.14 deferred work.** Wire scaffolding from v1.0.14 now populated with the Swift + CLI implementation.

### Interactive-probe polling (Swift-side)

- Wire: `SessionAppLifecycleRequest.wait_for_interactive_ms: Option<u64>` (additive; `#[serde(default)]`).
- Wire response: `SessionAppLifecycleResponse.reached_interactive: bool` + `interactive_named_ids: Vec<String>` (up to 8 sample ax-ids captured at fire moment).
- Runner: after `.state == .runningForeground` is observed, the `launchApp` handler polls `entry.app.descendants(matching: .any)` at 500 ms cadence, counts descendants with non-empty `accessibilityIdentifier` NOT in the ignore-list, fires `reachedInteractive` on ≥ `minIdentifierCount`, or times out and increments `launchAppTimedOutBeforeInteractive`.
- Config file: `.smix/config.yaml interactiveProbe: { minIdentifierCount: 3, ignore: [SplashScreenLogo, com.example.app] }`. CLI reads via `serde_norway`, JSON-encodes, forwards to runner as `TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON`. Runner falls back to bundled defaults when absent.
- SDK: `App::clear_app_data_with_launch` defaults `wait_for_interactive_ms: Some(30_000)` — consumers using yaml `clearAppData` automatically see `launchAppReachedInteractive` counter delta with zero yaml migration.
- Counter fields `launch_app_reached_interactive` + `launch_app_timed_out_before_interactive` in `SessionLifecycleCounters` are now populated by the runner (were 0 in v1.0.14 wire-scaffold).

### `AppUnavailableReason` enum + hint field on `/tree` unavailable envelope

- Swift `TreeRoute.unavailable(reason:hint:)` variant emits enriched `{"ok":false,"error":"snapshot_unavailable","reason":"alive-but-tree-empty","hint":"…"}` body. Legacy `TreeRoute.unavailable()` still present for compat.
- Swift `AppUnavailableReason` enum: `crashedDuringInit` / `aliveButTreeEmpty` / `aliveButTreeStale` / `driverDisconnected` / `unknown`. Each carries a `defaultHint: String` steering downstream tooling.
- Runner-side detection in `SmixRunnerServer.swift` `/tree` handler:
  - Cache-suppressed short-circuit → `crashed-during-init` (observed XCTIssue about app not running).
  - Snapshot handler returned nil → consults `currentUnavailableReasonInferer` task-local closure. UITest target reads `XCUIApplication.state` for the current bundle: `.notRunning` → `crashed-during-init`; foreground/background running → `alive-but-tree-empty`; unknown → `.unknown` fallback.
  - Fallback (guarded closure threw entirely) → `driver-disconnected`.
- Wire in `smix-runner-client`: `RunnerTransportError::AppUnavailable` gains `category: Option<String>` + `hint: Option<String>` fields. `classify_error_body` discriminates v1.0.15 category values (`crashed-during-init` etc.) from legacy free-form `reason` strings; both populate cleanly for backward compat.
- Pre-v1.0.15 runners emitting legacy `{"ok":false,"error":"snapshot_unavailable"}` land in `category: None, hint: None` — the consumer's error message stays functional either way.

### `smix run --retry N` + per-flow attempt attribution

- CLI: new `--retry <N>` flag on `smix run` (default 1 = pre-v1.0.15 behaviour).
- Runtime: each flow wrapped in an attempt loop; retries only fire on non-zero exit; first success short-circuits.
- Per-attempt tracking captures `attempt_index`, `status` (`ok`/`timeout`/`error`), `error_class` (`TIMEOUT`/`DRIVER_ERROR`/`EXPECTATION_FAILURE`/`RUNNER_UNREACHABLE`), `wall_ms`, and any new `.ips` filename that appeared under `~/Library/Logs/DiagnosticReports/` during the attempt's window (attribution vs whole batch).
- Persistence: `~/.local/share/smix/flow-attempts.json` (last 32 flows) via new `smix-simctl::set_flow_attempts_persist_path` (parallels the v1.0.7 `subprocess_ring` and v1.0.14 `reset_app_data_counters` patterns).
- CLI dump overlay: `smix diagnostic dump` (non-JSON) renders a new `=== recent flows (retry attribution) ===` section per flow with per-attempt lines; `--json` payload gets `runner.recentFlows: Vec<FlowAttemptRecord>` (wire type land in v1.0.14).

### Wire compatibility

- All new request/response fields carry `#[serde(default)]`. Pre-v1.0.15 clients see zero behaviour change.
- `SessionAppLifecycleRequest.wait_for_interactive_ms: Option<u64>` — opt-in.
- `SessionAppLifecycleResponse.reached_interactive: bool` + `interactive_named_ids: Vec<String>` — additive.
- `TreeRoute.unavailable(reason:hint:)` — new variant; legacy `unavailable()` kept.
- `RunnerTransportError::AppUnavailable.category` + `.hint` — additive Option fields.
- `SessionLifecycleCounters.launch_app_reached_interactive` + `launch_app_timed_out_before_interactive` — already in v1.0.14 wire; v1.0.15 populates.
- `DiagnosticDumpResponse.recent_flows` — already in v1.0.14 wire; v1.0.15 populates via CLI overlay.

### Ship gate (real-sim, iOS 26.5)

```
$ smix --version                                     → smix 1.0.15
$ smix runner install --force                       → extracted 303 files at v1.0.15
$ /health.runnerVersion                              → "1.0.15"

$ curl -X POST /session/open …
$ curl -X POST /session/launch-app -d '{"sessionId":"…","waitForForegroundMs":15000,"waitForInteractiveMs":15000}'
→ HTTP 200
→ reachedInteractive: true
→ interactiveNamedIds: ["Settings", "AdditionalDimmingOverlay", "com.apple.settings.primaryAppleAccount", …8 sampled]

$ smix diagnostic dump | grep -A1 interactive
  interactive: reachedInteractive=1 timedOutBeforeInteractive=0  # timedOut>0 → process foreground but a11y tree unusable

$ /diagnostic/dump payload sessionCounters
  launchAppReachedInteractive: 1
  launchAppTimedOutBeforeInteractive: 0
```

680 workspace tests + all pre-existing tests green. `smix run --retry` mechanism not exercised in real-sim gate (needs a yaml with flaky expectations to fail-then-retry, out of scope for Preferences smoke); implementation locked by static tests.


## [1.0.14] — 2026-07-11

**resetAppData verb (URL-scheme JS-wipe) + external metro log tail (`--metro-log <path>`) + verb-selection guide.**

Version jump 1.0.11 → 1.0.14 (no interim v1.0.12 or v1.0.13 published).

### `resetAppData` verb (URL-scheme JS-wipe)

Fixes the "dev-fixture ceremony cost" problem: every prior `clearAppData` wiped the app's container INCLUDING expo-dev-client's persisted metro URL + Metro bundle cache + dev-tools state — replaying a 15-30 s dev-client cold-boot ceremony every launch.

New verb: `resetAppData` fires an app-owned URL scheme on the host (`simctl openurl <UDID> <url>`), optionally waits for a completion signal on the external metro log tail, then returns. No container tear. Consumer app decides scope (typically `mmkv.clearAll()` + `console.log('[dev] reset-complete token=<uuid>')`).

yaml shapes:

```yaml
# short form
- resetAppData: 'myapp://dev-mutate?action=reset'

# map form
- resetAppData:
    via: url-scheme            # only 'url-scheme' today; extensible
    url: 'myapp://dev-mutate?action=reset'
    waitFor:
      logLinePattern: '\[myapp-dev\] reset-complete token='
      # OR: sleepMs: 500 (best-effort fallback when --metro-log unset)
    timeoutMs: 5000
```

- `Step::ResetAppData { url, wait_for, timeout_ms }` in `smix-adapter-maestro`; parser accepts short-form + map-form.
- `smix_sdk::ResetAppDataWaitFor` enum (`LogLinePattern(String)` / `Sleep(u64)`) shared between adapter Step and SDK.
- Runtime dispatch fires `simctl openurl` via `App::open_url`, then either sleeps or awaits a `smix_metro_log::MetroLogTail::await_signal` match — the tail is provided by `smix run --metro-log <path>`.
- `smix-simctl::increment_reset_app_data_total()` + `increment_reset_app_data_timed_out()` counters, persisted to `~/.local/share/smix/reset-app-data-counters.json` so `smix diagnostic dump` (later, separate process) surfaces the counts.

Wire counter fields in `SessionLifecycleCounters`: `reset_app_data_total`, `reset_app_data_timed_out`. CLI-side populated (host-side dispatch, no runner HTTP round-trip for the reset itself).

### External metro log tail (`--metro-log <path>` on `smix diagnostic dump`)

Fixes the "log gate skipped — metro was already running externally" problem: consumers who spawn metro externally (`nohup bun dev > /tmp/metro.log`) couldn't see JS-side log signal when a flow stalled.

- New CLI flags on `smix diagnostic dump`:
  - `--metro-log <path>` — tail the last N lines from this file at dump time.
  - `--metro-log-tail-lines <N>` — default 200.
- New `tail_lines(path, n)` helper — seeks from EOF in 8 KB chunks, splits on `\n`, handles UTF-8 split across chunk boundaries, files smaller than one chunk, files with no trailing newline. 6 unit tests locked.
- New wire field `DiagnosticDumpResponse.metro_log_tail: Vec<String>` — CLI-side populated at dump time (not runner). Backward-compat additive.
- CLI display gains a `=== metro log tail (last N of file) ===` section when populated.
- Also lands `smix diagnostic dump` sections for `resetAppData` counters + `interactive` counters (v1.0.15 will populate the latter).

For runtime tail during `smix run` (used by v1.0.14's `resetAppData waitFor: { logLinePattern }` and pre-existing `expect.signal` verbs), the existing `smix-metro-log FileTailSubscriber` + `MetroLogTail` continue to serve — no new subscriber design required.

### Verb-selection guide

- A verb-selection guide — decision tree + comparison matrix for `clearAppData` vs `resetAppData` vs `clearState + clearKeychain`, plus a migration crib from pre-v1.0.14 yaml to the split baseline + fast-path pattern.

### Forward-compat wire scaffolding (populated in v1.0.15)

Wire types added in v1.0.14, Swift/impl side deferred to v1.0.15 so consumers get a coherent interactive-probe release rather than a half-populated one:

- `SessionLifecycleCounters.launch_app_reached_interactive` + `launch_app_timed_out_before_interactive` (interactive-probe counters; Swift-side polling not yet wired — always 0).
- `FlowAttemptRecord` + `FlowAttempt` types + `DiagnosticDumpResponse.recent_flows: Vec<FlowAttemptRecord>` (retry attribution; --retry N mechanism not yet wired — always empty).
- All `#[serde(default)]` — a v1.0.14 consumer ignoring the fields sees zero behaviour change; v1.0.15 populates the same fields without a wire migration.

### Wire compatibility

- New request/response fields carry `#[serde(default)]` everywhere.
- `Step::ResetAppData` is a new parser entry — pre-v1.0.14 yaml unaffected.
- `SessionLifecycleCounters` gains 4 fields (2 populated by `resetAppData`, 2 scaffolded for the interactive probe).
- `DiagnosticDumpResponse` gains 2 fields (`metroLogTail` populated CLI-side, `recentFlows` scaffolded).
- No route path changes; no HTTP method changes; no runner-side behaviour change (all v1.0.14 work is on the CLI + host side).

### Ship gate observations (real-sim, iOS 26.5)

```
$ smix --version                                                              # → smix 1.0.14
$ smix runner install --force                                                 # → extracted 303 files at v1.0.14
$ cat ~/.local/share/smix/runner/.smix-runner-version                         # → 1.0.14
$ smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.apple.Preferences
runner up: http://localhost:22087/health = 200 (runner v1.0.14)
$ curl -s http://127.0.0.1:22087/health | jq .runnerVersion                   # → "1.0.14"

# --metro-log tail render
$ echo -e "line-1\nline-2\nline-3\nline-4\nline-5" > /tmp/test-metro.log
$ smix diagnostic dump --metro-log /tmp/test-metro.log --metro-log-tail-lines 3
=== metro log tail (last 3 of file) ===
  line-3
  line-4
  line-5

# New counter sections render
$ smix diagnostic dump | head
  resetAppData: total=0 timedOut=0  # timedOut>0 → URL scheme fired but reset-complete log-line never arrived
  interactive: reachedInteractive=0 timedOutBeforeInteractive=0  # timedOut>0 → process foreground but a11y tree unusable
```

680 workspace cargo tests + 3 new clearAppData parser tests + 3 new resetAppData parser tests + 6 new `tail_lines` unit tests + 1 new reset-app-data-counters roundtrip test green.


## [1.0.11] — 2026-07-11

**launchApp launchArgs/launchEnv + wait-for-foreground + always-emit aliveCache + terminate-outcome counters.**

### The three v1.0.10 followup gaps closed

- **`aliveCache: null` in `/diagnostic/dump`.** Root cause: `SessionHandlers.diagnostic` closure read `SmixRunnerServer.currentAppAliveCache` (task-local); FlyingFox's per-request task spawn wasn't propagating the `withValue` scope around `server.run()`. Fix: `test_runForever()` now extracts `AppAliveCache` to a named local `localAppAliveCache` and the diagnostic handler closure captures the reference directly (not via task-local). The dump payload always emits `aliveCache` with a `wired: bool` sentinel + all-zero counters when unwired, so consumers can distinguish "runner has no cache" from "cache present, workload didn't fire".
- **Expo SDK 57 dev-launcher server picker blocking business flow.** `clearAppData` wipes the dev-client's persisted metro URL, next launch shows the picker, SDK 57 URL scheme no longer auto-navigates. Fix: `launchApp` HTTP endpoint (and yaml `clearAppData` step) accept optional `launchArgs: []` and `launchEnv: {}` fields; forwarded to `XCUIApplication.launchArguments` / `.launchEnvironment` before launch. Consumers steer the picker via `-EXInternalMetroPort` launchArg or `EX_DEV_CLIENT_METRO_URL` launchEnv (fixture57 accepts both).
- **`bug_type: 309 exec_terminated_before_ready` `.ips` writes during clearAppData.** Diagnosis: `XCUIApplication.launch()` returns after launch is dispatched, not when the app has signalled launchd ready. Caller's next step (or a batch retry firing another clearAppData) hits terminate mid-launch → XCUIApplication times out cooperative-terminate → falls back to hard kill → launchd catches `exec_terminated_before_ready` → `.ips`. Fix: `launchApp` endpoint accepts `waitForForegroundMs: Option<u64>`. When set, the runner polls `XCUIApplication.state` every 250 ms until `.runningForeground` or the deadline. `App::clear_app_data` defaults to 15 s. Response body carries `waitedMs` + `terminalState` (0 unknown / 1 notRunning / 2 runningBackgroundSuspended / 3 runningBackground / 4 runningForeground) so `/diagnostic/dump` can surface `launchAppReachedForeground` vs `launchAppTimedOutBeforeForeground` counters.

### CLI (Rust)

- **`SessionAppLifecycleRequest` gains `args`, `env`, `wait_for_foreground_ms`** (`#[serde(default)]`; pre-v1.0.11 callers see zero behaviour change). Non-`#[non_exhaustive]` on the request side (consumers construct) but IS on the response (consumers read).
- **`SessionAppLifecycleResponse` gains `waited_ms`, `terminal_state`, `terminated_cooperatively`** (additive).
- **`DiagnosticDumpResponse` gains `alive_cache: AliveCacheCounters`** (always emitted, `wired: bool` sentinel) **and `session_counters: SessionLifecycleCounters`** (cumulative — survive close). Client-side `smix diagnostic dump` (non-JSON) renders both under new sections.
- **`App::clear_app_data_with_launch(args, env)` SDK method** — `clear_app_data()` becomes a thin wrapper. yaml `clearAppData: { launchArgs, launchEnv }` (with `args` / `env` short aliases) parses to `Step::ClearAppData { launch_args, launch_env }` and threads through.

### Runner-side (Swift)

- `SessionRoute.AliveCacheCounters.wired: Bool` sentinel; `SessionRoute.DiagnosticSnapshot.aliveCache` is non-Optional and always emitted; `SessionLifecycleCounters` embedded alongside.
- `SessionRoute.AppLifecycleRequest` decoder accepts `args`, `env`, `waitForForegroundMs`; falls back to pre-v1.0.11 shape (bare `sessionId`) so older clients still work.
- `launchApp` handler applies `entry.app.launchArguments = req.args` + `.launchEnvironment = req.env` before `.launch()`, then polls `.state` for `.runningForeground` up to `req.waitForForegroundMs` (250 ms cadence). Reports `waitedMs`, `terminalState`, `terminatedCooperatively` (always false on launch) in outcome.
- `terminateApp` handler observes `app.state == .notRunning` after `.terminate()` returns; sets `terminatedCooperatively` accordingly. `terminateAppViaXCUIApplication` counter advances when cooperative; `terminateAppViaFallback` advances when XCUIApplication timed out and fell back. `> 0 fallback` is the smoking-gun for the `.ips` diagnosis.
- Cumulative `LifecycleCounters` local (class + NSLock, no actor overhead for the sync mutations) advanced on every `open`, `close`, `relaunchApp`, `terminateApp`, `launchApp`; snapshotted into every diagnostic response.

### Wire compatibility

- All new fields carry `#[serde(default)]`. Pre-v1.0.11 clients see zero change.
- New request fields default to empty — a pre-v1.0.11 SDK still sends bare `{sessionId}` and gets pre-v1.0.11 launch semantics.
- New response fields default to zero — a pre-v1.0.11 SDK ignoring them keeps working.

### Documentation

- A standing note explaining what `AppAliveCache` protects, what `unknown` descendants mean, and how to distinguish "app dead + retry-spam broken by cache" from "app alive but a11y sparse" (the case observed on Expo SDK 57 dev-launcher).

### Ship gate

Real-sim observations (iOS 26.5) at v1.0.11:

```
POST /session/launch-app  {sessionId, args: ["-AppleLanguages","(en)"], env: {SMIX_TEST_ENV:"hello"}, waitForForegroundMs: 15000}
→ HTTP 200 {"ok":true, "wallMs":2786, "waitedMs":0, "terminalState":4, "terminatedCooperatively":false}

POST /session/terminate-app  {sessionId}
→ HTTP 200 {"ok":true, "wallMs":1050, "waitedMs":0, "terminalState":1, "terminatedCooperatively":true}

POST /diagnostic/dump
→ aliveCache: {wired: true, markAliveTotal: 1, ...}
→ sessionCounters: {openedTotal: 1, terminateAppTotal: 1, terminateAppViaXCUIApplication: 1, terminateAppViaFallback: 0, launchAppTotal: 1, launchAppReachedForeground: 1, launchAppTimedOutBeforeForeground: 0, ...}
```

`terminatedCooperatively: true` + `terminateAppViaFallback: 0` — the cooperative pathway went through cleanly on Preferences. Consumer real-app validation (Expo 57 dev-launcher bypass) happens downstream after upgrading the CLI + regenerating runner sources via the v1.0.10 auto-sync path.

667 workspace cargo tests + 6 Swift SmixRunnerCore tests + 3 new clearAppData parser tests green. Corpus gate infrastructure landed in v1.0.10.


## [1.0.10] — 2026-07-11

**Systemic fix for the CLI-vs-runner distribution drift that made v1.0.4–v1.0.9 patches silently no-op on stale on-disk runner sources.**

### Root cause (confirmed with hard evidence)

`cargo install smix` used to ship only the Rust binary; the Swift `SmixRunner.xcodeproj` + `Sources/SmixRunnerCore/` + `SmixRunnerUITests/` sources were obtained separately at consumer install time, never version-synced afterward. A consumer's on-disk `~/.local/share/smix/runner/SmixRunnerUITests/SmixRunnerUITests.swift` was 2212 lines with zero references to `sessionHandlers` / `/session/open` / `SessionHandlers` while the current repo file was 2669 lines (v1.0.9) with them present. That's why 6 consecutive CLI patches (v1.0.4–v1.0.9) shipped session lifecycle + observability + crash-dialog fixes but that consumer's runner stayed frozen at a pre-v1.0.3 revision — `/session/open` 404 100% of the time, `clearAppData` unusable, a11y-cache re-probe log line never emitted.

Secondary root cause: `GET /health` route always called `HealthRoute.response()` (legacy `{"ok":true}` since v0.x). The `runnerVersion` field CHANGELOG v1.0.2 claimed was never emitted, so version drift has been invisible client-side across every prior release.

### New crate — `smix-runner-sources`

- Ships the Swift runner project as a checked-in gzipped tarball baked into the `smix-cli` binary via `include_bytes!`.
- Regenerated by `scripts/release/build-runner-tarball.sh` (`gzip -n` reproducible).
- Excludes the 13 MB `SmixCoreFFI.xcframework` binary — that continues to be fetched separately.
- `SOURCES_VERSION = env!("CARGO_PKG_VERSION")` — matches the workspace and every ecosystem publish.

### CLI (Rust)

- **Runner project auto-sync.** `resolve_runner_project` (called by every `smix runner up`) now reads `~/.local/share/smix/runner/.smix-runner-version` before dispatching xcodebuild. On drift OR missing, the embedded tarball extracts in place, backing up any prior tree to `~/.local/share/smix/runner.bak-<ts>/`. Zero user migration on upgrade. First-run consumers get sources populated transparently.
- **`smix runner install [--force] [--path <dir>]` verb.** Explicit sync for troubleshooting, first-time-setup, or `--force` re-extract when the tree has been hand-edited. Idempotent when already current.
- **CLI forwards `TEST_RUNNER_SMIX_RUNNER_VERSION=<CARGO_PKG_VERSION>` env.** Xcode strips the `TEST_RUNNER_` prefix; runner reads `SMIX_RUNNER_VERSION` via `ProcessInfo`.
- **Client-side version-mismatch gate at `runner up`.** After `/health` returns 200, parses `runnerVersion` field; refuses boot with actionable message ("run `smix runner install --force`") on mismatch. Legacy-body runners (pre-v1.0.10) get a warning but no refusal so existing consumers aren't broken by the upgrade.
- **Subprocess-ring persistence.** `smix-simctl::set_subprocess_ring_persist_path` (called at CLI startup with `~/.local/share/smix/subprocess-ring.json`) writes-through every simctl invocation record atomically. The v1.0.7 empty-`diagnostic dump`-payload-after-supervisor-cycles gap is closed — the file survives cycles; post-mortem tools read the file, not in-memory state.

### Runner-side (Swift)

- **`/health` route wires `HealthRoute.responseDetail`.** Returns `{ok, runnerVersion, uptimeMs, lastRequestAtMs, sessionsOpen, activationsTotal}`. `runnerVersion` sourced from `SMIX_RUNNER_VERSION` env (fallback `"unknown"`).
- **`AppAliveCache` observability counters.** `markDeadTotal`, `markAliveTotal`, `suppressHitTotal`, `suppressMissTotal`, `reprobeAttemptedTotal`, `reprobeSucceededTotal`, `reprobeInvalidatedEarly`, `reprobeExhaustedWindow`. Every mutation on the actor advances the paired counter.
- **Re-probe path wired to counters.** The v1.0.9 background Task now calls `noteReprobeAttempted` at spawn, `noteReprobeSucceeded` on invalidate-alive, `noteReprobeInvalidatedEarly` when external `markAlive` beat the probe, `noteReprobeExhaustedWindow` on the 6-iteration exhaustion path. The grep-for-log-line problem is now a numeric check on `/diagnostic/dump` counter deltas.
- **`/diagnostic/dump` extended.** `DiagnosticSnapshot.aliveCache: AliveCacheCounters?` — `nil` when the runner opted out; JSON body omits the field to preserve wire compatibility.

### Wire

- `HealthResponse` fields were already declared in v1.0.2's wire crate but never populated — this release makes them non-zero.
- `DiagnosticSnapshot` gains optional `aliveCache` object; parsers ignoring unknown fields keep working.

### Infrastructure

- **`scripts/release/corpus-gate.sh`.** Runs every yaml under `SMIX_CORPUS_DIR` (defaults to a bootstrap-corpus fixture directory under `crates/smix-cli/tests/fixtures/`). Fails the release on any yaml failure. Dumps `smix diagnostic dump --json` on teardown into `.tmp/release-gate/<ts>/`.

### Tests

- `smix-runner-sources`: 7 tests (extract round-trip, version file write, xcframework-excluded regression guard, backup-on-force, refuse-on-non-empty, version-file read).
- `smix-cli::runner`: 3 auto-sync tests (extract on missing, re-extract on stale, no-op when current) + 1 env test (`TEST_RUNNER_SMIX_RUNNER_VERSION` set correctly).
- `smix-simctl::subprocess_ring`: 1 persist round-trip test simulating supervisor cycle.
- Swift `AppAliveCacheCountersTests`: 6 tests covering mutation counters + diagnostic JSON serialisation + null-cache omission.

### Ship-gate observations (real-sim, iOS 26.5)

Observations satisfying the RFC's real-sim gate:

1. `smix runner install` — extracted 303 files at v1.0.10, previous 2212-line SmixRunnerUITests.swift (pre-v1.0.3) → 2706-line v1.0.10; xcframework preserved from backup tree via the carry-over patch.
2. `GET /health` — `{"ok":true,"runnerVersion":"1.0.10","uptimeMs":16105,"lastRequestAtMs":0,"sessionsOpen":0,"activationsTotal":0}` — the field CHANGELOG v1.0.2 claimed but never emitted is now real.
3. `POST /session/open` (bundleId `com.apple.Preferences`, activate=false) — HTTP 200 + `{"sessionId":"6F7C4A73-…","activatedOnce":false,"serverTimeMs":1783746973931}`. **The chronic 404 that spanned v1.0.4-v1.0.9 is permanently closed.**
4. `POST /diagnostic/dump` — `aliveCache:{"markDeadTotal":0,"markAliveTotal":1,…}` — counters wire end-to-end (markAliveTotal:1 came from the /session/open handler's `cache.markAlive` per D2 §"successful open re-establishes the target").

The consumer app was not installed on the validation sim (unrelated to smix), so the corpus gate remains for a follow-up validation with a real consumer app installed. The systemic fix itself — the CLI-vs-runner drift closure — was observed working on real sim before publish.


## [1.0.9] — 2026-07-11

App-alive cache adaptive re-probe + supervisor RunnerCycled log context. Closes the two named v1.0.8 deferrals.

### Runner-side (Swift)

- **App-alive cache adaptive re-probe.** When an XCTIssue "Application X is not running" is observed, the cache still marks the bundle dead for 20 s. Now the runner spawns a background `Task` that polls `XCUIApplication.state` every 3 s during the window; on the first observation of a non-`.notRunning` state, calls `markAlive` immediately + emits `smix-runner: app-alive cache re-probe hit <bundle> state=<n>; early invalidate` on stderr. Fixes a reported failure mode where slow-bootstrap apps sat blocked for the full 20 s while they were actually alive again.
- Bounded to 6 iterations (18 s) — matches the cache window minus one probe interval for slack. If the app is still `.notRunning` after 6 probes the cache expires naturally.

### CLI (Rust)

- **Supervisor `RunnerCycled` event with log context.** The JSON emitted on every cycle now carries a `context` field with ±5 lines around the matched trigger:
  ```json
  {"event":"RunnerCycled","reasonMatched":"** TEST INTERRUPTED **","context":["2026-07-11 …", "…"],"atMs":1720689124321}
  ```
  Consumers get cycle-cascade classification data without needing a separate `grep` pass on the runner log. Best-effort — if the log rotated between the match and the read the `context` array comes back empty.

### Wire + ABI compatibility

- No wire changes.
- No SDK ABI changes.
- Runner behaviour change is invisible to consumers not observing stderr.
- Supervisor JSON gains a new optional `context` field; parsers ignoring unknown fields keep working.

### Deferred (still)

- **`launchApp: clearState: true` deprecation + auto-expand** — waiting on downstream corpora to migrate to `clearAppData` first. Once migration is confirmed, v1.0.10 will emit the WARN + auto-expand.



Eliminate the "app quit unexpectedly" ReportCrash system dialog that fired during in-place data clears.

### Root cause revisited

v1.0.4 replaced `simctl uninstall + install` with an in-place clear (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`). Dogfooding reported the dialog STILL fired. Diagnosis: even without the uninstall, `simctl terminate` sends SIGKILL to the target, which `com.apple.ReportCrash` on iOS 26.5 sim treats as a crash. The whole `simctl` termination pathway is what triggers the dialog — not just the uninstall.

The systemic answer: move termination + launch INSIDE the XCUITest runner process via `XCUIApplication.terminate()` / `.launch()` (cooperative via `testmanagerd`; does NOT signal ReportCrash). The sandbox wipe stays on the host via `SimctlClient::clear_app_sandbox` but ONLY after the cooperative terminate, so ReportCrash was never signalled.

### Runner-side (Swift)

- **`POST /session/terminate-app { sessionId }`** → cooperative `XCUIApplication.terminate()` on the session's cached binding. testmanagerd stop; no SIGKILL; no ReportCrash signal.
- **`POST /session/launch-app { sessionId }`** → cooperative `XCUIApplication.launch()`. Fresh instance sees whatever sandbox state the SDK left for it.
- Both are additive routes; v1.0.7 runners return 404 and consumers should either upgrade the runner or route through the legacy `Session::relaunch_app`.

### CLI + adapter (Rust)

- **New yaml verb `clearAppData`** — session-scoped in-place data clear. Bare verb, no args. Maps to `App::clear_app_data` which orchestrates the 3 steps host-side. Requires an open session (auto-populated by `smix run`).
- **`App::clear_app_data() → Result<wall_ms>`** on the Rust SDK. Grabs `session_id` + `bundle_id` from the driver + `udid` from `App::require_udid`; calls `runner.terminate_session_app` → `simctl.clear_app_sandbox` → `runner.launch_session_app`.
- **`Session::reset_app_data()`** — thin ergonomic wrapper on `App::clear_app_data`, for consumers who hold a `Session` handle directly.
- **`launchApp: clearState: true` NOT yet deprecated** in this cycle — legacy shape still runs the pre-v1.0.8 `LaunchFreshOp` sequence. Consumers migrating to `clearAppData` get the crash-dialog fix; consumers who keep the legacy shape stay unaffected until v1.0.9 flips the default.

### Wire additions

- `SessionAppLifecycleRequest` / `SessionAppLifecycleResponse` in `smix-runner-wire`.
- `HttpRunnerClient::terminate_session_app(req)` / `launch_session_app(req)` on the Rust client.

### Deferred to v1.0.9

- **Adaptive app-alive cache re-probe** (parked because the crash-dialog fix is enough to unblock downstream gates and the a11y-cache work has its own testing surface).
- **Supervisor `RunnerCycled` reason with log context.**
- **Deprecation of `launchApp: clearState: true`** — emit WARN + auto-expand to `clearAppData + launchApp: {}`. Deferred because the deprecation needs a full-corpus consumer migration, and consumers should migrate their subflows first on their own timeline.

### Wire + ABI compatibility

- Additive routes; v1.0.7 runners return 404 on the new endpoints.
- Additive `Step::ClearAppData` variant on the yaml Step enum; `#[non_exhaustive]` was already in play (via yaml deserialization), so consumers using pattern matching are unaffected.



Systemic observability + subprocess integrity. Three reported symptoms shared one root cause: smix was opaque about its own runtime.

### Subprocess integrity

- **`SimctlClient::clear_app_sandbox` uses `/bin/rm`** (not `"rm"`). `xcrun simctl spawn <UDID> <cmd>` uses `posix_spawn` inside the sim; PATH resolution is NOT run, so a bare command name fails `NSPOSIXErrorDomain code 2: No such file or directory` on iOS 17+ sims. This was the direct root cause of a reported ENOENT failure on `launchApp: clearState: true` mid-flow. `current_locale` + `set_locale` similarly use `/usr/bin/defaults`.
- **`SimctlError::NonZeroExit` extended with `argv: Vec<String>` + `wall_ms: u64`**. Display impl now surfaces every arg simctl was asked to run — `xcrun simctl spawn <UDID> /bin/rm -rf /Users/.../Documents ... exited 2 (312ms): ...` — instead of just the subcommand name. Consumers reading the error know exactly what smix asked simctl to do.
- `SimctlError` marked `#[non_exhaustive]`; `SimctlError::non_zero_exit(sub, code, stderr)` helper for callers translating foreign errors.

### Observability surface

- **Ring buffer of recent `simctl` invocations** (capped 128; oldest evicted). Public accessor `smix_simctl::recent_subprocesses() -> Vec<SubprocessRecord>` — `argv`, `exit_code`, `wall_ms`, `stderr_head` (first 256 bytes), `timestamp`.
- **`POST /diagnostic/dump`** runner-side route — snapshot of `{ sessions, simHealth, supervisorPid, uptimeMs, recentSubprocesses }`.
- **`smix diagnostic dump [--json]`** CLI verb — calls `/diagnostic/dump` on the runner, merges with the client-side ring, pretty-prints a runtime post-mortem view. `--json` for CI consumption. Legacy runners (v1.0.6-) return 404; CLI degrades gracefully to client-side ring only.
- `HttpRunnerClient::diagnostic_dump()` Rust client method.

### Streaming discipline

- **`smix runner supervise` flushes stdout after every `RunnerCycled` JSON event**. Supervisor events now reach the consumer's parser even when the outer flow crashes fast right after a cycle.

### Cold-rebuild progress banner

- **`smix runner up` prints an explicit cold vs warm banner**. Detects warm by checking `.smix/runner/derived-data-<UDID>/` presence + populated. Cold path prints `COLD REBUILD expected up to 10 minutes` and emits a `xcodebuild still working (Ns elapsed)` heartbeat every 30 s. Warm path prints `warm rebuild ~3 s expected`. Fixes the reported case where a consumer harness's `spawnSync` timeout (300 s) tripped during cold recompile after a version bump with no visible progress signal.

### Related regression fix

- `smix-sdk/tests/launch_fresh_plan.rs` was pre-v1.0.4; asserted `Uninstall+Install` on the default clear_state path. v1.0.4 flipped the default to in-place (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`); tests updated to match shipping behaviour. Force-reinstall path exercised via `plan_launch_fresh_calls_v2(true)`.

### Wire + ABI compatibility

- All wire additions additive. `POST /diagnostic/dump` on runners < v1.0.7 returns 404; CLI degrades gracefully.
- `SimctlError` is `#[non_exhaustive]`; construction sites updated to fill new fields via `non_zero_exit` helper.



Sidecar supervise + symmetric down-cascade + rust 1.97 baseline. Follow-up to v1.0.5 folding the supervisor's spawn-and-teardown into the runner lifecycle so consumers who want automatic `TEST INTERRUPTED` recovery just add `--supervise` to their existing `smix runner up`.

### CLI (Rust)

- **`smix runner up --supervise`** — after `/health` returns 200, spawn a detached `smix runner supervise` process, redirect stdout/stderr to `.smix/runner/supervise-<UDID>.log`, and record its pid in `state.json` under a new `supervisorPid` field. Sidecar runs in its own process group so a ctrl-C on the CLI doesn't tear it down.
- **`smix runner down` cascades supervisor teardown.** Before the xcodebuild SIGINT, `down` reads `state.json` and if a `supervisorPid` is present + still matches a `smix runner supervise` process, sends SIGTERM (5 s), escalates to SIGKILL if needed. `down` invoked from inside the supervisor itself (re-entrant case, during auto-cycle) skips the self-kill.
- **`smix runner cycle` preserves the sidecar flag.** If the pre-cycle `state.json` records a supervisor, the post-cycle `up` re-attaches one. Consumers who ran `up --supervise` get supervision back automatically after a cycle.

### Runner state schema (backward-compatible)

- `state.json` gains optional `supervisorPid: u32` field via `#[serde(default)]`. State files written by v1.0.5 or earlier deserialize without change.

### Workspace hygiene

- `rust-version = "1.97"` in the workspace `Cargo.toml`. Baseline bump for the `if let` chain stabilizations + std ergonomics. Consumers on `cargo install` see no change (prebuilt binary); consumers building from source now need rustc 1.97+.

### Documentation

- CHANGELOG format going forward groups entries under `### CLI (Rust)`, `### Runner-side (Swift)`, `### SDK — all four`, `### Documentation`, `### Deferred`. First entry using the new pattern; retroactive edit of v1.0.4/v1.0.5 not required.

### Deferred (v1.0.7+)

- **Opportunistic 1.97 idiom cleanups.** A handful of nested `if let` sites collapse under 1.97's chain stabilizations. Not a functional change; queued as a hygiene sweep for a slow release cycle.

### Wire + ABI compatibility

- No wire additions.
- No SDK ABI additions.
- CLI additions are opt-in via `--supervise`; the classic path is unchanged.



Session persistence across XCTest lifecycle, host-side XCTest supervisor daemon, runner idle-close sweep, and the release smoke gate script. Closes the three v1.0.4 deferrals + the "shipped on build-green only" gap.

### Added — session persistence

- **`POST /session/list`** → `{sessions: [{sessionId, bundleId, openedAtMs, lastActivatedAtMs}]}`. Rust: `HttpRunnerClient::list_sessions()`. CLI: `smix runner list-sessions` (pretty-printed table).
- **`Session::still_valid()` on all 4 SDKs** — probes `/session/list` and returns `true` iff the runner still knows this session id. Consumers wire it after a `Session::state` transition to `Cycling` or `Dead` to decide whether to keep using the session (§D1 preserves them across cycles) or reopen.
- **Runner-side persistence** — session table serializes to `~/Documents/smix-sessions.json` inside the sim on every mutation via `Data.write(.atomic)` (atomic-rename write). Boot rehydrates whatever's there, rebuilding each `XCUIApplication(bundleIdentifier:)` fresh (no `.activate()` call — the client's next request drives that). `smix runner cycle` preserves the file, so consumer `Session-Id` survives the cycle transparently.

### Added — supervisor daemon

- **`smix runner supervise [--runner-project <path>]`** — foreground process that tails `.smix/runner/runner-<UDID>.log`, matches interrupt patterns (`** TEST INTERRUPTED **`, `SchemeActionResultOperation started unexpectedly`), and auto-invokes `runner::cycle()` on hit. Backoff: 60 s per-cycle cooldown. Circuit breaker: 5 cycles in 10 minutes → exit non-zero so a monitoring layer can escalate. Emits `{"event":"RunnerCycled","reasonMatched":"...","atMs":N}` JSON on stdout per cycle.

### Added — idle-close sweep

- **Runner-side session idle-close** — `SessionEntry` gains `lastAccessedAt`; `resolveApp()` refreshes it on every `Session-Id` hit. Detached `Task.detached` in `test_runForever` reaps sessions whose `lastAccessedAt` is older than 60 s every 15 s. Half-orphaned client sessions (SIGKILL wipes client without close) vanish within 60-75 s instead of accumulating until runner restart. Emits a stderr line on non-zero reap for operator visibility.

### Added — release smoke gate + ship script

- **`scripts/release/smoke-v1.smoke.sh` + `.smoke.yaml`** — real-sim gate exercising every net-new v1.0.4/v1.0.5 code path: pacer floor (`takeScreenshot × 10`), `--debug-output` `fail.tree.json` emit on a deliberate `assertVisible` fail, `runner cycle` + `/session/list` persistence, supervisor 5 s alive check. Requires jq + a booted sim.
- **`scripts/release/ship.sh <version> [--i-know-what-im-doing]`** — DAG-ordered 4-ecosystem publisher, refuses to run unless the smoke gate has passed in the last hour. Bypass flag is an audit-visible knob, not a silent default.

### Wire + ABI compatibility

- All additions are additive (routes, response fields, CLI verbs).
- v1.0.5 clients work against v1.0.4 runners (missing `/session/list` → 404; SDK `Session::still_valid()` propagates the error and consumers treat as invalid).
- v1.0.4 clients keep working against v1.0.5 runners.



Studio protection + gate-hardening. Motivation: a downstream gate loop running against a v1.0.3 runner triggered a `SimRenderServer` `brk 1` assertion inside the `com.apple.display.captureservice` dispatch queue, cascading into shutdown_stall and forced macOS restarts. This release closes the full gate-hardening feedback plus the SimRenderServer stress fix, plus lifecycle-safe-exit primitives.

### Added — sense layer

- **`smix-sim-health` — new stone crate.** Watches SimRenderServer + xcodebuild pids + `/health` age + rolling screenshot wall times. State machine `Healthy | Degraded | Dead`; transitions broadcast on a `tokio::sync::broadcast` channel. Business-unaware; SDK-facing state is exposed via `Session::state` (below), driver-side auto-cycle policies live per driver.
- **`HttpRunnerClient::with_sim_health(monitor)`** — `/health` outcomes feed `SimHealthMonitor::record_health_ok`/`record_health_fail`. `HttpRunnerClient::sim_health()` accessor.

### Added — act layer

- **`smix-simctl` screenshot pacer.** Adaptive interval floor: 100 ms in the fast path (recent wall < 800 ms), 1500 ms in the slow path (recent wall ≥ 800 ms). Circuit breaker: any recent wall ≥ 1500 ms or any failure trips a 3 s hold that surfaces the new typed error `SimctlError::CaptureBackpressure { retry_after }`. Consumers whose gates already screenshot at ≥ 200 ms cadence are unaffected; tight loops slow to the pacer floor. This is the direct fix for the `SimRenderServer` `brk 1` triggering pattern on iOS 26.5.2 (25F84).
- **`SimctlClient::with_screenshot_pacer(cfg)`** builder + **`SimctlClient::with_sim_health(monitor)`** builder — wire the pacer's observations back to the sim-health monitor for global state classification.

### Added — CLI

- **`smix runner cycle`** — new verb. Reads the current runner state, tears down (SIGINT + wait, preserves per-udid `derived-data-<udid>/`), brings up on the same device + port + bundle. Warm re-up in ~3 s vs cold ~15 s. Errors if no `state.json` exists (`runner up` for a cold start).
- **`smix runner up` bundle validation** — refuses to boot without `--bundle`, prints a clear error + example. `SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1` bypasses the guard (opts back into the legacy Preferences default with an explicit warning). With `--bundle` set, logs `[runner] target bundle-id: <id>` at boot.
- **`smix run --gate-signal <regex>` + `--gate-signal-timeout <ms>`** — prepends an implicit `expect.signal { regex, timeoutMs }` step at the START of the flow (index 0), blocking until the regex is observed in the metro log tail. Requires `--metro-log-url` also set. Symmetric to the existing `--await-signal` end-of-flow gate. Default timeout 60 s; zero disables. Replaces the node-side wait-for-metro-signal helper consumers had to write.

### Added — debug output

- **`--debug-output <dir>/step-<N>-<verb>.tree.json`** — on step failure, alongside the fail PNG the adapter now writes a full a11y-tree snapshot captured at the moment the step's expectation was evaluated. Turns "screenshot shows the text but assertVisible failed" mysteries into "here's exactly what the runner saw."
- **`run-summary.json` per-step trace** — the summary now carries `steps: [{n, verb, verdict, wallMs, jsonPath, pngPath?, treePath?, failureKind?, failureMessage?}]`. Populated for both success and failure runs (partial trace on failure preserved via a snapshot taken before the `?`-return early-exit).

### Added — session lifecycle

- **`POST /session/close-all`** — closes every open session on the runner. Idempotent (`{ok, closed:N}`). Rust: `HttpRunnerClient::close_all_sessions()`.
- **`POST /session/relaunch-app {sessionId}`** — runner does `terminate() + launch()` on the session's cached `XCUIApplication` binding IN PLACE, preserving the session id and XCUITest binding. Returns `{ok, wallMs}`. Recovers from a downstream app crash without cycling the runner. Rust: `HttpRunnerClient::relaunch_session_app(&req)`; SDK: `Session::relaunch_app()` (Rust), `session.relaunchApp()` (TS / Swift / Kotlin).
- **`Session::state` + state stream/flow/event across all 4 SDKs.** The runner emits `X-Sim-Health: healthy|degraded|cycling|dead` on every response; SDKs parse it and surface transitions to consumers:
  - Rust — `Session::state() -> SessionState`.
  - TypeScript — `session.state` + `session.on('state', listener)`.
  - Swift — `session.state` + `session.stateStream: AsyncStream<SessionState>`.
  - Kotlin — `session.state` + `session.stateFlow: StateFlow<SessionState>`.

### Added — extended health

- **Extended `GET /health` body** now includes `simRenderServer: {alive, pid}` and `xcodebuildTestHost: {alive, pid, restartCount}`. Legacy clients that only read `{ok:true}` continue to work.

### Added — safe-exit cascade

- **`smix run` SIGINT / SIGTERM handling.** `tokio::signal::ctrl_c()` and SIGTERM race against the flow execution; on signal the CLI aborts the in-flight flow, runs a best-effort `/session/close` under a 2 s timeout, prints `interrupted (SIGINT|SIGTERM) — running session-close cascade` on stderr, and exits with POSIX-conventional 130 (SIGINT) / 143 (SIGTERM). The Rust adapter's `--debug-output` partial-trace file still fires on interrupt so the last-attempted step is captured. Solves the "ctrl-C leaves a session hanging until runner idle-close fires" complaint.

### Fixed — `openLink` URL preservation

- **`SimctlClient::open_url` argv preservation** — verified byte-identical URL passthrough (`openurl_argv` test helper + 3 unit tests covering percent-encoded schemes, query params with `&`/`#`, unicode). The dev-launcher picker behavior reported on `expo-dev-client 57.0.5` is upstream (not smix); the finding lives on expo-dev-client's side and is documented for the record.

### Documented — `--activate` per-request cost auto-resolution

- **`--activate` per-request cost** is auto-resolved for consumers who upgrade to v1.0.3 sessions (via `smix run` auto-session or explicit `Session.open`). The runner short-circuits `App-Activate: true` when a `Session-Id` header is present, so the 50-100 ms per-request main-actor hop described in earlier feedback no longer applies for session-mode flows. No code change needed; documented here so consumers know to prefer `--activate` inside a session rather than passing it per-request.

### Wire + ABI compatibility

- All additions are additive (routes, response fields, enum variants, SDK types).
- v1.0.4 clients work against v1.0.3 runners (missing routes → 404 → fall through; missing headers → `Session::state` stays `Healthy`).
- v1.0.3 clients work against v1.0.4 runners (extra fields / headers ignored).

### Verified builds

- Rust workspace (26 crates): fresh `cargo check --workspace --jobs 1` clean 3m06s.
- Swift Package: `swift build` clean; `xcodebuild build-for-testing -project SmixRunner.xcodeproj -scheme SmixRunner -destination 'generic/platform=iOS Simulator'` — `** TEST BUILD SUCCEEDED **`.
- Kotlin: `./gradlew :sdk:build` — BUILD SUCCESSFUL in 28s.
- TypeScript: `tsc --noEmit` clean.

### Deferred to v1.0.5 (independent charters)

- **Session-persistence across XCTest lifecycle.** Needs a separate design for state serialization.
- **Host-side XCTest supervisor** — auto-cycle-on-`TEST INTERRUPTED`. v1.0.4 provides the manual escape hatch (`smix runner cycle` verb) plus the programmatic detection surface (`Session::state` transitions via `X-Sim-Health` + `AppAliveCache` markDead from parsed XCTIssues); a fully-automatic supervisor daemon is v1.0.5 material.
- **Runner-side idle-close 120 s → 60 s tightening** — deferred; the client-side `smix run` SIGINT / SIGTERM cascade already covers the primary orphaned-session case.



Session lifecycle at the runner boundary. Building on v1.0.2's rate-limited activation, v1.0.3 lets consumers open a session at the start of a flow, run the entire flow against a cached `XCUIApplication` binding, and close on exit — no per-request activation. This is the systemic fix that supersedes the interim rate-limit; the legacy per-request path stays as a fallback.

### Added

- **Session routes on the iOS runner** — `POST /session/open {bundleId, activate}` returns `{sessionId, activatedOnce, serverTimeMs}`; `POST /session/close {sessionId}` (idempotent) returns `{ok}`; `POST /session/renew-activation {sessionId}` returns `{ok, activated}` subject to a 2 s per-session rate limit. Wire types available on `smix-runner-wire` since v1.0.2; runner-side handlers land in v1.0.3.
- **`Session-Id` header** on every runner request. When present, `resolveApp()` short-circuits to the session's cached binding — no per-request activation regardless of `App-Activate`.
- **Rust SDK `Session`** — `App::open_session(bundle_id, activate) -> Session`. Consumer flow: `let session = app.open_session("com.example.app", true).await?; session.app().tap(...).await?; session.close().await?;`. `Session::renew_activation()` for consumer-driven drift recovery.
- **TypeScript SDK `Session`** — `Session.open(runner, "com.example.app", { activate: true })` on any `HttpRunnerClient`-shaped runtime. Consumers pair with `try / finally { await session.close() }`.
- **Swift SDK `HttpSmixSimRuntime` + `Session`** — URLSession-backed `SmixSimRuntime` implementation speaking the SmixRunnerCore wire directly, with session-aware header attachment. `Session.open(runtime, activate: true)` acquires a session; `session.close()` releases. Every request from the runtime while the session is open carries `Session-Id`.
- **Kotlin SDK `HttpSmixSimRuntime` + `Session`** — java.net.HttpURLConnection-backed runtime (no additional dependencies beyond the existing kotlinx-serialization-json), same wire contract. `Session.open(runtime, activate = true)` / `session.close()`. Thread-safe on the session-id field via `AtomicReference`.
- **`smix run` opens a session automatically** — every CLI invocation opens a session at start, closes on exit. Runners that don't implement `/session/open` (v1.0.x pre-1.0.3) return non-2xx; the CLI emits a WARN and falls through to the legacy per-request path (rate-limited since v1.0.2, so still safe).

### Wire + ABI compatibility

- All new routes are additive
- All new SDK types are additive (`Session`, `SessionOpenRequest`, etc.)
- v1.0.x clients keep working against v1.0.3 runners (Session-Id header optional)
- v1.0.3 clients work against v1.0.2 runners with a WARN + legacy fallback

## [1.0.2] — 2026-07-09

### Fixed

- **Runner activation storm** — the XCUITest-side `resolveApp()` no longer calls `.activate()` on every request when `App-Activate: true` is set. Instead, `.activate()` runs at most once per bundle-id per 5 s. Long-running gates (visual / perf regression, ~340 s of continuous requests against the runner) previously accumulated ~1000+ activate calls, exhausting XCTest process arbitration on iOS 26.5+ and crashing `test_runForever()` mid-run. Recovery semantics preserved: after 5 s of silence a subsequent activate hint is honored, so a foreground steal by SpringBoard is auto-recovered within the same window.
- **Simulator screenshot PNG colorspace metadata** — `xcrun simctl io <udid> screenshot` on iOS 26.5 sub-builds started omitting the `sRGB` ancillary chunk from its PNG output. macOS Preview.app and other viewers fall back to Display P3 in the absence of an embedded ICC profile, over-saturating red and adding yellow anti-alias fringing on text. `SimctlClient::screenshot` now byte-splices a synthesized `sRGB` chunk (rendering intent = 0, perceptual) into the PNG stream immediately before the first IDAT when none is present. IDAT bytes are never decoded or modified — pixel-comparison consumers (dhash, hamming) see byte-identical decoded pixel arrays.

### Added

- **Runner liveness observability** (Rust client) — `HttpRunnerClient::with_liveness_window(N)` opts in to rolling-window request outcome tracking. If a majority of the last N requests failed, subsequent calls surface `RunnerTransportError::RunnerDegraded { window, non_success_recent, last_endpoint, last_error }` instead of returning silent stale bodies. Any transport-level `is_connect()` error additionally probes `/health` with a 1 s timeout; if the runner is unreachable, subsequent calls surface `RunnerTransportError::RunnerDied { last_seen_ms, last_error }`.
- **Extended `GET /health` body** — the runner-side JSON response now includes `runnerVersion`, `uptimeMs`, `lastRequestAtMs`, `sessionsOpen`, and `activationsTotal`. Legacy clients that jq-parse `{"ok":true}` continue to work — the extended body is a superset. The Rust client's `HttpRunnerClient::health_detail()` parses the new fields.
- **Wire types for session lifecycle** — `smix-runner-wire` exports `SessionOpenRequest / SessionOpenResponse / SessionCloseRequest / SessionCloseResponse / SessionRenewActivationRequest / SessionRenewActivationResponse`. The Rust client (`HttpRunnerClient::open_session`, `close_session`, `renew_session_activation`) can drive these when a runner implements the endpoints; the corresponding runner-side routes are queued for v1.0.3.

## [1.0.1] — 2026-07-09

### Fixed

- **Parser** — `smix run` now accepts the `expect: { visible: <selector>, timeoutMs?: N }` and `expect: { notVisible: <selector>, timeoutMs?: N }` shapes emitted by `smix migrate` for `extendedWaitUntil`. The `expect: { visible: ... }` shorthand (no timeout, equivalent to `assertVisible`) is likewise accepted. Previously the parser only recognized the top-level `expect: { text | id: ... }` maestro-alias form, so codemodded corpora failed at run time with `expected 'text' or 'id' key`. Regression tests in `smix-adapter-maestro/tests/parser.rs` pin every accepted shape.
- **`smix migrate --help`** — help text corrected to state that comments, copyright headers, and blank lines survive the codemod byte-identical (matches 1.0.0's actual behavior).

### Added

- **`smix run --check`** — parse-only pre-flight. Reads every listed flow YAML and reports parse or include errors without connecting to a runner or booting a simulator. Exit 0 on clean parse across every flow; non-zero (2) on any error. Suitable for CI without simulator infrastructure.

## [1.0.0] — 2026-07-08

First public release.

### Added

- **CLI** — `smix` binary with subcommands `run`, `sim`, `runner`, `migrate`, `annotate`, `authoring`, `tree`, `find`, `tap`, `fill`, `clear`, `scroll`, `screenshot`, `describe`, `doctor`.
- **Rust SDK** — `smix-sdk` crate exposing the `App`, `Selector`, `KeyName`, and `Runtime` types plus a fluent builder for connection configuration.
- **TypeScript SDK** — `@goliapkg/smix` on npm; Playwright-shape API surface mirrored to the Rust SDK.
- **Swift SDK** — Swift Package published as a GitHub Release; provides a prebuilt `SmixCoreFFI.xcframework`.
- **Kotlin SDK** — `jp.golia.smix:smix-sdk` on Maven Central; UiAutomator-backed runner for the Android Emulator.
- **YAML runtime** — Maestro-compatible YAML syntax accepted directly (both maestro-canonical `tapOn` and smix-canonical `tap` forms).
- **Codemod** — `smix migrate` rewrites YAML from maestro-canonical to smix-canonical while preserving comments, copyright headers, and blank lines byte-identical.
- **Fixture registry** — `--fixture-registry <file.ts|file.json>` enables the `- fixture: <id>` YAML verb.
- **Metro log signals** — `expect.signal`, `expect.signals`, `expectLogClean`, and the `--metro-log-url ws:// | file:// | -` transport with configurable allowlists.
- **Annotation** — bundled Inter Regular and Noto Sans SC fonts; the `takeScreenshot` verb accepts `annotate:` clauses composing `circle`, `line`, `arrow`, `text`, and `box` primitives; `smix annotate` standalone CLI.
- **Auto-annotate on failure** — `--debug-output` fail-step PNGs receive an automatic red circle, step label, and summary; opt out with `--no-fail-annotate`.
- **JUnit output** — `smix run --format junit --output report.xml` writes a JUnit-XML testsuite consumable by common CI pipelines.
- **Authoring tier** — `smix authoring suggest`, `capture-tree`, `diff-tree`, and `record` for authoring flows against a live simulator or emulator.
- **Standard subflows** — bundled `std/wipe-app-state.yaml`, `std/wait-metro-bundle.yaml`, `std/quit-qa-mode.yaml`, `std/dismiss-open-in.yaml`, and `std/ensure-locale.yaml`.
- **MCP server** — `smix mcp` subcommand exposes the SDK surface to Claude Code and other MCP-aware clients.

### Stability commitments

- Wire format frozen — any breaking wire change is a v2.0 release.
- ABI frozen for the ten core "stone" crates (`smix-error`, `smix-selector`, `smix-screen`, `smix-runner-wire`, `smix-input`, `smix-verbs`, `smix-metro-log`, `smix-fixture`, `smix-annotate`, `smix-migrate`) — additive changes only within v1.x.
- All CLI flags shipped in v1.0 remain accepted for the v1.x lifetime.
- The YAML verb table (`smix-verbs`) is the single source of truth; removing a verb is a major-version change.

See [`docs/ai-guide/wire-format.md`](./docs/ai-guide/wire-format.md) and [`docs/ai-guide/abi-stability.md`](./docs/ai-guide/abi-stability.md) for the full contracts.
