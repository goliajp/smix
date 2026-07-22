# 05 — CLI reference

> Every subcommand + flag of the `smix` binary. Reference, not tutorial — for tutorial see [01-quickstart.md](01-quickstart.md).

## `smix run` — execute a flow YAML

### `smix run <YAML_FILE>` — run a flow

```bash
smix run [FLAGS] <FLOW.yaml>
```

| Flag | Env | Default | Description |
|---|---|---|---|
| `--device <DEVICE>` | `SMIX_UDID` | (none, required) | Target simulator UDID or Android device id (e.g. `emulator-5554`) |
| `--bundle-id <ID>` | `SMIX_BUNDLE_ID` | (unset) | Default app under test (overrideable by `appId:` in YAML) |
| `--runner-port <PORT>` | `SMIX_RUNNER_PORT` | `22087` | Runner HTTP port. iOS = 22087, Android = 28080 by convention |
| `--platform <ios\|android>` | `SMIX_PLATFORM` | `ios` | Target platform |
| `--apps-config <PATH>` | `SMIX_APPS_CONFIG` | (unset) | Path to `apps.yaml` for cross-platform `app:` logical key resolution |
| `--no-launch` | — | (off) | Skip initial `foreground()` call. Use when you launched the app via other means |
| `--trace-dir <PATH>` | — | `./.smix/trace/<runid>` | Where screenshots / video / JSON traces go |
| `--dry-run` (alias `--check`) | — | (off) | Parse-only gate: validates every listed yaml (+ `runFlow:` includes) and reports `parse OK/FAIL` per file with step counts. No runner, no simulator. Exit 0 on clean parse, 2 on any error |
| `--retry <N>` | — | `1` | Per-flow attempt count; attempts recorded in `~/.local/share/smix/flow-attempts.json` for `smix diagnostic dump` attribution |
| `--debug-output <DIR>` | — | (unset) | Per-step JSON + on-fail screenshot/tree artifacts |

Environment variables consumed by `smix run` (beyond the flag-bound ones above):

| Env | Default | Description |
|---|---|---|
| `SMIX_WEBVIEW_BRIDGE_PORT` | `28080` | Port of the in-app `SmixWebViewBridge` that `webviewEval` posts to on iOS. Infrastructure port like `SMIX_RUNNER_PORT`, not one of the behaviour switches below — it has no `.smix/config.yaml` equivalent |
| `SMIX_AUTO_OCR_FALLBACK` | (off) | `1`/`true`/`yes` — auto-lift bare-string selectors to `fallback: [text, ocrText]` at parse time; `A\|B` regex-OR strings split per alternative on the OCR tiers |
| `SMIX_TAP_OCR_POLL_MS` | `3000` | Poll budget for `tapOn` fallback chains that contain `ocrText` (250 ms cadence) |

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 2 | YAML parse error (`RunError::Parse`) |
| 3 | runtime SDK failure (`RunError::Sdk`) — sim crashed, app died, etc. |
| 4 | unknown key / direction (`RunError::UnknownKey` / `RunError::UnknownDirection`) |
| 5 | runFlow cycle / file IO (`RunError::RunFlowCycle` / `RunError::Io`) |
| 6 | runner unreachable (runner not up / port wrong) |

### Output formats

- stdout has step-by-step progress + final summary JSON
- stderr has DEBUG / WARN lines (when `RUST_LOG=info` or higher)
- Final line is `summary: N steps, X warnings, Y skipped, Z expanded subflows` on success

## `smix` — environment + low-level

### Sim management (`smix sim …`)

```bash
smix sim list                  # all sims (JSON: --json)
smix sim resolve <ALIAS>       # alias → UDID
smix sim boot <ALIAS|UDID>     # boot
smix sim shutdown <ALIAS|UDID> # shutdown
smix sim erase <ALIAS|UDID>    # wipe (reset content)
smix sim screenshot <ALIAS|UDID> <out.png>
smix sim launch <ALIAS|UDID> <bundle-id>
smix sim terminate <ALIAS|UDID> <bundle-id>
smix sim install <ALIAS|UDID> <path/to.app>
smix sim uninstall <ALIAS|UDID> <bundle-id>
smix sim openurl <ALIAS|UDID> <url>
smix sim appearance <ALIAS|UDID> light|dark
smix sim keychain-reset <ALIAS|UDID>
smix sim exec <ALIAS|UDID> <verb> [args...]
```

**Sim safety hook**: bare `xcrun simctl <verb>` is BLOCKED for mutating verbs. Read-only `simctl list` is allowed. To pass-through unmapped subcommands use `smix sim exec <ALIAS|UDID> <verb> ...` (this is accepted by the hook because the device id is explicit).

`<ALIAS>` resolves via the workspace's `.smix/` registry, created and populated by `smix sim register <alias> --udid <UDID>`. UDIDs always work without a registry. (A pre-2.1 `.smix/sims.json` is imported on first use and left on disk.)

### Runner management

The runner is the on-device server smix drives. `runner up` blocks until
its `/health` answers, so when the command returns you can run a flow.

**iOS** — drives `xcodebuild` against the XCUITest runner:

```bash
smix runner up <ALIAS|UDID> --bundle com.example.app
smix runner up <ALIAS|UDID> --bundle com.example.app --supervise
smix runner down
```

`--bundle` is required: it binds the runner's `XCUIApplication`, and
without it every `/tree` reads the wrong app. `--supervise` attaches a
sidecar that re-cycles the runner if it dies; `runner down` cascades to
it. Port comes from `--runner-port`, else the alias's `runnerPort` in
the registry, else 22087.

**Android** — installs the instrumentation APK, forwards the port, and
`am instrument`s the Kotlin runner:

```bash
cd android-runner && ./gradlew :app:assembleDebugAndroidTest   # once, to build it
smix runner up emulator-5554 --platform android
smix runner down --platform android --device emulator-5554
```

The device is the adb serial — there is no registry indirection on this
path. Default port is 28080. `runner up` is idempotent: if `/health`
already answers on that port it says so and returns rather than stacking
a second instrumentation onto it.

`runner down --platform android` requires `--device`, because an adb
command without a serial acts on whichever device happens to be
attached — and a developer's own phone is often plugged in next to the
emulator. Every adb call smix makes names its device explicitly for the
same reason; note that `gradlew install*` does **not**, so prefer these
commands over gradle install tasks.

Bringing the Android runner up by hand, if you need to:

```bash
adb -s emulator-5554 install -r -t \
  android-runner/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb -s emulator-5554 forward tcp:28080 tcp:28080
adb -s emulator-5554 shell am instrument -w \
  -e class dev.smix.runner.RunnerTest#runServerForever \
  dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner
```

A typo in any of those three coordinates produces `OK (0 tests)` — a
silent no-op that reads as success.

### Doctor (housekeeping)

```bash
smix doctor                    # health probe: xcrun, simctl, sims, runners
smix down                      # tear down all smix-owned processes (runner, capture)
```

If XCUITest / xcodebuild processes remain after teardown, use `pgrep -fl xctrunner` / `pkill xctrunner` (or the equivalent for `xcodebuild`) to clear them.

### Low-level probes (use a running runner)

These act on the runner's currently-active sim. Used for ad-hoc debugging without writing YAML.

The selector is a positional argument in `<kind>:<value>` shorthand —
`id:` / `text:` / `label:` / `role:`.

```bash
smix tap id:home-increment-btn
smix tap "text:Submit"

smix find id:home-counter-label            # prints exists=<bool>
smix wait-for id:loading-spinner --timeout 5    # seconds, not ms

smix fill id:form-email-input --text alice@example.com
smix press-key return                      # positional key name
smix scroll "text:Row #5000" --direction down
smix hide-keyboard

smix tree --json | jq .                    # full a11y tree
smix tree                                  # human-readable outline
smix describe                              # visible interactive elements
smix system-popups                         # list active popups
smix system-popup-action <popup-id> <button-id>
```

Every one of these takes `--port` and `--device`, and resolves the port
by the precedence at the bottom of this page. `--device` is narrower
here than on `smix run`: it names a UDID or a registry alias purely so
the port that device is registered on can be looked up. It does not
change which simulator the call reaches — the port already does that,
because a runner is a process listening on one.

```bash
smix sim register jp --udid <UDID> --runner-port 22088
smix tap id:home-tab --device jp           # dials 22088, not 22087
```

### Run-script driver (sequential YAML of `smix` subcommands)

```yaml
# script.yaml
- name: boot
  cmd: sim boot
  args: [<device>]
- name: runner
  cmd: runner up
  args: [{from: outputs.boot.udid}, --bundle, com.example.app]
- name: tap-home
  cmd: tap
  args: ["id:tab-home"]
```

```bash
smix run-script script.yaml
```

Use for matrix runs, repeated probes, anything that needs sequential CLI calls. Not for app UI flows — use `smix run` for that.

### Device registry

```bash
smix sim list                                # find the UDID
smix sim register dev --udid <UDID>          # record it under an alias
smix sim register jp --udid <UDID> --locale ja-JP --runner-port 22088
```

`register` creates the `.smix/` registry when absent (walking up from the
working directory; `SMIX_SIMS_JSON` overrides the location). Device
name, runtime, and device type are read from `simctl`, so only the UDID
and alias are yours to choose. After this, every command accepts the
alias where it accepts a UDID.

Registry file shape, for editing by hand:

```json
{
  "version": 1,
  "sims": {
    "dev": {
      "deviceName": "iPhone 17 Pro",
      "udid": "47ACEAE5-36BA-4C62-811B-F09B397910D7",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
      "locale": "ja-JP",
      "runnerPort": 22088
    }
  }
}
```

`locale` and `runnerPort` are optional.

## Migration and authoring

### `smix migrate` — maestro yaml to smix

```bash
smix migrate flow.yaml              # rewritten yaml to stdout
smix migrate flows/                 # every .yaml under a directory
```

Renames verbs to their smix spellings (`tapOn` → `tap`,
`extendedWaitUntil` → `expect` + `timeoutMs`, `retry.max` →
`retry.maxRetries`) and drops deprecated argument forms. A verb it does
not recognise is passed through unchanged with a `WARN:` line on
stderr — migrate never silently drops a step it cannot translate.

It also warns about selector keys v2 refuses (`enabled:` is one), since
those fail at parse time and it is better to hear it here than on the
next run.

### `smix authoring` — compose against a live sim

Suggest selectors matching a partial spec, and capture or diff
accessibility-tree baselines for visual gates. Requires a runner.

### `smix annotate` — draw on a screenshot

Circle, arrow, text, box and line primitives over a PNG, for failure
reports a human or an agent has to read.

### `smix capsule` — one-command bring-up and teardown

Headless boot, capture and runner start together (`up`), and the
reverse (`down`). The guard rejects a windowed session by default;
`--soft` accepts the soft-capsule fallback.

### `smix diagnostic store` — read the persisted state

```bash
smix diagnostic store               # this workspace's .smix
smix diagnostic store --root PATH   # another store
```

Prints everything smix has persisted, as JSON: the device registry,
runner handles, capsule records and the diagnostic buffers. State used
to be JSON files you could `cat`; this is what replaces that. A value
that is not valid JSON is shown as hex rather than stopping the dump,
because this is what you run when something is already wrong.

## Common command recipes

### Smoke test the running sim quickly

```bash
smix find "text:Welcome" || echo "not on welcome screen"
smix tree | head -50   # see what's on screen right now
smix system-popups     # check for blocking system alerts
```

### Reset to known state between tests

```bash
smix sim exec <device> terminate com.example.app
smix sim exec <device> launch com.example.app
sleep 2
smix find id:home-container   # confirm fresh launch
```

### Capture per-step screenshot (debug aid)

```bash
smix sim exec <device> io screenshot /tmp/s.png && open /tmp/s.png
```

### iOS + Android in one terminal

```bash
# iOS port 22087 (default)
smix run --device <iosudid> --runner-port 22087 --no-launch flow.yaml

# Android port 28080
smix run --device emulator-5554 --platform android \
  --apps-config apps.yaml \
  --runner-port 28080 --no-launch flow.yaml
```

## Environment-variable precedence

```
1. Explicit CLI flag (highest)
2. ENV var (e.g. SMIX_UDID)
3. the .smix/ registry (for aliases)
4. Hardcoded default in code (lowest)
```
