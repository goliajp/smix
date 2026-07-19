# 01 — Quickstart

> Goal: from a fresh repo, get a passing YAML run in under 10 minutes on both iOS and Android.

## Prerequisites

| Need | iOS | Android |
|---|---|---|
| OS | macOS (Xcode required) | macOS or Linux (with Android SDK) |
| Tools | Xcode 16+, `xcrun simctl` | `adb`, `emulator`, SDK cmdline-tools |
| Sim/emulator | iOS Simulator (installed with Xcode) | AVD (any name; refer to it by that name) |
| Rust | 1.79+ via `rustup` | same |
| Bun | 1.x (only if running the web UI; not for YAML flows) | n/a |

## Build the binaries

```bash
cd /path/to/smix
cargo build --release   # ~30s cold, <5s warm
ls target/release/smix
```

One binary, one product: `smix`. All subcommands (boot, runner, run a flow, low-level probes) live under it.

## iOS path: boot → run → teardown

```bash
# 1. Discover or pick a simulator UDID
smix sim list   # prints registry aliases + boot state

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
  flows/00_smoke.yaml

# 6. Teardown
smix runner down 5D087114-ECB3-443C-...
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

# 3. Install + start the smix-android-runner instrumentation
adb -s emulator-5554 install -r /path/to/smix-runner-androidTest.apk
adb -s emulator-5554 forward tcp:28080 tcp:28080
adb -s emulator-5554 forward tcp:28081 tcp:28081   # only if you use the WebView eval bridge
adb -s emulator-5554 shell am instrument -w -e debug false \
  -e class com.example.runner.RunnerTest \
  com.example.runner/androidx.test.runner.AndroidJUnitRunner \
  > /tmp/smix-runner-instrument.log 2>&1 &

# Wait for runner /health
until curl -sf http://127.0.0.1:28080/health >/dev/null; do sleep 1; done

# 4. Launch the app under test
adb -s emulator-5554 shell am start -n com.example.app/.MainActivity

# 5. Run a YAML flow
smix run \
  --platform android \
  --apps-config apps.yaml \
  --device emulator-5554 \
  --runner-port 28080 \
  --no-launch \
  flows/01_home_counter.yaml

# 6. Teardown
adb -s emulator-5554 shell am force-stop com.example.app
adb -s emulator-5554 shell am force-stop com.example.runner
adb -s emulator-5554 emu kill
```

## Common first-run errors

| Symptom | Cause | Fix |
|---|---|---|
| `simctl io ... is retired` | You called bare `xcrun simctl`. The sim safety hook is active. | Use `smix sim ...` or `smix sim exec <udid> ...` instead. |
| `runner /health` 502 / connection refused | Runner did not finish coming up. | Wait 3–5s after `smix runner up`, re-curl /health, or check `pgrep -fl xctrunner`. |
| `tap_by_id: element not found` | Element off-screen (LazyRow / horizontal-scroll tab bar) | Scroll first (`adb shell input swipe`) or use a different selector. See [03-selectors.md](03-selectors.md). |
| iOS build fails with `Cannot find 'FooScreen' in scope` | You added a new file to your Xcode project but the `.pbxproj` wasn't updated. | Add PBXBuildFile + PBXFileReference + PBXGroup + PBXSourcesBuildPhase entries. |
| `webview_eval: runner webview-bridge unreachable (501)` | WebView shim not started | Navigate to the WebView-hosting screen at least once before exercising `webview_eval`, and ensure `adb forward tcp:28081` ran. |

## Where to go next

- Want to **write** a YAML? → [02-yaml-reference.md](02-yaml-reference.md)
- Need to **find an element** by some property other than testTag? → [03-selectors.md](03-selectors.md)
- Want a reference for how to organize testTags in your test app? → [06-fixtures.md](06-fixtures.md)
