#!/usr/bin/env bash
# v15.1-C9: an app iOS 27 stops at launch for not adopting UIScene is named.
#
# iOS 27 ends an app that builds its window in the app delegate, at launch,
# in `__UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`. smix
# said only "Application … is not running". 14.1 reads the crash report the
# launch left and says why; that was tested on a saved report, and this is
# the run on a device: a minimal app with no scene manifest, built here,
# installed, and `runner up` for it.
#
# The crash leaves a macOS crash report dialog on the desktop of whoever
# is at the machine, and nothing short of a machine-wide setting keeps it
# away. So it runs only when a person names it, SMIX_E2E_CRASH_DIALOG=1,
# and is there to close the dialog; a batch (the release tier) never does.
# Reading the cause out of a report is covered without a device by the
# saved-report tests.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_C9_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="dev.smix.noscene"
PROJECT="${SMIX_C9_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
WORK="$(mktemp -d)"

log()  { printf '[v15.1-c9] %s\n' "$*" >&2; }
fail() { printf '[v15.1-c9] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v15.1-c9] cannot judge: %s\n' "$*" >&2; exit 2; }

[ "${SMIX_E2E_CRASH_DIALOG:-}" = 1 ] \
  || cannot_judge "it crashes an app on purpose, which puts a crash report dialog on this Mac's desktop; set SMIX_E2E_CRASH_DIALOG=1 to run it while someone is there to close it"

UDID="" WE_BOOTED=0 WE_INSTALLED=0
cleanup() {
  if [ "$WE_INSTALLED" = 1 ]; then xcrun simctl uninstall "$UDID" "$APPID" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
# The app: a delegate that makes its own window, and no scene manifest.
APP="$WORK/NoScene.app"
mkdir -p "$APP"
cat >"$WORK/main.swift" <<'SWIFT'
import UIKit

final class Delegate: UIResponder, UIApplicationDelegate {
  var window: UIWindow?
  func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    let window = UIWindow(frame: UIScreen.main.bounds)
    window.rootViewController = UIViewController()
    window.makeKeyAndVisible()
    self.window = window
    return true
  }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(Delegate.self))
SWIFT
SDK="$(xcrun --sdk iphonesimulator --show-sdk-path)"
xcrun --sdk iphonesimulator swiftc -target "$(uname -m)-apple-ios17.0-simulator" -sdk "$SDK" \
  -o "$APP/NoScene" "$WORK/main.swift" >"$WORK/build.log" 2>&1 || fail "swiftc: $(tail -3 "$WORK/build.log")"
cat >"$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>NoScene</string>
  <key>CFBundleIdentifier</key><string>$APPID</string>
  <key>CFBundleName</key><string>NoScene</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>MinimumOSVersion</key><string>17.0</string>
</dict>
</plist>
PLIST

if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
runtime="$(xcrun simctl getenv "$UDID" SIMULATOR_RUNTIME_VERSION 2>/dev/null || true)"
case "$runtime" in 27.*) : ;; *) cannot_judge "$IOS_ALIAS runs iOS '${runtime:-?}'; the stop at launch is iOS 27's" ;; esac
"$SMIX" sim install "$UDID" "$APP" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
WE_INSTALLED=1

rc=0
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || { tail -10 "$WORK/up.log" >&2; fail "runner up succeeded for an app iOS 27 stops at launch"; }
grep -q "does not adopt the scene lifecycle" "$WORK/up.log" \
  || { tail -20 "$WORK/up.log" >&2; fail "the refusal does not say the app does not adopt the scene lifecycle"; }
report="$(grep -o 'Crash report: [^ ]*\.ips' "$WORK/up.log" | head -1 | sed 's/^Crash report: //')"
[ -n "$report" ] && [ -f "$report" ] || { tail -20 "$WORK/up.log" >&2; fail "the refusal does not name a crash report that exists"; }
log "runner up refused (exit $rc): names the scene lifecycle and $report"
log "V15.1-C9-E2E-PASS"
