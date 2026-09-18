#!/usr/bin/env bash
# What does an open Device Hub window do when a simulator boots under it?
#
# Xcode 27 replaced Simulator.app with Device Hub. smix's capsule guard
# existed because a running Simulator.app pops a window for every boot;
# whether Device Hub does the same decides what the guard should watch.
# Nobody had written that fact down, so this measures it: open Device
# Hub, boot one simulator, read the window before and after, put the
# simulator back.
#
# "Shows" is not "lists". The sidebar lists every simulator on the
# machine whether it is booted or not (measured 2026-09-19: sixteen rows,
# fourteen of them shut down), so "is the device in the sidebar" is
# always yes and says nothing. What a boot could move is the selection
# and the detail view — the window's title names the device on show —
# so those are what is read.
#
# This is an instrument, not a judgement. Exit 0 means it measured;
# exit 2 means it could not read Device Hub's window — which is not the
# same as reading that nothing moved, and must never be reported as such.
#
# Usage:
#   bash scripts/dev/device-hub-shows-a-boot.sh <UDID>
#   bash scripts/dev/device-hub-shows-a-boot.sh --selftest
set -euo pipefail

BUNDLE_ID="com.apple.dt.Devices"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

say() { printf 'device-hub-shows-a-boot: %s\n' "$*"; }

device_hub_running() { pgrep -x DeviceHub >/dev/null 2>&1; }

# The reader: `device-hub-read.swift` compiled once per run, or whatever
# SMIX_DEVICE_HUB_READ names (the selftest points it at a fake).
reader() {
  if [ -n "${SMIX_DEVICE_HUB_READ:-}" ]; then
    printf '%s' "$SMIX_DEVICE_HUB_READ"
    return
  fi
  local out
  out="$(mktemp -d)/device-hub-read"
  swiftc -O -o "$out" "$HERE/device-hub-read.swift" >/dev/null
  printf '%s' "$out"
}

device_state_of() {
  xcrun simctl list devices -j | python3 -c '
import json, sys
u = sys.argv[1]
for devs in json.load(sys.stdin)["devices"].values():
    for d in devs:
        if d["udid"] == u:
            print(d["state"]); sys.exit(0)
print("absent")' "$1"
}

wait_for_state() {
  local udid="$1" want="$2" i
  for i in $(seq 1 60); do
    [ "$(device_state_of "$udid")" = "$want" ] && return 0
    sleep 1
  done
  return 1
}

# One reading → "title=<t>\tselected=<udid|none>\tlists=<yes|no>", or exit 2.
read_hub() {
  local bin="$1" udid="$2" text title selected lists
  text="$("$bin" 2>&1)" || {
    say "cannot read Device Hub's window ($text) — an unread window, not a window where nothing moved" >&2
    exit 2
  }
  title="$(printf '%s\n' "$text" | sed -n 's/^title=//p' | head -1)"
  selected="$(printf '%s\n' "$text" | awk '$1=="row" && $3=="selected=true" {print $2; exit}')"
  if printf '%s\n' "$text" | grep -q "^row $udid "; then lists=yes; else lists=no; fi
  printf '%s\t%s\t%s' "$title" "${selected:-none}" "$lists"
}

measure() {
  local udid="$1" bin was_running i windows before after
  local title_before sel_before lists_before title_after sel_after lists_after moved
  [ "$(device_state_of "$udid")" = "Shutdown" ] || { say "$udid is not Shutdown — this measures a boot, so it needs one to perform" >&2; exit 2; }

  if device_hub_running; then was_running=1; else was_running=0; fi
  say "device_hub_was_running=$was_running"
  bin="$(reader)"

  open -b "$BUNDLE_ID"
  for i in $(seq 1 40); do
    windows="$(osascript -e 'tell application "System Events" to tell process "DeviceHub" to count windows' 2>&1)"
    case "$windows" in ''|*[!0-9]*) ;; *) [ "$windows" -ge 1 ] && break ;; esac
    sleep 0.5
  done
  case "$windows" in
    ''|*[!0-9]*) say "cannot read Device Hub's windows (System Events said: $windows)" >&2; exit 2 ;;
  esac
  [ "$windows" -ge 1 ] || { say "Device Hub opened no window within 20 s — nothing to measure against" >&2; exit 2; }

  before="$(read_hub "$bin" "$udid")"
  IFS=$'\t' read -r title_before sel_before lists_before <<<"$before"
  if [ "$lists_before" != yes ]; then
    say "the reading has no row for $udid — the sidebar lists every simulator, so a reading without this one did not read the sidebar" >&2
    exit 2
  fi

  smix sim boot "$udid" 2>/dev/null
  wait_for_state "$udid" Booted || { say "$udid did not reach Booted" >&2; smix sim shutdown "$udid" 2>/dev/null || true; exit 2; }
  sleep 5

  after="$(read_hub "$bin" "$udid")"
  IFS=$'\t' read -r title_after sel_after lists_after <<<"$after"

  smix sim shutdown "$udid" 2>/dev/null
  wait_for_state "$udid" Shutdown || say "warning: $udid still not Shutdown" >&2
  if [ "$was_running" = 0 ]; then
    osascript -e 'tell application "DeviceHub" to quit' >/dev/null 2>&1 || true
  fi

  if [ "$sel_after" = "$udid" ]; then moved=yes; else moved=no; fi
  say "windows=$windows selected_before=$sel_before selected_after=$sel_after title_before=\"$title_before\" title_after=\"$title_after\" hub_moved_to_device=$moved"
}

# The instrument driven with fakes on PATH and a fake reader: the three
# shapes it must tell apart are "the selection moved to the booted
# device", "it did not", and "the window could not be read" — and only
# the last one is exit 2.
selftest() {
  local self tmp bin fail=0 out rc
  self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  tmp="$(mktemp -d)"; bin="$tmp/bin"; mkdir -p "$bin"
  ln -s "$(command -v python3)" "$bin/python3"
  local u=00000000-0000-4000-8000-000000000001 other=00000000-0000-4000-8000-000000000002

  printf '#!/bin/sh\ncat "%s/devices.json"\n' "$tmp" > "$bin/xcrun"
  printf '#!/bin/sh\nexit 0\n' > "$bin/pgrep"
  cp "$bin/pgrep" "$bin/open"; cp "$bin/pgrep" "$bin/sleep"
  printf '#!/bin/sh\necho 1\n' > "$bin/osascript"
  cat > "$bin/smix" <<EOF
#!/bin/sh
case "\$2" in
  boot) sed -i '' 's/"state":"Shutdown"/"state":"Booted"/' "$tmp/devices.json" ;;
  shutdown) sed -i '' 's/"state":"Booted"/"state":"Shutdown"/' "$tmp/devices.json" ;;
esac
EOF
  chmod +x "$bin"/*
  reset_devices() {
    printf '{"devices":{"rt":[{"udid":"%s","name":"fixture-sim","state":"Shutdown"}]}}' "$u" > "$tmp/devices.json"
  }
  # The fake reader answers from two canned readings, before and after
  # the fixture says the device booted.
  write_reader() {
    cat > "$tmp/reader" <<EOF
#!/bin/sh
if grep -q Booted "$tmp/devices.json"; then cat "$tmp/after.txt"; else cat "$tmp/before.txt"; fi
EOF
    chmod +x "$tmp/reader"
  }
  write_reader
  run() { PATH="$bin:/usr/bin:/bin" SMIX_DEVICE_HUB_READ="$tmp/reader" bash "$self" "$u" 2>"$tmp/err"; }

  reset_devices
  printf 'title=other\nrow %s selected=true\nrow %s selected=false\n' "$other" "$u" > "$tmp/before.txt"
  printf 'title=fixture-sim\nrow %s selected=false\nrow %s selected=true\n' "$other" "$u" > "$tmp/after.txt"
  out="$(run)" || { echo "selftest: moved case exited $?: $(cat "$tmp/err")" >&2; fail=1; }
  case "$out" in *"hub_moved_to_device=yes"*) ;; *) echo "selftest: expected hub_moved_to_device=yes, got: $out" >&2; fail=1 ;; esac

  reset_devices
  cp "$tmp/before.txt" "$tmp/after.txt"
  out="$(run)" || { echo "selftest: unmoved case exited $?: $(cat "$tmp/err")" >&2; fail=1; }
  case "$out" in *"hub_moved_to_device=no"*) ;; *) echo "selftest: expected hub_moved_to_device=no, got: $out" >&2; fail=1 ;; esac

  reset_devices
  printf 'title=other\nrow %s selected=true\n' "$other" > "$tmp/before.txt"
  rc=0; run >/dev/null || rc=$?
  [ "$rc" = 2 ] || { echo "selftest: a reading without the device's row should exit 2, got $rc" >&2; fail=1; }
  grep -q 'did not read the sidebar' "$tmp/err" || { echo "selftest: the missing-row verdict must say the sidebar was not read" >&2; fail=1; }

  reset_devices
  printf '#!/bin/sh\necho "device-hub-read: Device Hub has no window" >&2; exit 2\n' > "$tmp/reader"; chmod +x "$tmp/reader"
  rc=0; run >/dev/null || rc=$?
  [ "$rc" = 2 ] || { echo "selftest: an unreadable window should exit 2, got $rc" >&2; fail=1; }
  grep -q 'not a window where nothing moved' "$tmp/err" || { echo "selftest: the unreadable verdict must say it is unread, not unmoved" >&2; fail=1; }

  rm -rf "$tmp"
  [ "$fail" = 0 ] || exit 1
  echo "device-hub-shows-a-boot selftest: moved, unmoved, a reading without the device, and an unreadable window are told apart; only the last two are exit 2"
}

case "${1:-}" in
  --selftest) selftest ;;
  "") echo "usage: device-hub-shows-a-boot.sh <UDID> | --selftest" >&2; exit 2 ;;
  *) measure "$1" ;;
esac
