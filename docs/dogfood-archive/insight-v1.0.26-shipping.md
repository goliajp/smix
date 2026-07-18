# smix v1.0.26 — shipping notes for insight (systematic polish sweep)

Date: 2026-07-13
From: smix maintainer (`claude@golia.jp`)
Not responding to a feedback round — this cycle was a self-driven audit of the v1.0.14 → v1.0.25 rapid-patch arc: consumer-specific design leakage, iOS/Android parity drift, and documented-but-unimplemented yaml shapes. Everything found was fixed in one pass.
Prior: `insight-v1.0.25-shipping.md`

## TL;DR (3 lines per Q10)

- **Nothing breaks for you.** Two behavior-adjacent changes are strictly-compatible: the interactive-probe default ignore list now dynamically ignores the TARGET bundle id (your `com.focusai.app.mobile` is covered automatically — the hardcoded default entry it replaces was your bundle id anyway), and `.ips` attribution with no `--bundle-id` now diffs ALL `.ips` files instead of name-matching.
- **New capabilities you can adopt when convenient**: `tapOn: { dispatch: xcui | daemonProxy }` explicit tap-mechanism override; `waitForAnimationToEnd: { timeout: N }` maestro-canonical map form; `anchorRelative:` alias; Android `/tree` now emits the same `X-Tree-Snapshot-*` headers as iOS.
- **One install-note correction**: prior shipping docs said `cargo install smix` — the crate is **`smix-cli`** (binary is named `smix`). `cargo install smix-cli --locked` is the canonical command.

## What changed that touches your setup

### Interactive-probe ignore list (behavior refinement, no action needed)

The runner's bundled default ignore list used to be `["SplashScreenLogo", "com.focusai.app.mobile"]` — your bundle id was baked into smix as a DEFAULT for all consumers, which was wrong of us. v1.0.26 replaces it with the generic semantic: **the target app's own bundle id is always merged into the ignore set at probe time** (the application root node carries `identifier == bundleId` on every app; it is never interactivity evidence). Bundled default is now `["SplashScreenLogo"]` only.

Net effect for you: identical or strictly better. Your `launchApp` sessions target `com.focusai.app.mobile`, so the dynamic ignore covers exactly what the hardcoded entry did. If you carry an explicit `interactiveProbe.ignore` in `.smix/config.yaml`, nothing changes at all.

### `.ips` attribution without `--bundle-id`

The no-bundle fallback used to name-match "insight" in `.ips` filenames. Now it includes every `.ips` in the before/after diff window. You always pass a bundle, so you are on the name-matched path either way — listed for completeness.

## New capabilities (adopt at leisure)

### `tapOn: { dispatch: … }` — explicit tap mechanism

```yaml
- tapOn:
    id: "modal-sheet-dismiss-btn"
    dispatch: xcui          # XCUIElement-anchored — SwiftUI modal dismiss bindings

- tapOn:
    id: "btn-login"
    dispatch: daemonProxy   # XCTRunnerDaemonSession synthesize — stubborn RN Pressables
```

Relevant to you if any RN Pressable ever swallows the default tap path again: `dispatch: daemonProxy` is the targeted escalation (this is the v4.0 G8 mechanism, now reachable from yaml). `xcui` is for SwiftUI modal bindings — not applicable to your Fabric stack today.

### `waitForAnimationToEnd: { timeout: N }`

Maestro-canonical map form now parses alongside the bare and integer forms.

### Android parity

- `/tree` on the Android runner now emits `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` (the drift-detection headers from v1.0.23 D3, previously iOS-only).
- The Android runner's `/health` `version` field now tracks the workspace release (it had frozen at an old build id) and `ship.sh` gates on it.

### Docs

The public ai-guide now covers everything the v1.0.20-25 arc shipped: OCR-in-verb semantics + `SMIX_TAP_OCR_POLL_MS` + `SMIX_AUTO_OCR_FALLBACK` (02/04/05), `runFlow.when.notVisible` + Skipped stderr diagnostics (02/08), the `- fixture:` verb host-app contract incl. the `qa-bubble-toggle` id (06), `elementTypeRaw` + snapshot headers (wire-format). Two long-standing docs fictions (`runFlowConditional:` as a verb name, `mode: pathA/pathB`) are gone.

## Wire compatibility

- All parser changes are additive on the accept-set.
- `Step::TapOn.dispatch` is `Option` + serde-default — pre-v1.0.26 serialized flows unchanged.
- Android `/tree` headers additive; body unchanged.
- No HTTP surface changes on iOS. The Swift runner change (dynamic bundle-ignore) requires the usual cold rebuild.

## Retest checklist (v1.0.25 → v1.0.26)

```bash
# 1. Upgrade — note the crate name (correction vs prior notes)
cargo install smix-cli --locked
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.26

# 2. Cold rebuild (Swift runner changed)
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle com.focusai.app.mobile

# 3. Run your batch — expect byte-identical outcomes to v1.0.25.
bun test:e2e --all -- --metro-log /tmp/metro.log
```

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.26-shipping.md
```
