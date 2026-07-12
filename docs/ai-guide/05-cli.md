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

`<ALIAS>` resolves via `.smix/sims.json` registry, which is populated by `smix selftest single/multi` or by hand. UDIDs always work.

### Runner management

```bash
smix runner up <UDID>          # start XCUITest server + block until /health
smix runner up <UDID> --port 22087 --soft --no-capture
smix runner down <UDID>        # stop & cleanup
```

`--soft` skips shutdown when the sim is already booted. `--no-capture` skips background screen recording (faster startup).

### Doctor (housekeeping)

```bash
smix doctor                    # health probe: xcrun, simctl, sims, runners
smix down                      # tear down all smix-owned processes (runner, capture)
```

If XCUITest / xcodebuild processes remain after teardown, use `pgrep -fl xctrunner` / `pkill xctrunner` (or the equivalent for `xcodebuild`) to clear them.

### Low-level probes (use a running runner)

These act on the runner's currently-active sim. Used for ad-hoc debugging without writing YAML.

```bash
smix tap --selector-id home-increment-btn
smix tap --selector-text "Submit"
smix tap --selector-coord 0.5,0.8

smix find --selector-id home-counter-label      # exit 0 if found, non-zero otherwise
smix wait-for --selector-id loading-spinner --timeout 5000 --until-absent

smix fill --selector-id form-email-input --text alice@example.com
smix press-key --key ENTER
smix scroll --selector-text "Row #5000" --direction DOWN
smix hide-keyboard

smix tree --json | jq .                   # full a11y tree
smix tree                                  # human-readable outline
smix describe                              # high-level ScreenDescription
smix system-popups                         # list active popups (camera permission, etc.)
```

### Run-script driver (sequential YAML of `smix` subcommands)

```yaml
# script.yaml
- name: boot
  cmd: sim boot
  args: [<device>]
- name: runner
  cmd: runner up
  args: [{from: outputs.boot.udid}, --soft, --no-capture]
- name: tap-home
  cmd: tap
  args: [--selector-id, tab-home]
```

```bash
smix run-script script.yaml
```

Use for matrix runs, repeated probes, anything that needs sequential CLI calls. Not for app UI flows — use `smix run` for that.

### Selftest

```bash
smix selftest single           # one sim, full capability matrix
smix selftest multi            # multiple sims concurrent, drift detection
```

Used by smix's own CI. Most external users will not run these.

## Common command recipes

### Smoke test the running sim quickly

```bash
smix find --selector-text "Welcome" || echo "not on welcome screen"
smix tree | head -50   # see what's on screen right now
smix system-popups     # check for blocking system alerts
```

### Reset to known state between tests

```bash
smix sim exec <device> terminate com.example.app
smix sim exec <device> launch com.example.app
sleep 2
smix find --selector-id home-container   # confirm fresh launch
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
3. .smix/sims.json registry (for aliases)
4. Hardcoded default in code (lowest)
```
