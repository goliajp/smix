# 01 — Quickstart

> Goal: from a fresh repo, get a passing YAML run in under 10 minutes on both iOS and Android.

## Prerequisites

| Need | iOS | Android |
|---|---|---|
| OS | macOS (Xcode required) | macOS or Linux (with Android SDK) |
| Tools | Xcode 16+, `xcrun simctl` | `adb`, `emulator`, SDK cmdline-tools |
| Sim/emulator | iOS Simulator (installed with Xcode) | AVD (any name; refer to it by that name) |
| Rust | only to build from source | same |
| Bun | 1.x (only if running the web UI; not for YAML flows) | n/a |

## Get the binaries

```bash
npm install -g @goliapkg/smix-cli    # or: cargo install smix-cli --locked
smix --version
```

Building from source works too, and is what you want when changing smix
itself:

```bash
cd /path/to/smix
cargo build --release   # ~30s cold, <5s warm
ls target/release/smix
```

### If you are scripting smix, resolve it from PATH

The three lines above install to three different places — npm's global
prefix, cargo's `~/.cargo/bin`, and a build tree — and none of them is
"where smix lives". A consumer's runner hard-coded `~/.local/bin/smix`,
the machine had the npm one, and the whole line failed with `smix not
found` before a single flow ran.

```bash
SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"
[ -n "$SMIX_BIN" ] || { echo "smix is not on PATH" >&2; exit 1; }
```

Keep the `SMIX_BIN` override: it is how you point a script at a build
tree without reinstalling, and it is what smix's own gates use.

**A gate in your own tree should pass its build, not whatever is on
PATH.** The two are different questions and the difference is a released
version: running a gate bare here once tested the globally installed
6.8.0 while the change under test sat in `target/release`. The gate said
green about a binary nobody was changing.

One binary, one product: `smix`. All subcommands (boot, runner, run a flow, low-level probes) live under it.

## Start here: a dedicated device for your project

The one command to reach for first is `smix init` — it registers a device
**for this project**, derives an alias from the project directory, and records
it as the project's default, so `smix run` here needs no `--device`:

```bash
smix init --device <UDID> --app ./YourApp.app   # register this project's device + install
smix run examples/hello.yaml                     # no --device: resolves the project's default
```

Everything below is the same steps by hand, when you want the low-level control
`smix init` wraps.

## iOS path: boot → run → teardown

```bash
# 1. Discover or pick a simulator UDID
smix sim list   # UDID / name / state / runtime for every sim

# 2. Boot it (alias or UDID both work)
smix sim boot <device>
# → booted: 5D087114-ECB3-443C-...

# 3. Bring up the smix runner (XCUITest server)
smix runner up 5D087114-ECB3-443C-... --bundle com.example.YourApp
# → runner up: http://localhost:22087/health = 200

# 4. Install your app under test (build it with your usual toolchain first)
smix sim exec <device> install /path/to/YourApp.app

# 5. Run a YAML flow
smix run \
  --platform ios \
  --device 5D087114-ECB3-443C-... \
  --runner-port 22087 \
  --no-launch \
  examples/hello.yaml

# 6. Teardown
smix runner down
```

If XCUITest processes remain after teardown, kill any residual `xctrunner` / `xcodebuild` processes before starting a new run.

## Android path: boot → run → teardown

```bash
# 1. Start the emulator
emulator @<avd-name> -no-audio -no-snapshot \
  -no-boot-anim -port 5554 \
  -skin 1080x2340 &

# Wait for boot
until adb -s emulator-5554 shell getprop sys.boot_completed 2>/dev/null | grep -q 1; do sleep 5; done

# 2. Build + install the app under test
adb -s emulator-5554 install -r /path/to/app-debug.apk

# 3. Build + start the smix runner (installs, forwards, instruments,
#    and blocks until /health answers)
(cd android-runner && ./gradlew :app:assembleDebugAndroidTest)
smix runner up emulator-5554 --platform android

# Only if you use the WebView eval bridge:
adb -s emulator-5554 forward tcp:28081 tcp:28081

# 4. Launch the app under test
adb -s emulator-5554 shell am start -n com.example.app/.MainActivity

# 5. Run a YAML flow
smix run \
  --platform android \
  --apps-config apps.yaml \
  --device emulator-5554 \
  --runner-port 28080 \
  --no-launch \
  examples/hello.yaml

# 6. Teardown
smix runner down --platform android --device emulator-5554
adb -s emulator-5554 shell am force-stop com.example.app
adb -s emulator-5554 emu kill
```

## Common first-run errors

| Symptom | Cause | Fix |
|---|---|---|
| `simctl io ... is retired` | You called bare `xcrun simctl`. The sim safety hook is active. | Use `smix sim ...` or `smix sim exec <udid> ...` instead. |
| `runner /health` 502 / connection refused | Runner did not finish coming up. | Wait 3–5s after `smix runner up`, re-curl /health, or run `smix runner list` — it names every runner on this machine, its port, and whether the ledgers know about it. |
| `tap_by_id: element not found` | Element off-screen (LazyRow / horizontal-scroll tab bar) | Scroll first (`adb shell input swipe`) or use a different selector. See [03-selectors.md](03-selectors.md). |
| iOS build fails with `Cannot find 'FooScreen' in scope` | You added a new file to your Xcode project but the `.pbxproj` wasn't updated. | Add PBXBuildFile + PBXFileReference + PBXGroup + PBXSourcesBuildPhase entries. |
| `webview_eval: runner webview-bridge unreachable (501)` | WebView shim not started | Navigate to the WebView-hosting screen at least once before exercising `webview_eval`, and ensure `adb forward tcp:28081` ran. |

## Where to go next

- Want to **write** a YAML? → [02-yaml-reference.md](02-yaml-reference.md)
- Need to **find an element** by some property other than testTag? → [03-selectors.md](03-selectors.md)
- Want a reference for how to organize testTags in your test app? → [06-fixtures.md](06-fixtures.md)

---

Driving a physical iPhone or Android device works the same way once the
device is registered — see [05-cli.md — Physical
devices](./05-cli.md#physical-devices) for registration, signing, and
what a phone cannot do.
