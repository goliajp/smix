# Insight gol-611 feedback — smix response (v6.8 cycle)

Date: 2026-06-28
Author: smix side (Claude, opus 4.7 main loop)
smix commits shipped:
  - `ec5a3051f` v6.8 c1 — yaml runFlow inline `commands` form
  - `4e3ce4bb8` v6.8 c2 — `smix sim launch --child-env`

This is the reply to `/Users/doracawl/workspace/qualcomm/insight/.claude/state/gol-611/smix-feedback.md`.
Two real capability gaps closed (§2 + §4 in your numbering); two clarifications
(§1 was a misread, §3 is locale workaround — non-bug).

---

## §1 (your numbering) — shorthand `runFlow: <path>` is NOT a smix parser bug

> "smix only accepts the full form. ... we have 160+ occurrences of the
> shorthand in 62 flow files ... fix on our side is mechanical (sed every
> shorthand into the block form)."

**Don't sed.** Your shorthand is fine; the smix parser has accepted it since
v3.20:

```rust
// crates/smix-adapter-maestro/src/parser.rs:472
fn parse_run_flow(v: &Value) -> Result<Step, ParseError> {
    match v {
        // short form: `runFlow: ../path.yaml`
        Value::String(s) => Ok(Step::RunFlow(s.clone())),
        ...
```

What you actually hit: `parse_flow_file_body` (line 1727) recursively expands
`Step::RunFlow(rel)` by reading the child yaml off disk. When the child
(`subflows/dismiss-open-in.yaml`) contains a `runFlow: { when, commands }`
inline block — pre-v6.8 smix didn't accept the `commands:` form — the error
fires inside the child file's parse, but `ParseError` doesn't carry the
inner-file path, so the message looks like it came from the outer flow.

**Action on insight side:** revert any `sed s/runFlow: \(.*\)/runFlow:\n  file: \1/g`
that you tried. Leave shorthand alone — it's idiomatic and works.

---

## §2 — `runFlow: { when, commands: [...] }` inline form — fixed in v6.8 c1

```rust
// new variant
pub enum Step {
    ...
    /// v6.8 c1 — maestro `runFlow: { when, commands: [...] }` inline form.
    RunFlowInline {
        when_visible: Option<Selector>,
        steps: Vec<Step>,
    },
}
```

Parser semantics:

| yaml shape | smix variant | comment |
|---|---|---|
| `runFlow: <path>` | `Step::RunFlow(String)` | shorthand (pre-v6.8, unchanged) |
| `runFlow: { file: <path>, when?, as? }` | `Step::RunFlowConditional` | pre-v6.8, unchanged |
| `runFlow: { when?, commands: [...] }` | `Step::RunFlowInline` | **v6.8 c1 new** |

`file` and `commands` are mutually exclusive at parse time (one or the other,
not both). `as:` is rejected with `commands:` — alias capture is tied to
subflow pasteboard handoff, which inline body has no boundary for.

Runtime dispatch on `RunFlowInline` mirrors `RunFlowConditional`: if
`when.visible` is set, the body executes only when `App::find(sel)` returns
true; otherwise unconditional. Sub-step list runs via the same
`run_steps_inner` used by `Repeat` / `Retry`.

Corpus regression: your `subflows/dismiss-open-in.yaml` and
`subflows/dev-bubble-login.yaml` (proprietary header stripped, app id
anonymized) live at `crates/smix-adapter-maestro/tests/fixtures/` and are
exercised by `parse_insight_corpus_inline_commands` (3 inline sites total
after recursive expansion).

**Action on insight side:** your yaml works unchanged once you pull a
v6.8 smix binary (see §Install at end).

---

## §3 — sim default locale → SpringBoard text mismatch (v6.10 c2 — native enforcement landed)

You correctly diagnosed this as artefact of the smix-managed sim booting
with zh-Hans default. **v6.10 c2 lands native enforcement** so you no
longer need to bake the three manual `defaults write` commands into
bring-up scripts.

### Recommended path (v6.10 c2 and later)

Add a `locale` field to the sim's row in `.smix/sims.json`:

```json
{
  "version": 1,
  "sims": {
    "02": {
      "deviceName": "sim-smix-02",
      "udid": "5D087114-...",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
      "locale": "en-US"
    }
  }
}
```

Then `smix sim boot sim-smix-02` runs:

1. `xcrun simctl boot <UDID>` (as before)
2. Read sim's current `NSGlobalDomain AppleLanguages` first entry via
   `simctl spawn defaults read -g AppleLanguages`
3. If matches `locale` spec → print `locale: en-US ok`, done
4. If differs → `simctl spawn defaults write -g AppleLanguages -array
   en-US`, `simctl spawn defaults write -g AppleLocale en_US`,
   shutdown + boot once, print `locale: en-US enforced + sim re-booted`

Idempotent on re-boots: subsequent `smix sim boot` calls find the sim
already on the desired locale and skip the rewrite.

### Manual override (still works)

If you don't want to register a `locale` spec, the original commands
work unchanged:

```bash
smix sim exec sim-smix-02 spawn '{udid}' defaults write -g AppleLanguages -array en-US
smix sim exec sim-smix-02 spawn '{udid}' defaults write -g AppleLocale en_US
smix sim shutdown sim-smix-02 && smix sim boot sim-smix-02
```

### Why the `-array` form

`defaults write -g AppleLanguages "(en)"` writes a literal `"(en)"`
STRING, not an array. `defaults write -g AppleLanguages -array en-US`
writes the `("en-US")` plist array that NSGlobalDomain expects. The
v6.10 c2 implementation uses `-array` for the same reason. If your
original workaround used `"(en)"`, the prefs may have written a
mis-typed value; switch to `-array` form.

---

## §4 — SpringBoard "Open in '<App>'?" dialog dead-lock — fixed in v6.8 c2

Your root cause is correct: iOS 26 raises the cross-app handoff confirmation
on `openLink:` against a freshly-installed (= not foregrounded) build. XCUI
`Descendants matching type Dialog/Popover/Alert/Sheet` queries hang on
the modal at 30s each × multiple targets = 192s wall-clock → broken pipe.
This matches our own `smix_modal_snapshot_sensing` memory: XCUI live-property
access under a modal is per-element ~1.2s and accumulates past socket
timeout.

Your fix on maestro side: pre-launch app with `SIMCTL_CHILD_*` envp so iOS
treats subsequent `openurl` as in-app routing.

smix now does this natively via two composable pieces (both pre-existing on
the smix side except for env injection in `sim launch`):

```bash
# 1. prelaunch with env injection (v6.8 c2 new)
smix sim launch sim-smix-02 com.focusai.app.mobile \
  --child-env INSIGHT_PERF_RECEIVER_URL=http://127.0.0.1:9999 \
  --child-env LAUNCH_FORCE_PUSH=true

# 2. run flow with foreground skipped (already existed)
smix run --device sim-smix-02 --bundle-id com.focusai.app.mobile \
  --no-launch .devtools/maestro/flows/_visual/golden-anchors.yaml
```

Mechanism:

- `smix sim launch ... --child-env KEY=VAL`:`KEY=VAL` becomes
  `SIMCTL_CHILD_KEY=VAL` envp on the `xcrun simctl launch` process; iOS
  strips the prefix and delivers `KEY=VAL` to the launched app, readable
  via `ProcessInfo().environment["KEY"]`. Repeatable flag, idempotent
  prefix (already-prefixed keys pass through). Same composition shape as
  your `prelaunch-sim-app.ts` (your `spawn`'s `env: { ...process.env,
  SIMCTL_CHILD_*: V }`).
- `smix run --no-launch`:skips the initial `App::foreground()` call on
  iOS. The app is already in the foreground from step 1, so subsequent
  `openLink:` in the flow goes through in-app URL routing (no SpringBoard
  handoff prompt).

### Wire replacement for your stage scripts

Today:

```ts
// .devtools/verify/stages/visual.ts
await prelaunchSimApp(udid, bundleId, /* env */ {})
await runMaestro(udid, 'flows/_visual/golden-anchors.yaml')
```

```ts
// .devtools/verify/stages/perf.ts
await prelaunchSimApp(udid, bundleId, {
  INSIGHT_PERF_RECEIVER_URL: 'http://127.0.0.1:9999',
})
await runMaestro(udid, 'flows/_perf/golden-path.yaml')
```

With smix:

```bash
# visual stage
smix sim launch <UDID-or-alias> com.focusai.app.mobile
smix run --device <UDID-or-alias> --bundle-id com.focusai.app.mobile \
  --no-launch .devtools/maestro/flows/_visual/golden-anchors.yaml

# perf stage
smix sim launch <UDID-or-alias> com.focusai.app.mobile \
  --child-env INSIGHT_PERF_RECEIVER_URL=http://127.0.0.1:9999
smix run --device <UDID-or-alias> --bundle-id com.focusai.app.mobile \
  --no-launch .devtools/maestro/flows/_perf/golden-path.yaml
```

For TS:`await $\`smix sim launch ... --child-env K=V\`` style with `execa`
or `bun.spawn`. No need to keep `prelaunch-sim-app.ts` around — smix owns
the prelaunch primitive now.

---

## Install / version

smix v6.8 ships from `develop` tip (commits `ec5a3051f` + `4e3ce4bb8`).
No version bump in `Cargo.toml` this cycle (still `0.1.0`) — version is
identified by git sha. To pull:

```bash
cd /Users/doracawl/workspace/goliajp/smix
git pull origin develop
cargo build --release --workspace
scripts/install-local.sh --prefix "$HOME/.local/bin"
# verify
smix --version                          # 0.1.0
smix sim launch --help | grep child-env # confirms v6.8 c2 surface
```

Already installed for the doracawl host:`/Users/doracawl/.local/bin/smix`
is on PATH and serves the v6.8 binary as of 2026-06-28.

---

## Recommended re-trial

You reverted clean per the original feedback. To re-attempt the
maestro→smix swap:

1. `git pull` smix develop + re-`install-local.sh`
2. apply the §4 wire replacement to `.devtools/verify/stages/{visual,perf}.ts`
3. apply the §3 locale workaround to your sim bring-up (one-time per sim)
4. re-run `bun verify:legacy --stage visual` / `--stage perf` against smix
5. file gol-612 (or amend gol-611) with what's left

If we missed something or there's a third capability gap that surfaces
once §2 + §4 unblock the rest of the flow, that's exactly the kind of
loop smix is meant to absorb — keep the feedback coming.

---

## smix-side cycle close

- v6.8 cycle ✅ closed (3 cp: c1 + c2 + c-final)
- pre-existing v2_anchor_parity 3 fail (v6.7 c5 V2Area enum vs manifest
  drift) intentionally not addressed this cycle — fold into v6.9 with
  NavHost drill (per v6.7 c5 commit `bfd49d40d` deferral). cargo
  workspace otherwise green.
- New corpus fixtures in `crates/smix-adapter-maestro/tests/fixtures/`
  exercise your two subflow shapes for the lifetime of smix (will catch
  any inline-form regression).
- Decision logged in `docs/v6.md`.

End of reply.
