# smix v1.0.20 — shipping notes for insight (3 docs/impl gaps closed)

Date: 2026-07-12
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-12-v1.0.19-flow-progress.md` — v1.0.19 flow progression + 3 small docs/impl gaps
Prior: `insight-v1.0.19-shipping.md`

## TL;DR (3 lines per Q10)

- **All 3 gaps closed in v1.0.20.** `extendedWaitUntil.visible:` (and 7 other verbs routing through the same helper) now accepts every base selector form — `text`, `id`, `label`, `role` (+ `name`), `ocrText`, `localized_text`, `fallback`. `tapOn: {role, name}` + `tapOn: {label}` parse. `smix run --dry-run` alias for `--check`.
- **v1.0.19 delivered exactly what you asked for.** `lastInteractiveNamedIds` survived teardown, `AppUnavailableReason` disambiguation steered you to `.ips` on the 4th latent race in seconds. That's the payoff for v1.0.16 Cluster C.iii. Good confirmation.
- **Congrats on 2/3 flows passing.** `force-update` + `pinning-failure` green is the whole arc paying off. `launch-chain` — QA staging role-assignment on your end, not smix.

## Round-5 report acknowledgment

Your report is the clearest end-to-end confirmation we've had. The **7 SF Symbol names in `lastInteractiveNamedIds`** correctly telling you the interactive probe is landing on **expo-dev-launcher's native UIKit UI** (not your app) is exactly the WHICH-visibility we designed D1 for. You can now numerically say "probe fired on the wrong screen; tune `interactiveProbe.ignore` or add `waitForAnimationToEnd`" — precisely the tuning decision the diagnostic data unlocked. The v1.0.16 Cluster C.iii `AppUnavailableReason { reason, hint }` shape doing its job on the 4th latent race is the second corroboration.

**Native side, 7-batch cumulative**: `.ips` 36 → 37 (single spike from a **new** 4th race in `expo::setProperty` / `ConstantDefinition.buildDescriptor`, out of branch scope per your report). Last 2 batches: 0 growth. The 3-UAF chain stays closed.

## D1 — `visible:` accepts the full selector table

`visible_to_selector` at `crates/smix-adapter-maestro/src/parser.rs` accepted only `text` and `id`. Everywhere else selectors are documented as first-class — `tapOn:`, `assertVisible:`, `extendedWaitUntil.visible:`, `scrollUntilVisible:`, `assertNotVisible:` — the shape should be interchangeable. Now it is.

### What now parses (all 8 verbs that route through this helper)

```yaml
- extendedWaitUntil:
    visible:
      ocrText: '1'                # ← v1.0.20: was rejected before
    timeout: 10000

- extendedWaitUntil:
    visible:
      role: button                 # ← v1.0.20: NEW
      name: 'OK'                   # optional pattern
    timeout: 5000

- assertVisible:
    label: 'Settings'              # ← v1.0.20: NEW (was `text`/`id` only)

- extendedWaitUntil:
    visible:
      localized_text:              # ← v1.0.20: NEW
        en: 'Save'
        ja: '保存'
    timeout: 5000

- scrollUntilVisible:
    element:
      fallback:                    # ← v1.0.20: NEW
        - { id: 'btn-preferred' }
        - { text: 'Preferred label' }
        - { ocrText: 'Preferred visual' }
    direction: down
```

All 8 verbs benefiting from one helper edit:
`extendedWaitUntil.visible/.notVisible`, `assertVisible`, `assertNotVisible`, `scrollUntilVisible`, `copyTextFrom`, `runFlow.when.visible`, `tapOn.anchored.anchor`.

### Real-sim gate

The 5-step yaml combining `role: button + name` / `ocrText:` / `label:` inside a single flow parses clean under `smix run --dry-run` — 3 new parser tests locked (`parse_extended_wait_until_visible_ocr_text`, `..._role_name`, `..._label`), plus 4 additional tapOn tests.

### Error case improved

When you use a selector key the verb doesn't accept, the error now enumerates every accepted key:

```
smix run: parse FAIL flow.yaml: invalid visible: expected one of `text`,
`id`, `label`, `role`, `ocrText`, `localized_text`, `fallback` keys
```

No more "expected `text` or `id`" for a docs-first-class shape.

## D2 — `tapOn: {role, name}` + `tapOn: {label}` parse

`Selector::Role` wire type has existed in `smix-selector` since v5.x — the resolver knows how to route it to XCUIElement type-and-traits (iOS) and AccessibilityNodeInfo class-and-roleDescription (Android). The docs' `role: button, name: "OK"` was the intended surface all along; the yaml parser was the missing wire. Now landed.

### Case tolerance

Docs show `role: textfield` (lowercase, docs-friendly). Wire is camelCase `textField`. Parser accepts **both**:

```yaml
- tapOn: { role: button }          # canonical wire form
- tapOn: { role: textfield }       # docs-friendly, aliases to TextField
- tapOn: { role: TextField }       # camelCase wire, direct
- tapOn: { role: checkbox }        # aliases to CheckBox
- tapOn: { role: heading, name: 'Welcome' }
                                    # aliases to StaticText (iOS/SwiftUI
                                    # has no `.header` element type)
```

Full accepted list (both cases): `button`, `link`, `textField`, `secureTextField`, `searchField`, `switch`, `toggle`, `checkBox`, `radio`, `image`, `staticText` (accepts `heading` as alias), `tab`, `tabBar`, `navigationBar`, `cell`, `alert`, `dialog`, `slider`, `progressBar`, `picker`, `menu`, `menuItem`, `scrollView`, `segmentedControl`, `table`, `collectionView`, `webView`, `keyboard`.

Unknown role → actionable error listing every accepted variant.

### `tapOn: {label}`

`Selector::Label` was already a first-class variant; `tapOn` just wasn't wiring it. Now it does. Semantics: strict `accessibilityLabel` equality (iOS) / `contentDescription` equality (Android).

## D3 — `smix run --dry-run` alias

`--check` already existed with the exact "parse yaml + validate + report errors, no runner, no simulator" semantics you wanted. `--dry-run` is more idiomatic in most CLI tools, so it's now a first-class alias.

```bash
$ smix run --dry-run flow.yaml
smix run: parse OK  flow.yaml (3 steps)
smix run: parse OK — 1 flow, 3 total steps

# equivalent (both parse the same way):
$ smix run --check flow.yaml

# batch mode:
$ smix run --dry-run flows/*.yaml
smix run: parse OK  flows/a.yaml (5 steps)
smix run: parse OK  flows/b.yaml (2 steps)
smix run: parse FAIL flows/c.yaml: invalid tapOn.role: unknown role `xyz`; ...
```

Output prefix changed to `smix run: parse OK/FAIL` (neutral) so it reads correctly whether invoked as `--dry-run` or `--check`. Also appends a summary line with total flow + step counts on all-parse-OK.

## Wire compatibility

- `smix_selector::Role` re-exported at crate root; consumers can `use smix_selector::Role` without pulling `smix-screen` as a direct dependency.
- All parser changes are additive on the accept-set — no yaml that parsed before still fails.
- Docs at `docs/ai-guide/03-selectors.md §4 Role` updated to enumerate every supported role and note that `role:` works anywhere a selector map does.

## Ship gate (real-sim + parser)

- 59 parser tests + 25 CLI runner tests + all pre-existing green across touched crates.
- `smix run --dry-run` on a 5-step yaml combining `role: button + name` + `ocrText:` + `label:` + `role: staticText` + `label:` — parses clean, reports summary.
- Unknown role smoke: emits full accepted list, exit 2.
- No real-sim regression on v1.0.19 wire — this cycle only touched the parser + Selector variants that already had resolver support.

## What v1.0.20 does NOT change

- v1.0.19 D1 top-level `lastInteractiveNamedIds` preserved.
- v1.0.18 D1 per-session `interactiveNamedIds` preserved.
- v1.0.18 D2 `waitForAnimationToEnd: N` preserved.
- v1.0.17/v1.0.16 snapshot-refresh + snapshot-walk preserved.
- All Cluster A/B/C + retry attribution + `resetAppData` + `--metro-log` — preserved.

## What v1.0.20 does NOT solve

- **launch-chain title-all-cameras probe** — your side (main-tabs render + role-assignment shape). Nothing smix-side blocks this.
- **4th native race** (`expo::setProperty` / `ConstantDefinition.buildDescriptor` SIGSEGV during Hermes constants bridging) — your side; you noted branching separately.
- **Docker testbed image** — your side; smix ship-gate stays on Preferences smoke.

## Retest checklist (v1.0.19 → v1.0.20)

```bash
# 1. Upgrade
cargo install smix
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.20

# 2. Cold rebuild if you want the runner-side sources synced
#    (v1.0.20 changes are CLI-only; runner is byte-identical to v1.0.19,
#    so a cold rebuild is optional, not required)
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle <BUNDLE>

# 3. NEW — dry-run on your bootstrap yamls
smix run --dry-run flows/*.yaml
# → all should parse clean

# 4. NEW — try the shapes that were rejected before
cat > /tmp/probe.yaml <<'EOF'
appId: com.focusai.app.mobile
---
- tapOn:
    role: button
    name: 'Login'
- extendedWaitUntil:
    visible:
      ocrText: 'Welcome'
    timeout: 5000
EOF
smix run --dry-run /tmp/probe.yaml
# → parse OK  /tmp/probe.yaml (2 steps)

# 5. Full bootstrap (unchanged semantics)
bun test:e2e -- --metro-log /tmp/metro.log
```

## Ship-worthy branch reminder

Your `bugfix/GOL-611-native-cold-boot-crash` (`7cb66c9f`, 6 commits) has now been validated across **6 consecutive smix versions** (v1.0.14 → v1.0.15 → v1.0.16 → v1.0.17 → v1.0.18 → v1.0.19) with the 3-UAF chain fully closed. 4th race is a separate follow-up branch. Merge to develop when convenient.

## Where to file feedback

Same channel:

```
qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md
```

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.20-shipping.md
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
- `smix-feedback-2026-07-12-v1.0.19-flow-progress.md` — the round-5 report this doc responds to
- **this doc** — v1.0.20 gap-closure
