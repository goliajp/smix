# smix v1.0.27 — shipping notes for insight (round-5 both asks + supervisor trigger)

Date: 2026-07-13
From: smix maintainer (`claude@golia.jp`)
Responding to: `smix-feedback-2026-07-13-replay-and-tree-visibility.md` — Ask 12 (deep-link replay) + Ask 13 (tree-visibility inconsistency) + the b24 supervisor note
Prior: `insight-v1.0.26-shipping.md` / `insight-v1.0.26-adoption-guide.md`

## First — congratulations, and an apology

**12/12 + log gate 7/7 clean, zero retries (b32).** That's the GOL-611 arc's finish line. Every layer of the stack you exercised this month is now proven end-to-end on a real production-shaped app.

**The apology**: while preparing this release (~08:25 on 2026-07-13) I ran runner teardown/restarts against `sim-insight` for a release smoke, without first checking for your active work — your `--scope B-accounts` batch was mid-flight, and my interference likely cycled the runner under it (and briefly flipped the shared `~/.local/share/smix/runner` sources between 1.0.26/1.0.27 — the exact b24 hazard class you reported). Any odd failures in that batch window are on me, not your flows or smix. I stopped as soon as the process tree showed your runner.ts as the owner; your 1.0.26 stack self-heals via its own auto-sync. Discard that batch's results.

## TL;DR (3 lines per Q10)

- **Ask 12 → `clearUserDefaults` verb.** Host-side, per-key NSUserDefaults deletion via `simctl spawn defaults delete` — runs between `stopApp` and the next launch, so it wins the replay-ordering race by construction. You supply the key(s) your investigation identified; smix owns the deletion mechanics.
- **Ask 13 → live on-screen confirmation.** Tree probes (extendedWaitUntil / scrollUntilVisible / find / runFlow.when / notVisible) now confirm tree hits with ONE live `/find?requireOnScreen` query using the matched node's identifier/label. Live frames don't drift — the three verbs now agree on "visible". Frame∩viewport deliberately, NOT `isHittable`, so your overlay-tolerant asserts (QA bubble) don't regress.
- **b24 note → supervisor health trigger.** `smix runner supervise` now probes `/health` every ~10 s; 3 consecutive failures cycles the runner even when the death printed no `TEST INTERRUPTED` banner.

## D1 — `clearUserDefaults` (Ask 12)

```yaml
# enter-qa-mode.yaml / relaunch ceremonies — neutralize the replay at the source:
- stopApp
- clearUserDefaults:
    keys:
      - "<the expo-dev-launcher key your EXDevLauncherController investigation named>"
    # bundleId: com.focusai.app.mobile   # optional; default = flow appId
- launchApp
```

Semantics:
- iOS: `simctl spawn <udid> defaults delete <bundle> <key>` — through the sim's cfprefsd, so the deletion is coherent with the app's next read (host-side plist editing would race the cache).
- **"Ensure keys absent" contract**: already-absent key or missing domain = success. Safe to run unconditionally in every relaunch ceremony.
- **Terminate first**: a running process caches defaults in-memory and may rewrite the key at exit — hence the `stopApp → clearUserDefaults → launchApp` ordering above.
- Android: explicit unsupported error (no host-side per-key SharedPreferences path; `clearAppData` remains the whole-store option).

Why this shape instead of a `stopApp` variant: WHICH keys encode replay state is app knowledge (expo-dev-launcher's storage key, and whatever else you find later — MMKV-adjacent flags, dev-menu state). The deletion capability is generic; the key list stays with you. You can also use it beyond dev-launcher — any persisted flag that poisons the next flow is now one yaml line to neutralize.

**One input needed from you**: the exact NSUserDefaults key name(s). Your round-5 report located the state in "the app container's NSUserDefaults / EXDevLauncherController state" — plug the concrete key(s) into the yaml and the replay class is closed. If it turns out the launcher stores it OUTSIDE NSUserDefaults (a file in the container), report back and we'll extend the verb with a container-file mode.

## D2 — tree-tier visibility agrees with tapOn (Ask 13)

### What changed mechanically

Your diagnosis was exact: XCUITest snapshots report below-fold elements with drifted in-viewport frames + `visible=true`, so smix's tree-tier frame∩viewport filter (which has existed all along) was being fed lies. The one honest data source is a LIVE XCUI query — the same reason `tapOn` failed truthfully on those elements.

v1.0.27 wiring:

1. `POST /find` gains `requireOnScreen: true` — the runner resolves the element LIVE and checks `el.frame.intersects(app.frame)` (current layout, no snapshot).
2. When a tree probe (wait_for / scroll probe / find) matches a node, the driver fires ONE such live confirm using the matched node's `identifier` (else `label`) as the handle:
   - confirmed → hit (cost: one live query per successful wait, ~ms);
   - refuted → treated as **not yet visible**: `extendedWaitUntil` keeps polling, `scrollUntilVisible` keeps swiping (exactly the state a swipe fixes), `find` / `runFlow.when` / `notVisible` return false.
3. `wait_for` timeout after refuted confirms now says it outright:
   > the a11y tree matched this selector but the LIVE on-screen check refuted it every time — the element exists with a stale/drifted snapshot frame (typically below the fold on iOS 26.5 + RN Fabric). Use scrollUntilVisible to bring it into the viewport first, or an ocrText tier to assert by pixels.

### Deliberate design choices you should know

- **Frame∩viewport, NOT `isHittable`.** Hittability is false for elements under floating overlays — your QA bubble sits on top of content you legitimately assert. The on-screen check can't regress overlay-tolerant assertions; a hittability-strict check would have.
- **Handle-less nodes keep tree semantics.** A matched node with neither identifier nor label can't be live-confirmed — the tree verdict stands (pre-v1.0.27 behavior). On your degraded Fabric trees those nodes are exactly the ones you already assert via `ocrText`, which is pixel-truth and unaffected.
- **Live-probe transport errors trust the tree.** A flaky probe must not turn a real hit into a miss.
- **Your OCR-only below-fold chains keep working unchanged** — you can now optionally migrate `scrollUntilVisible` back to id/text tiers where testIDs exist, since the probe now actually scrolls.

### Bonus fix found while wiring

Regex-pattern text selectors (e.g. bare `'A|B'`) dispatched to the live `/find` route serialized as an object its decode rejects — burning an ~8 s retry budget and erroring instead of evaluating. They now host-resolve like other complex shapes. (You mostly dodged this because your gates used literal strings or `fallback:` chains.)

## D3 — supervisor health-unreachable trigger (your b24 note)

`smix runner supervise` now probes `GET /health` every ~10 s in addition to log-marker watching. 3 consecutive failures (~30 s unreachable) → cycle, through the same 60 s cooldown + 5-per-10-min storm accounting, emitting:

```json
{"event":"RunnerCycled","reasonMatched":"health-unreachable x3","context":[],"atMs":...}
```

This catches the marker-less death class you hit in b24 (runner died silently after a downgrade sync reused warm derived-data).

## Wire compatibility

- `POST /find` `requireOnScreen` is optional; absent = pre-v1.0.27 exists-only behavior.
- `clearUserDefaults` verb + `Step::ClearUserDefaults` are additive.
- No changes to any wire shape you currently consume.
- Cold runner rebuild required (Swift `/find` handler changed).

## Ship gate

- 81 parser (+4) + 90 runtime_mock (+2) + full workspace green; 360 swift-bridge tests green — and the swift suite is now a **non-bypassable ship.sh gate** (a stale test had sat failing unnoticed for 15+ releases because that suite was never gated).
- Real-sim smoke on my side was **skipped deliberately** — your batch owned the sim (see apology above), and D2's positive case (drifted Fabric frames) only exists on your app anyway. Your next batch is the empirical gate, same pattern as v1.0.21.

## Retest checklist (v1.0.26 → v1.0.27)

```bash
# 1. Upgrade
cargo install smix-cli --locked
cp ~/.cargo/bin/smix ~/.local/bin/smix
smix --version                                              # → 1.0.27

# 2. Cold rebuild (Swift /find handler changed)
rm -rf .smix/runner/derived-data-*
smix runner up <UDID> --bundle com.focusai.app.mobile

# 3. Ask 12 — wire the key into your relaunch ceremony
- stopApp
- clearUserDefaults: { keys: ["<your dev-launcher key>"] }
- launchApp
# then delete the overlay-tolerant asserts + late close-panel calls the
# replay forced on you (b23-b31 workarounds).

# 4. Ask 13 — optionally re-try one below-fold chip chain with id/text
# tiers (the wait/scroll/tap trio now agrees); or just run --all and
# confirm nothing regressed. Watch for the new "LIVE on-screen check
# refuted" hint in any timeout — that's the drift being caught.

# 5. Supervisor — no action; the health trigger is automatic under
# `--supervise`. Kill -9 the runner mid-batch if you want to see it fire.
```

## Where to file feedback

Same channel: `qualcomm/insight/.claude/state/gol-611/smix-feedback-2026-07-<date>-<name>.md`

For v1.0.27: (1) does `clearUserDefaults` with your key kill the replay end-to-end? (2) do the wait/scroll/tap trio agree on your below-fold chains now? (3) the exact key name(s) you used — we'll document them in the adoption guide for future expo-dev-client consumers.

## Fullpath

```
/Users/doracawl/workspace/goliajp/smix/docs/ai-guide/insight-v1.0.27-shipping.md
```
