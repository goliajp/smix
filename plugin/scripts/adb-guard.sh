#!/usr/bin/env bash
#
# adb-guard — PreToolUse Bash hook enforcing explicit-emulator
# addressing for Android device mutations. The Android counterpart of
# sim-guard.sh, and the parity gap it closes: iOS has had explicit-UDID
# enforcement for a while, Android had nothing, so any `adb install` or
# `am instrument` with no `-s` went to whatever single device was
# attached — which on a developer machine is often a physical phone.
# One has been wiped that way before (memory: a Samsung SM-S9010).
#
# It BLOCKS (exit 2) a command that mutates a device's app state unless
# that command names an emulator serial explicitly:
#
#   1. `adb install` / `uninstall` / `am instrument` / `adb push` with
#      no `-s emulator-NNNN`, or with a `-s` that names a non-emulator
#      (a physical serial like R5CT52DF07D), and
#   2. `gradlew installDebug*` / `connectedAndroidTest` / `connectedCheck`
#      unless the command line pins `ANDROID_SERIAL=emulator-NNNN` — the
#      gradle android plugin fans install/connected tasks out to EVERY
#      attached device, so an unpinned run reaches the phone too.
#
# Read-only adb (`devices`, `shell getprop`, `forward`, `logcat`,
# `emu kill`) is never touched — none of it writes app state. Non-adb,
# non-gradle commands pass untouched.
#
# The emulator serial pattern is `emulator-<port>`; a physical device
# serial is anything else. Requiring the emulator form (rather than
# denylisting one known phone) means a newly-plugged phone is safe by
# default — the same stance sim-guard takes with UDIDs.
#
# The command under judgement comes from hook-command.py, which drops
# heredoc bodies that are written rather than run — writing a paragraph
# about an install command is not performing one.

set -euo pipefail

command="$(python3 "$(dirname "$0")/hook-command.py" 2>/dev/null || true)"

# Fast path: nothing that could mutate an Android device → allow.
case "$command" in
  *adb\ *|*gradlew*|*am\ instrument*) ;;
  *) exit 0 ;;
esac

deny() {
  echo "adb-guard: $1" >&2
  echo "adb-guard: name an emulator explicitly — 'adb -s emulator-5554 …' or 'ANDROID_SERIAL=emulator-5554 ./gradlew …'; never an unpinned mutation (it reaches a physical phone) and never a physical serial" >&2
  # A guard that only says no gets worked around rather than obeyed. Name
  # the way to do the same thing safely.
  echo "adb-guard: or drive it through smix, which requires the device up front: smix runner up --platform android --device emulator-5554" >&2
  exit 2
}

# Subcommands that read and change nothing, named one by one.
#
# The header has always said read-only adb is never touched. Until
# 2026-08-12 that was not true of a physical serial: `-s <phone>` was
# refused for any subcommand at all, so `adb -s R5CT… shell getprop
# ro.product.cpu.abi` — which writes nothing — came back as a policy
# refusal that reads like a typo. A rule whose stated form is kinder
# than its real one gets discovered the hard way, and by someone who
# then stops believing the rest of it.
#
# An allowlist rather than a denylist, and a narrow one: `shell` on its
# own is not read-only (`shell pm uninstall`, `shell rm`), so only
# `shell getprop` is here. Anything not named falls through to the
# refusals below.
is_read_only() {
  case "$1" in
    *' devices'*|*' get-state'*|*' get-serialno'*|*' start-server'*) return 0 ;;
    *' logcat'*|*' forward --list'*|*' reverse --list'*) return 0 ;;
    *' shell getprop'*) return 0 ;;
    *) return 1 ;;
  esac
}

# A `-s <serial>` that names a physical device (not emulator-NNNN) is
# refused for anything that could change it. Reading is allowed and
# always was, on paper.
if printf '%s' "$command" | grep -qE 'adb[[:space:]]+(-[a-z]+[[:space:]]+)*-s[[:space:]]+[^[:space:]]+' ; then
  serial="$(printf '%s' "$command" | grep -oE '\-s[[:space:]]+[^[:space:]]+' | head -1 | sed -E 's/^-s[[:space:]]+//')"
  case "$serial" in
    emulator-*) ;;  # explicit emulator — the safe case
    *)
      if ! is_read_only " $command"; then
        deny "adb -s '$serial' names a non-emulator device (physical serials are not emulator-NNNN)"
      fi
      ;;
  esac
fi

# Mutating adb subcommands must carry an explicit emulator serial.
# `am instrument` is reached via `adb [-s …] shell am instrument`.
mutating='(install|uninstall|push)'
if printf '%s' "$command" | grep -qE "adb[[:space:]]+([^|;&]*[[:space:]])?$mutating([[:space:]]|$)" \
   || printf '%s' "$command" | grep -qE 'am[[:space:]]+instrument'; then
  if ! printf '%s' "$command" | grep -qE '\-s[[:space:]]+emulator-[0-9]+'; then
    deny "a device-mutating adb command with no explicit '-s emulator-NNNN'"
  fi
fi

# gradle install / connected tasks fan out to every attached device.
if printf '%s' "$command" | grep -qE 'gradlew[^|;&]*(install(Debug|Release)[A-Za-z]*|connectedAndroidTest|connectedCheck|connectedDebugAndroidTest)'; then
  if ! printf '%s' "$command" | grep -qE 'ANDROID_SERIAL=emulator-[0-9]+'; then
    deny "a gradle install/connected task with no 'ANDROID_SERIAL=emulator-NNNN' pin — it installs to ALL attached devices"
  fi
fi

exit 0
