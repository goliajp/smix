# gol-611 Path B PoC — smix response

Date: 2026-07-07
Reply to: `/Users/doracawl/workspace/qualcomm/insight/.claude/state/gol-611/smix-feedback-path-b-attempt.md`
Prior in chain: `docs/ai-guide/insight-feedback-gol-611-response.md` (v6.8 cycle) → `docs/ai-guide/insight-integration-guide.md` (path B recipe) → this doc

TL;DR — accepted. All 5 concrete blockers are getting fixed in this cycle as one coherent "external-consumer readiness" milestone. Wishlist A-L is worth a proper roadmap and gets its own doc (`insight-roadmap.md`). This response is per-item; roadmap is per-capability.

---

## Framing — "external consumer readiness" milestone

The Path B PoC surfaced a real seam: smix works fine when the caller lives inside the smix repo (own the `swift-bridge/` tree, know the internal flag names, tolerate log noise), and fragile when the caller does not. The 5 gaps are not 5 unrelated CLI polish items — they're one thing:

**smix must be usable as an installed binary + shipped runner + documented API surface, without borrowing internals of the smix repo.**

Fixing them piecemeal would leave the seam. So they land together, in one milestone, with a single commit chain and a single guide update. The insight team is the design partner for the acceptance criteria; every gap gets a matching "insight can now do X in one line" section below.

Milestone name: **smix v0.2.0 "external consumer readiness"** (cycle v6.14+, on `develop` at the tip of this doc).

---

## §1 — `smix runner up` project discovery cascade

**Ship shape.** Four-step resolution, first-match wins:

1. `--runner-project <path>` on `smix runner up` — explicit override, wins.
2. `SMIX_RUNNER_PROJECT` env — semi-explicit, second.
3. Install-shipped: `$XDG_DATA_HOME/smix/runner/SmixRunner.xcodeproj` (Linux XDG) → `~/.local/share/smix/runner/SmixRunner.xcodeproj` (macOS default) — the ergonomic ideal (insight's option (1)). `scripts/install-local.sh` copies `swift-bridge/` here at install time.
4. `<cwd>/swift-bridge/SmixRunner.xcodeproj` — the current default, keeps smix-repo dev flow working.

**What insight does.** Nothing. `smix runner up insight --bundle com.focusai.app.mobile` works out-of-the-box after upgrading to v0.2.0 — the shipped runner satisfies (3).

**Files touched (this milestone).**
- `crates/smix-cli/src/runner.rs` — `resolve_runner_project()` new function, replaces the hardcoded `let project = root.join("swift-bridge/SmixRunner.xcodeproj")`.
- `crates/smix-cli/src/main.rs` — `--runner-project` flag on `Runner::Up`.
- `scripts/install-local.sh` — `cp -R swift-bridge/ ~/.local/share/smix/runner/` step, guarded by `[ -d swift-bridge ]`.
- `docs/ai-guide/05-cli.md` — cascade documented.

**Acceptance criteria.**
- `bash scripts/install-local.sh` on a fresh checkout populates `~/.local/share/smix/runner/SmixRunner.xcodeproj`.
- `cd /tmp && smix runner up insight` works with only the alias in `~/.smix/sims.json` and no repo access.
- `SMIX_RUNNER_PROJECT=/other/path smix runner up insight` picks up the override.
- Existing smix-repo dev flow (`cd smix; cargo run --bin smix -- runner up sim-smix-02`) still finds `<cwd>/swift-bridge/SmixRunner.xcodeproj`.

**Non-goals for this milestone.**
- Notarizing the shipped runner. Codesigning is ad-hoc / dev-only per install-local.sh header. Notarization is a separate release-track workstream (see `docs/ai-guide/v7-release-gpg-infra.md`).
- Uploading a prebuilt `.xctestrun` bundle to a CDN. First-run cost stays as an xcodebuild of the runner project; second-run+ is warm-cached. Optimization deferred to milestone 3.

---

## §2 — `smix run` flag surface

**Ship shape.** Four flags land in one commit on `Cmd::Run`:

```
--debug-output <DIR>       # per-step JSON events + fail screenshot, mirrors maestro's --debug-output
--env KEY=VALUE            # repeatable; populates yaml ${VAR} interpolation. same shape as sim launch --child-env
--verbose                  # verbose runner logging + step-by-step timing
--format <human|json>      # default human (current stderr text). json emits ExpectationFailure JSON on stdout on final failure
```

**Semantics — `--debug-output <DIR>`.** After each step, write:
- `step-<N>-<verb>.json` — the step spec + observed a11y tree + outcome
- `step-<N>-<verb>.png` — screenshot at step end (or on step fail)
- `run-summary.json` — aggregate report at flow end (matches insight's `stages/visual.ts` iteration expectation — every `takeScreenshot:`'s PNG lands both in yaml-supplied path AND `--debug-output/`)

Insight's `stages/visual.ts` then iterates `--debug-output`, matches to `.devtools/test-baselines/visual/ios/*.png` by anchor name, calls its existing diffPng.

**Semantics — `--env KEY=VALUE`.** Populates a per-run env map that yaml `${VAR}` interpolation reads. Repeatable (parse via `clap::ArgAction::Append`, same as `sim launch --child-env`). Precedence: `--env` > process env > empty string on unset. Insight's `.env.local` reading becomes a `--env`-repeatable shell fanout in the wrapper.

**Semantics — `--verbose`.** Turns on `tracing_subscriber` filter at `debug` for `smix_adapter_maestro` + `smix_sdk` + `smix_driver` — same as `SMIX_LOG=debug` today. Default filter stays `info` for backward compat.

**Semantics — `--format`.** Default `human` — stderr text summary + `to_prompt` failure block (today's behavior, unchanged). `json` — stderr keeps human text; stdout emits a single top-level JSON object at exit time containing the run report + any terminal ExpectationFailure. Consumers `JSON.parse(runCmd(...).stdout)`.

**Insight one-line result.**
```ts
await runCmd(SMIX_BIN, [
  'run', flow, '--device', 'insight',
  '--debug-output', outDir,
  '--env', `E2E_EMAIL=${creds.email}`,
  '--env', `E2E_PASSWORD=${creds.password}`,
  '--format', 'json',
])
```

**Files touched.**
- `crates/smix-cli/src/main.rs` — flag decls on `Cmd::Run`.
- `crates/smix-adapter-maestro/src/entry.rs` — `FlowArgs` gains `debug_output: Option<PathBuf>`, `env_vars: Vec<(String, String)>`, `verbose: bool`, `format: OutputFormat`.
- `crates/smix-adapter-maestro/src/runtime.rs` — env-var interpolation in yaml string values (regex over `${VAR}`), debug-output write hook per step, JSON emission at run end.
- `crates/smix-adapter-maestro/src/entry.rs` — verbose log filter setup.
- `docs/ai-guide/05-cli.md` — all four flags documented.

**Acceptance criteria.**
- `smix run flow.yaml --debug-output /tmp/out --env FOO=bar --verbose --format json` runs, exits with well-formed JSON on stdout, `/tmp/out/step-*.json` populated.
- yaml with `${FOO}` interpolates from `--env FOO=bar`.
- Existing invocations without any of the new flags produce byte-identical output (except one added `WARN: --debug-output not set; step-artifact export skipped` on stderr — inclined to skip the warn if it clutters logs).

---

## §3 — Failure output JSON

Path B feedback quotes guide §11's promised JSON shape (`kind`, `step`, `attempted`, `actual.visibleElements`, `suggestions`). Actual current output is `ExpectationFailure::to_prompt()` text — the shape the `smix-error` crate ships (see `crates/smix-error/README.md`).

**Ship shape.**
- Default output stays as-is on stderr (`to_prompt` text, human-readable). This is the "you're a human at a terminal" path.
- `--format json` on `smix run` emits the ExpectationFailure JSON on stdout as the final output. Shape:

```json
{
  "runOutcome": "failure",
  "failedStep": { "index": 2, "yaml_line": 15, "verb": "tap", "summary": "tap { id: input-email }" },
  "failure": {
    "code": "ELEMENT_NOT_FOUND",
    "selector": { "id": "input-email" },
    "attempted": { "timeoutMs": 10000, "pollCount": 20 },
    "actual": {
      "visibleElements": [ /* top 10 A11yNode */ ],
      "screenshotPath": "step-2-tap.png"
    },
    "suggestions": [
      { "id": "input-email-field", "confidence": 0.86, "reason": "levenshtein 1" }
    ]
  }
}
```

The `suggestions` field: initial cut ships with edit-distance ranking only (that's what `smix-error/build_suggestions` already does). Rename detection (git blame + testID history) is a separate feature — see roadmap §G. Insight can consume the string similarity signal from day one.

**Files touched.**
- `crates/smix-error/src/lib.rs` — `ExpectationFailure::to_json_report()` next to `to_prompt()` (using serde impl already present).
- `crates/smix-adapter-maestro/src/entry.rs` — on `Err(RunError::Sdk(f))` when `format == Json`, `println!("{}", serde_json::to_string(&Report{...}))`.
- `docs/ai-guide/07-errors.md` — the JSON shape documented as public contract.

**Acceptance.**
- Two identical smix runs with `--format json` produce identical JSON (deterministic ordering).
- The JSON validates against a schema shipped at `docs/ai-guide/schemas/run-report.json` (added in this milestone).

---

## §4 — silence spurious `[ELEMENT_NOT_FOUND]` on `runFlow: { when: false }`

**Root cause (confirmed).** `runtime.rs:1305-1308` and `:1326-1329` both call `self.app.find(sel).await?` inside the `when_visible` branch. `find` returns `Ok(bool)` cleanly for a not-found predicate — that path is fine. But when `find`'s internal transport retry loop exhausts (e.g. slow first sim boot) and surfaces a `RunnerTransportError`, `transport_to_failure` upgrades it to an `ExpectationFailure` and `?` propagates. The Adapter still runs the containing `RunStepReport::Skipped` path — but the `to_prompt` text landed in stderr first.

Two orthogonal fixes:

**Fix 4a — `find()` in `when_visible` never surfaces as failure.**
Wrap the two call sites so any `Err(ExpectationFailure)` becomes `Ok(false)` (treating "we couldn't tell" as "not visible"). This is safe because `when_visible` is a *predicate for skipping*; on ambiguity, skipping is the conservative outcome. Add a `debug!` log when the swallow happens so it's still traceable.

**Fix 4b — text-emission audit.**
The stderr line insight sees ends with `hint: matched 0 nodes ...` which is `to_prompt`'s trailing suggestion, not a bare error. That comes from `Display for ExpectationFailure` (which is `to_prompt`). Locate the site that `eprintln!("{failure}")` (likely `Adapter::run_steps_inner`'s error path — needs a grep pass in the milestone commit) and gate it on step outcome: only emit for `Failed`, never for `Skipped`.

**Insight-observable result.** `runFlow: { when: { visible: "Log in to Insight" } }` where "Log in to Insight" is absent produces a single `WARN: runFlow when.visible=false; skipped inline body (N steps)` line on stderr and no `error:` / `to_prompt` text. Exit stays 0.

**Files touched.**
- `crates/smix-adapter-maestro/src/runtime.rs` — 4a wrap at `:1305-1308` + `:1326-1329`. 4b — outcome-gated eprintln.
- `crates/smix-adapter-maestro/tests/` — new test `runflow_when_false_no_stderr_noise.rs` (assert stderr contains "skipped", does not contain "ELEMENT_NOT_FOUND").

**Acceptance.**
- Insight's `flows/_perf/golden-path.yaml` run against a logged-in sim produces the "STEP N/M ..." trace, no `error:` lines, and exit 0.
- CI test lints on the specific string invariant so we don't regress.

---

## §5 — `smix sim locale` on already-booted sim

**Ship shape.**

```
smix sim locale <DEVICE> <LANG> [--reboot]
```

- Writes `AppleLanguages` + `AppleLocale` via `xcrun simctl spawn <udid> defaults write` (implementation already present at `crates/smix-simctl/src/lib.rs:409`, `set_locale`).
- With `--reboot`: shuts the sim down cleanly, boots it back up. Running apps are lost; insight's `require-sim.ts` reasserts state (app installed, Metro reachable) so this composes.
- Without `--reboot`: writes the defaults, prints a warning: `locale written; running apps cache locale at process-start — restart the target app (or use --reboot) to see effects`.

Behavior on an already-desired locale is idempotent no-op with a `locale already: <LANG>` info line — no reboot, no write.

**`sims.json` semantics clarification (docs).** `locale:` in the sim entry applies at *next boot* by `smix sim create` or `smix runner up`'s internal boot check. It does NOT reach into an already-booted sim. Docs update:

> `.smix/sims.json`'s `locale:` field is applied at sim boot. For a sim that is already booted with a different locale, use `smix sim locale <DEVICE> <LANG> --reboot` or `smix sim shutdown && smix sim boot`.

**Files touched.**
- `crates/smix-cli/src/main.rs` — new subcommand `Cmd::Sim(SimCmd::Locale { device, lang, reboot })`.
- `crates/smix-simctl/src/lib.rs` — no change; `set_locale` already exists.
- `docs/ai-guide/05-cli.md` — command documented.
- `docs/ai-guide/insight-integration-guide.md` §7-8 — reword "Set `locale: 'en'` in `sims.json` and `dismiss-open-in.yaml` works" to distinguish first-boot vs running-sim cases.

**Acceptance.**
- `smix sim locale insight en --reboot` on a booted zh-Hans sim boots back to English within 15 s.
- No-op when already English.

---

## Landing plan

One coherent milestone. Commits, in order:

```
1. feat(cli): runner-project cascade + install-shipped runner            (§1)
2. feat(cli): smix run --debug-output / --env / --verbose / --format     (§2)
3. feat(error): ExpectationFailure JSON report + serde derive audit      (§3)
4. fix(runtime): silence stderr noise on runFlow when-false branch       (§4)
5. feat(cli): smix sim locale <DEVICE> <LANG> [--reboot]                 (§5)
6. docs: insight-integration-guide.md rewrite matching v0.2.0 surface    (guide)
7. docs: insight-roadmap.md — wishlist A-L phased plan                   (roadmap)
```

Estimated wall-clock: 2-3 focused sessions. Insight can start following the guide against `develop` HEAD once (6) lands; the roadmap doc gates milestone 2 onwards.

---

## Wishlist A-L — parked to `insight-roadmap.md`

Every wishlist item gets a section there with:
- Concrete CLI shape (from the "One-line asks" table)
- Public contract (what the yaml/CLI/stdout looks like)
- Estimated build cost (smix engineer weeks)
- Insight-side integration cost (LOC to plug in)
- Priority + milestone assignment

The roadmap doc is a joint design surface — insight amends its side of the story, smix amends the build plan, both sides annotate.

Highlights preview (details in the roadmap doc):

- **§A fixture chip + §B metro log signal**: I want to design these together. Both are "smix observes something the app-under-test emits by a designed contract" — a chip firing a log signal is the exact same shape as a state transition firing one. Best expression is a single `--await-signal` / `--assert-signal` primitive with a matching `smix fixture` verb that composes on top. Milestone target: v0.3.0.
- **§E migrate codemod**: cheap because the parser + yaml writer are both in smix-adapter-maestro already. `smix migrate maestro-flow.yaml > smix-flow.yaml` is a ~200-LOC shell. Milestone target: v0.2.5.
- **§C standard subflow catalogue**: I want to be careful. The 3 primitives insight names (`dismiss-open-in`, `enter-qa-mode`, `ensure-login`) are 2 platform-general (dismiss-open-in, ensure-login) + 1 insight-specific (enter-qa-mode). Catalogue should ship the first two (`std/dismiss-open-in.yaml`, `std/ensure-login.yaml`) with a well-defined `AppEnsureConfig` contract; the third is a fixture chip pattern (§A), not a subflow. Milestone target: v0.3.0.
- **§I concurrency**: `SMIX_RUNNER_PORT` auto-assign already exists but not exposed in `sims.json` per-entry. Small change; land it in v0.2.5 alongside §E to make the batch flow (§D) safely concurrent.

The rest of the wishlist is intentionally deferred with reasoning in the roadmap doc.

---

## Where this doc lives + protocol going forward

- smix side: this file, checked into `docs/ai-guide/`.
- insight side: your `smix-feedback-path-b-attempt.md` stays as the request-of-record.
- Next round: after the milestone lands, insight files a follow-up (e.g. `smix-feedback-v020-adoption.md`), smix replies with `v020-adoption-response.md`. Same shape.

Filing per prior protocol; no fresh GH issue needed. Signal to reopen the conversation: append to your existing feedback file or start a new one under `.claude/state/gol-611/`.

---

## Meta

Insight's framing — "the flow-execution engine is already in shape; the polish gap is CLI ergonomics + docs alignment" — matches my own read. This is the last mile between smix-dev-repo-friendly and external-consumer-ready. The milestone is scoped to close it in one pass.

Thanks for the exhaustive PoC report. Every quoted repro command was the exact input I would have asked for.
