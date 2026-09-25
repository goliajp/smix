#!/usr/bin/env bash
# The devices the e2e scripts drive when their caller names none.
#
# Source this, do not run it. Each script still takes its own override
# variable (SMIX_C5_ANDROID, SMIX_SMOKE_IOS, …) — that is how a caller points
# one script elsewhere. What lives here is the default, once: nineteen
# scripts used to write `sim-smix-android-01` into themselves, so moving the
# suite to another device was nineteen edits, and the release had to export
# nine variables to keep them off an AVD somebody else was using.
#
# SMIX_E2E_ANDROID / SMIX_E2E_IOS point the whole suite at once.
# E2E_ANDROID_SECOND is the other smix AVD, for the scripts whose subject
# is two emulators. E2E_IOS_SECOND is the iOS counterpart: `sim-smix-03`,
# by UDID because it is not registered — registering it would write to
# this machine's device registry, which consumers' rows share.
E2E_ANDROID="${SMIX_E2E_ANDROID:-sim-smix-android-01}"
E2E_ANDROID_SECOND="${SMIX_E2E_ANDROID_SECOND:-sim-smix-android-02}"
E2E_IOS="${SMIX_E2E_IOS:-sim-smix-02}"
E2E_IOS_SECOND="${SMIX_E2E_IOS_SECOND:-89980B43-EF26-446A-A897-848C1AD3A872}"

# ---- physical devices: never by default -------------------------------
#
# On 2026-09-25 a release dry-run uninstalled smix's runner from the
# owner's Samsung, screenshotted their iPhone through a runner somebody
# had up on it, and opened a usbmux tunnel to it. Every script involved
# found a device "attached" and "registered" and took that as permission
# — and the registrations had been written into the machine's ledger by
# the same suite. Neither word says whose a device is.
#
# So a script reaches a phone only when a person named one:
#
#   SMIX_E2E_PHYSICAL_ANDROID=<serial or alias>
#   SMIX_E2E_PHYSICAL_IOS=<udid or alias>
#
# Nothing here looks at the bus. `device-e2e-tier.sh` and `ship.sh` clear
# both, so a release never runs a physical leg, whatever is plugged in.
# Guarded by `an-e2e-leaves-the-phones-alone`.

# e2e_physical_or_skip <android|ios> <tag> → sets E2E_PHYSICAL to the
# named device, or says why it will not look and exits the script 2
# (could not judge). Called directly, never inside `$( )` — an exit in a
# subshell ends the subshell, and the script would carry on.
e2e_physical_or_skip() {
  local kind="$1" tag="$2" named="" var
  case "$kind" in
    android) named="${SMIX_E2E_PHYSICAL_ANDROID:-}"; var=SMIX_E2E_PHYSICAL_ANDROID ;;
    ios) named="${SMIX_E2E_PHYSICAL_IOS:-}"; var=SMIX_E2E_PHYSICAL_IOS ;;
    *) printf '[%s] e2e_physical_or_skip: unknown kind %s\n' "$tag" "$kind" >&2; exit 1 ;;
  esac
  if [ -z "$named" ]; then
    printf '[%s] no physical %s device was named, so none is touched — being attached\n' "$tag" "$kind" >&2
    printf '[%s] or registered is not permission. Set %s to run this leg.\n' "$tag" "$var" >&2
    exit 2
  fi
  E2E_PHYSICAL="$named"
}

# e2e_isolate_machine [parent] → exports SMIX_MACHINE_DIR as a fresh
# directory (under `parent` when given, which the caller already removes
# on exit). Called directly, not inside `$( )`, for the same reason. The
# real ledger is read by every smix on this machine, consumers' included;
# a test that registers a device, lifts a destructive guard or
# unregisters one does it here.
e2e_isolate_machine() {
  local parent="${1:-}"
  if [ -n "$parent" ]; then
    SMIX_MACHINE_DIR="$parent/machine"
    mkdir -p "$SMIX_MACHINE_DIR"
  else
    SMIX_MACHINE_DIR="$(mktemp -d)"
  fi
  export SMIX_MACHINE_DIR
}

# Identifiers with the shape of a device and no device behind them. An
# Apple UDID whose sixteen-digit tail starts with ten zeros, an Android
# serial that starts with FAKE: the gate accepts only these as literals.
E2E_FAKE_IOS_UDID="00008120-0000000000C0FFEE"
E2E_FAKE_IOS_UDID_LOWER="00008120-0000000000c0ffee"
E2E_FAKE_ANDROID_SERIAL="FAKE0SERIAL01"

# e2e_ledger_path <device> → the file this machine keeps that device's
# ledger in, as smix answers it (`lease status --json`). Scripts used to
# build the path: three from the checkout's `.smix/leases/`, which stopped
# being written in 4.0 and still held a file from August, so they read a
# stale row and failed on a recording that was running; one from
# `$HOME/.local/share/smix`, which is not where the ledger is under
# SMIX_MACHINE_DIR or XDG_DATA_HOME. Needs $SMIX (scripts/lib/e2e-binary.sh).
e2e_ledger_path() {
  "$SMIX" lease status "$1" --json | python3 -c 'import json, sys; print(json.load(sys.stdin)["path"])'
}
