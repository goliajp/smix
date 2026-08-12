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

# Every judgement below reads ONE command, and it is the same one the
# pattern matched.
#
# It used to read the whole Bash call for both, and the two could land on
# different commands. An `adb -s emulator-5554 shell getprop` in front of
# an `adb -s <phone> install -r app.apk` allowed the install: the pin on
# the first answered for the second, and a phone got an APK. A `devices`
# anywhere in the call released an `rm -rf` on a phone somewhere else in
# it. The mirror image refused honest work — a `curl -s <url>` beside a
# legitimate device command had its URL read as the device name.
#
# The one thing that does travel between commands is an exported
# `ANDROID_SERIAL`, because in shell it genuinely does. A `VAR=value cmd`
# prefix does not: it applies to that command alone, and treating it as a
# pin for the next line would restore the hole in gradle's half. A `-s`
# never travels — it is one invocation's argument — and neither does
# being read-only, which is a fact about one command and says nothing
# about what another one does.
judge_one() {
  local one="$1"
  local carried="$2"
  local serial

  if printf '%s' "$one" | grep -qE 'adb[[:space:]]+(-[a-z]+[[:space:]]+)*-s[[:space:]]+[^[:space:]]+' ; then
    serial="$(printf '%s' "$one" | grep -oE '\-s[[:space:]]+[^[:space:]]+' | head -1 | sed -E 's/^-s[[:space:]]+//')"
    case "$serial" in
      emulator-*) ;;  # explicit emulator — the safe case
      *)
        if ! is_read_only " $one"; then
          deny "adb -s '$serial' names a non-emulator device (physical serials are not emulator-NNNN), in: $one"
        fi
        ;;
    esac
  fi

  # Mutating adb subcommands must carry an explicit emulator serial.
  # `am instrument` is reached via `adb [-s …] shell am instrument`.
  local mutating='(install|uninstall|push)'
  if printf '%s' "$one" | grep -qE "adb[[:space:]]+([^|;&]*[[:space:]])?$mutating([[:space:]]|$)" \
     || printf '%s' "$one" | grep -qE 'am[[:space:]]+instrument'; then
    if ! printf '%s' "$one" | grep -qE '\-s[[:space:]]+emulator-[0-9]+'; then
      deny "a device-mutating adb command with no explicit '-s emulator-NNNN', in: $one"
    fi
  fi

  # gradle install / connected tasks fan out to every attached device.
  if printf '%s' "$one" | grep -qE 'gradlew[^|;&]*(install(Debug|Release)[A-Za-z]*|connectedAndroidTest|connectedCheck|connectedDebugAndroidTest)'; then
    if [ "$carried" != "yes" ] \
       && ! printf '%s' "$one" | grep -qE 'ANDROID_SERIAL=emulator-[0-9]+'; then
      deny "a gradle install/connected task with no 'ANDROID_SERIAL=emulator-NNNN' pin — it installs to ALL attached devices, in: $one"
    fi
  fi
}

carried_pin=""
while IFS= read -r one; do
  [ -n "$one" ] || continue
  if printf '%s' "$one" | grep -qE '^[[:space:]]*export[[:space:]]+ANDROID_SERIAL=emulator-[0-9]+([[:space:]]|$)' \
     || printf '%s' "$one" | grep -qE '^[[:space:]]*ANDROID_SERIAL=emulator-[0-9]+[[:space:]]*$'; then
    carried_pin="yes"
  fi
  judge_one "$one" "$carried_pin"
done <<< "$command"

exit 0
