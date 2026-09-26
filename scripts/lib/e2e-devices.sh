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
# is two emulators. E2E_ANDROID_THIRD stands in for somebody else's
# emulator (v6.1-c5 starts a read-only instance of it); nothing drives it,
# so it is never the one a release has running. E2E_IOS_SECOND is the iOS counterpart: `sim-smix-03`,
# by UDID because it is not registered — registering it would write to
# this machine's device registry, which consumers' rows share.
E2E_ANDROID="${SMIX_E2E_ANDROID:-sim-smix-android-01}"
E2E_ANDROID_SECOND="${SMIX_E2E_ANDROID_SECOND:-sim-smix-android-02}"
E2E_ANDROID_THIRD="${SMIX_E2E_ANDROID_THIRD:-sim-smix-android-03}"
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

# e2e_start_emulator <log> <emulator args…> — start an emulator by hand, in
# a process group of its own, in the background; `$!` is its launcher.
#
# A plain `emulator … &` joins the script's process group, so whatever
# ends the script's group — a tier's deadline, Ctrl-C, a harness ending a
# command — signals the launcher, which passes it on, and the headless
# qemu aborts in its signal-time quit path: an "Android Emulator quit
# unexpectedly" dialog on the owner's desktop (2026-09-25). macOS has no
# setsid(1), so python makes the group and then becomes the launcher.
e2e_start_emulator() {
  local log="$1"
  shift
  local sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
  python3 -c 'import os, sys; os.setpgid(0, 0); os.execv(sys.argv[1], sys.argv[1:])' \
    "$sdk/emulator/emulator" "$@" >"$log" 2>&1 &
}

# e2e_stop_emulator <serial> [seconds] — the one way this suite stops an
# emulator: `emu kill` through its console, then wait until adb no longer
# lists it in any state. Returns 1, saying so, if it is still listed when
# the wait runs out; nothing further is sent — signalling or killing an
# emulator that is quitting is what leaves the crash dialog behind.
e2e_stop_emulator() {
  local serial="$1" within="${2:-60}" i=0
  adb -s "$serial" emu kill >/dev/null 2>&1 || true
  while adb devices 2>/dev/null | awk -v s="$serial" '$1 == s { f = 1 } END { exit !f }'; do
    i=$((i + 1))
    if [ "$i" -gt $((within * 4)) ]; then
      printf 'e2e_stop_emulator: %s still listed %ss after emu kill; nothing further was sent to it\n' \
        "$serial" "$within" >&2
      return 1
    fi
    sleep 0.25
  done
}

# ---- is a simulator up ------------------------------------------------
#
# simulator_state <UDID> [transport...] → Booted | Shutdown | … | absent
#
# The one place a script learns a simulator's state. Twenty scripts used
# to ask simctl themselves, and one of them grepped for `UDID (Booted)` —
# a string simctl never prints, since the line reads `name (UDID) (Booted)`.
# It always found the device down, recorded that it had booted it, and
# shut the release's simulator down on its way out (2026-09-25, v10.2-c12).
#
# A transport runs the listing somewhere else and the answer is read here:
# `simulator_state "$UDID" rssh` asks the federation node. Booting and
# shutting down stay in each script: teardown-restores-scan reads a
# script's own record of what it booted.
# Guarded by `an-e2e-leaves-the-phones-alone` (rule 10).
simulator_state() {
  local udid="$1"
  shift
  "$@" xcrun simctl list devices -j | python3 -c '
import json, sys
u = sys.argv[1]
print(next((d["state"] for v in json.load(sys.stdin)["devices"].values() for d in v if d["udid"] == u), "absent"))' "$udid"
}

# simulator_name <UDID> → the name simctl lists it under, or nothing.
# Whose a simulator is is read from its name (smix's own are `sim-smix-*`);
# asked here for the same reason its state is.
simulator_name() {
  xcrun simctl list devices -j | python3 -c '
import json, sys
u = sys.argv[1]
print(next((d["name"] for v in json.load(sys.stdin)["devices"].values() for d in v if d["udid"] == u), ""))' "$1"
}

# ---- whose a device is, before driving it ------------------------------
#
# e2e_yield_if_held <device> [<channel> <smix command>]
#
# Ends the script through its own `cannot_judge` when smix's machine
# ledger says somebody holds <device>, naming the holder; returns 0 when
# the device is free. A script whose device cannot be asked about cannot
# judge either — an unanswered question is not a free device.
#
# This replaced `pgrep -f 'runner.ts|smix run|supervise'`, which yielded
# whenever anything smix-shaped ran anywhere on the machine: a consumer's
# batch on their own emulator kept eight of these scripts from ever
# running (2026-09-26), though they drive our devices on ports of their
# own. The ledger is where a claim on a device is recorded and where
# every smix is refused one — the same question, asked of the one place
# that knows.
#
# Remote: pass the channel and the command that runs smix there, e.g.
#   e2e_yield_if_held "$UDID_M" rssh "cd '$REMOTE_REPO' && target/release/smix"
e2e_yield_if_held() {
  local device="$1" out err held rc
  err="$(mktemp)"
  # stdout alone is the JSON: a line smix writes on stderr (a store
  # warning, a note about an old book) is not part of the answer, and read
  # as part of it the answer stopped parsing.
  if [ $# -ge 3 ]; then
    out="$("$2" "$3 lease status '$device' --json" 2>"$err")" && rc=0 || rc=$?
  else
    out="$("${SMIX:?e2e_yield_if_held needs SMIX (source scripts/lib/e2e-binary.sh)}" lease status "$device" --json 2>"$err")" && rc=0 || rc=$?
  fi
  [ "$rc" = 0 ] || { local why; why="$(cat "$err")"; rm -f "$err"; cannot_judge "could not ask the ledger about $device (exit $rc): $why"; }
  # Status taken with `&& rc=0 || rc=$?`: a caller under `set -e` would
  # otherwise end on the assignment itself, before the case below could
  # say why — a script that stops without a word (2026-09-26).
  held="$(printf '%s' "$out" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except ValueError:
    sys.exit(3)
if "heldBy" not in d:
    sys.exit(4)
h = d["heldBy"]
if h:
    state = "alive" if h["alive"] else "its launcher exited, what it started still runs"
    print("pid %s (%s): %s" % (h["pid"], state, h["cmd"]))')" && rc=0 || rc=$?
  local stderr_seen; stderr_seen="$(cat "$err")"; rm -f "$err"
  case $rc in
    0) ;;
    3) cannot_judge "the ledger's answer about $device was not JSON: $out${stderr_seen:+ (stderr: $stderr_seen)}" ;;
    4) cannot_judge "the smix asked about $device does not say who holds a device (no heldBy) — rebuild it" ;;
    *) cannot_judge "could not read the ledger's answer about $device (exit $rc)" ;;
  esac
  [ -z "$held" ] || cannot_judge "$device is held by $held — yielding, not seizing it"
}
