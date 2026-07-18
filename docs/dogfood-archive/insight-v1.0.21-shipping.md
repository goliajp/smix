# smix v1.0.21 — shipping notes for insight (iOS 26.5 alert-button role mapping)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12-v1.0.19-flow-progress.md` — v1.0.20 quick retest addendum
Prior: `insight-v1.0.20-shipping.md`

## TL;DR (3 lines per Q10)

- **Root cause confirmed at the perception layer, fix landed at TreeRoute.** iOS 26.5 XCUITest exposes `UIAlertController` action buttons as `.other` (rawValue 1) or `.staticText` (48), not `.button` (9). Fixed by ancestor-scoped `rawType` promotion inside `.alert` / `.dialog` / `.sheet` subtrees — labeled `.other` / `.staticText` descendants get emitted as `"button"` on the tree JSON wire.
- **Cross-iOS-version `role: button` semantics preserved without per-consumer patches.** No yaml changes needed on your side; the same `tapOn: { role: button, name: 'Reload' }` that regressed on v1.0.20 will match again.
- **7 new Swift unit tests locked** covering the promotion rules exhaustively; empirical iOS 26.5 verification pending your next batch (you have the failing case ready — that's your gate).

## Round-6 report acknowledgment

Your addendum was the highest-signal report we've had. **Zero mistake time** wasted:
- You upgraded to v1.0.20 → tried the newly-parsing `tapOn: {role, name}` → observed regression instantly
- You diagnosed the layer precisely (iOS 26.5 elementType exposure change, not selector wire) — that's exactly what smix's perception layer is for, so we know where to fix
- You noted the same failure mode would extend to SwiftUI `.confirmationDialog` / keyboard `return` bar buttons — turned out our fix picks those up for free by generalizing over `.alert` / `.dialog` / `.sheet` ancestors

Also **reverted cleanly to `fallback: [id, text]`** — good example of `fallback:` earning its keep for cross-version resilience. Consumer contract on smix's side is "the primitive works; iOS drift is smix's problem, not yours." v1.0.21 makes good on that.

## D1 — iOS 26.5 `UIAlertController` button role mapping

### Root cause

`UIAlertController` (and SwiftUI `.confirmationDialog` / `.actionSheet`) render their action buttons with underlying UIKit views that XCUITest's snapshot-time elementType inference returns `.other` (rawValue 1) or `.staticText` (rawValue 48) on iOS 26.5. On iOS 25 and earlier, the same buttons returned `.button` (rawValue 9). Not documented in Apple release notes; it's an XCUITest snapshot exposure change.

Everything else about the buttons is unchanged — they have `accessibilityLabel`, they respond to `.tap()`, they emit `.button` accessibility trait bits. But smix's `role: button` yaml selector was matching on `rawType == "button"` (from `elementTypeName(9)`), so alert buttons became invisible to `Selector::Role`.

### Fix — perception-layer promotion

`TreeRoute.nodeToDict` now propagates an `inActionContainer` boolean through recursion. When we enter an `.alert` (7) / `.dialog` (8) / `.sheet` (5) subtree, we mark all descendants as in-action-container. For any descendant with:
- `elementType == .other` (1) OR `elementType == .staticText` (48)
- AND non-empty `label` or `title`
- AND `inActionContainer == true`

we emit `"rawType": "button"` on the wire instead of `"other"` / `"staticText"`.

Non-labeled decorative background views under an alert stay `"other"` — the label gate prevents sweeping up too aggressively. Real `.button` (9) elements under an alert stay `"button"` unchanged. Nested containers (sheet-inside-alert) don't double-promote — single boolean tracks presence.

### Why perception layer, not resolver

Two other options considered:
1. Rust `smix-selector-resolver` matching `Role::Button` against `{rawType == "button"} OR {rawType == "other" && ancestor.alert && labeled}` — same semantics, but scattered lookaside logic in every consumer that reads the tree.
2. Add a new `Role::AlertButton` variant — new wire surface for consumers to memorize.

Rejected both. `docs/ai-guide/03-selectors.md §Role Selectors` promises `role: button` matches "iOS: XCUIElement type + traits" — that's a semantic promise the perception layer should uphold across iOS version drift. Fixing at emission means every downstream matcher (Rust resolver, host coord resolver, Kotlin resolver, TypeScript wire consumer) sees consistent data. No cross-cutting patches.

### Wire compatibility

- `rawType` field shape is `String`, unchanged.
- Existing `role: alert` / `role: dialog` / `role: sheet` selectors targeting the container itself are unaffected — we only touch descendant elementTypes.
- Pre-v1.0.21 consumers matching alert-buttons via `text:` or `id:` still see the same match — text and id fields untouched.
- No CLI or adapter parser changes. Byte-identical to v1.0.20 on those crates.

### Ship gate

7 new Swift unit tests cover the rules end-to-end (`TreeRouteTests`):
- `test_serialize_alertOtherChildWithLabel_promotedToButton` — Cancel/OK under .alert with rawValue 1 → promoted to "button"
- `test_serialize_alertStaticTextChildWithLabel_promotedToButton` — Reload with rawValue 48 → promoted
- `test_serialize_dialogNestedButton_promoted` — .other 2+ levels deep under .dialog → still promoted
- `test_serialize_alertOtherChildNoLabel_notPromoted` — decorative background stays "other"
- `test_serialize_otherOutsideActionContainer_notPromoted` — .other with label at top level stays "other"
- `test_serialize_realButtonUnderAlert_stillButton` — pre-existing .button (rawValue 9) inside .alert stays "button"
- `test_serialize_sheetOtherChild_promoted` — SwiftUI `.confirmationDialog` / `.actionSheet` case covered

26 TreeRoute tests total, all green.

**Real-sim empirical validation pending your next batch.** We tried to trigger a UIAlertController on `sim-insight` via Preferences navigation but the flows into Erase / Reset alerts don't fire the alert without deep navigation. Your `tapOn: { role: button, name: 'Reload' }` failing case on the actual QA Panel alert is the empirical gate.

## v1.0.21 does NOT change

- v1.0.20 D1 `visible_to_selector` full selector table — preserved.
- v1.0.20 D2 `tapOn: {role, name}` + `tapOn: {label}` parser — preserved.
- v1.0.20 D3 `--dry-run` alias — preserved.
- v1.0.19 top-level `lastInteractiveNamedIds` — preserved.
- v1.0.18 per-session `interactiveNamedIds` + `waitForAnimationToEnd: N` — preserved.
- CLI / adapter / SDK / Rust wire types — byte-identical to v1.0.20.

## What v1.0.21 does NOT solve

- **launch-chain `title-all-cameras` probe** — your side (main-tabs render + role-assignment).
- **4th native race** (`expo::setProperty` / `ConstantDefinition.buildDescriptor`) — your side; separate follow-up branch.
- **Docker testbed image** — your side; smix ship-gate stays on parser + Swift unit tests.

## Retest checklist (v1.0.20 → v1.0.21)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.21

# 2. Cold rebuild is REQUIRED — this cycle changes the Swift runner,
#    not just the CLI. Version-mismatch gate will refuse boot otherwise.
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle <BUNDLE>
# → "synced runner sources → 1.0.21" line means it re-extracted

# 3. Revert your yaml back to the natural shape
#   (was: fallback: [id: 'Reload', text: 'Reload'] — working workaround)
#   (v1.0.21: tapOn: { role: button, name: 'Reload' } should match)

# 4. Full bootstrap batch
bun test:e2e -- --metro-log /tmp/metro.log

# 5. If role: button still doesn't match on some other alert-adjacent
#   UI, dump the tree at the failure moment and share — same
#   promotion should apply if the ancestor is .alert / .dialog /
#   .sheet. If it's another container type (e.g., a custom modal
#   that reports as .other), we can extend the ancestor set.
smix diagnostic dump --json | jq '.runner.lastInteractiveNamedIds'
```

## Ship-worthy branch reminder

Your `bugfix/GOL-611-native-cold-boot-crash` (`7cb66c9f`, 6 commits) has now been validated across **7 consecutive smix versions** (v1.0.14 → v1.0.20). 3-UAF chain still fully closed. 4th race is a separate follow-up branch. Merge to develop when convenient.

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

For v1.0.21 feedback: please note whether:
1. `tapOn: { role: button, name: 'Reload' }` matches the QA Panel alert button (the failing v1.0.20 case).
2. If you have other alert-adjacent flows in the bootstrap batch — do they benefit from the promotion?
3. Any `role: button` targets that STILL don't match — that would point to a container ancestor we don't cover yet (custom modal? non-`.alert`/.dialog/.sheet action host?).

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.21-shipping.md
```

## Prior chain

- `smix-feedback-2026-07-11-post-native-fix.md`
- `insight-2026-07-11-post-native-fix-response.md`
- `insight-2026-07-11-v1.0.12-open-questions.md`
- `smix-feedback-2026-07-11-v1.0.12-answers.md`
- `insight-v1.0.14-shipping.md`
- `insight-v1.0.15-shipping.md`
- `smix-feedback-2026-07-11-round-2-status.md`
- `insight-v1.0.16-shipping.md`
- `smix-feedback-2026-07-12-v1.0.16-runner-crash.md`
- `insight-v1.0.17-shipping.md`
- `smix-feedback-2026-07-12-v1.0.17-results.md`
- `insight-v1.0.18-shipping.md`
- `smix-feedback-2026-07-12-v1.0.18-round-4.md`
- `insight-v1.0.19-shipping.md`
- `smix-feedback-2026-07-12-v1.0.19-flow-progress.md` (base + addendum)
- `insight-v1.0.20-shipping.md`
- **this doc** — v1.0.21 iOS 26.5 alert-button role mapping
