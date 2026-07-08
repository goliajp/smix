# smix-android-runner

> Android-side counterpart of the swift `smix-runner`. Mirror of the
> XCTest-based iOS runner: an instrumentation test target launched
> via `adb shell am instrument` that boots an embedded HTTP server
> on `0.0.0.0:28080` + holds the process open while the host-side
> Rust driver (`smix_driver::AndroidDriver`) dispatches over
> adb-forwarded port 28080.

## Build (requires Android SDK + Java 17)

```bash
export ANDROID_HOME=$HOME/Library/Android/sdk
export JAVA_HOME=$(/usr/libexec/java_home -v 17)

cd android-runner

# Bootstrap gradle wrapper if not present (one-time).
gradle wrapper --gradle-version 9.3.1

# Build instrumentation test APK (no emulator required).
./gradlew :app:assembleDebugAndroidTest

# Lint static analysis on the build files.
./gradlew :app:lint
```

## Emulator visibility

Smoke scripts boot the emulator **with the visible window by default** so
you can see what's happening. To run headless (CI / batch), set
`EMU_VISIBLE_FLAGS=-no-window`:

```bash
EMU_VISIBLE_FLAGS=-no-window bash android-runner/scripts/smoke-tree.sh
```

## End-to-end smoke (requires emulator)

```bash
# Boot emulator (pin id + name explicitly).
emulator @<avd-name> -id <avd-name> &

# Install runner APK + test APK.
adb -s emulator-5554 install -r app/build/outputs/apk/debug/app-debug.apk
adb -s emulator-5554 install -r app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk

# Start runner.
adb -s emulator-5554 forward tcp:28080 tcp:28080
adb -s emulator-5554 shell am instrument -w -e class dev.smix.runner.RunnerTest \
  dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner &

# Probe /health from host.
curl http://127.0.0.1:28080/health
# expect: {"status":"ok","runner":"smix-android-runner", ...}

# Teardown.
adb -s emulator-5554 shell am force-stop dev.smix.runner.test
adb -s emulator-5554 forward --remove tcp:28080
```
