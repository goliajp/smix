#!/usr/bin/env bash
# What a `neverVisible` run over a span with a known flash in it says.
#
# Source this, do not run it. The watch reports, when it saw nothing, the
# longest stretch in which nobody was looking. A flash shorter than that
# stretch can fall inside it unseen, so a pass reporting such a gap is not
# evidence that the watch misses flashes — it is no evidence either way.
# Only a pass whose own reported gap was shorter than the flash is a miss.
# The rule reads the watch's own number and nothing about the machine: a
# busy host shows up as a long gap, and a quiet one that still misses is
# still red.
#
#   flash_verdict <exit> <output> <flash-ms>
#       prints `caught: …`, `missed: …` or `cannot: …` and returns 0, 1 or 2
#   fixture_flash_ms <kotlin|swift> <source>
#       prints how long the fixture's overlay stands, in ms, read from the
#       fixture's own source; fails when it cannot be read
#
#     bash scripts/lib/flash-verdict.sh --selftest

flash_verdict() {
  local rc="$1" out="$2" flash_ms="$3" when line gap
  when="$(printf '%s' "$out" | grep -o 'appeared after [0-9]* ms, [^—]*' | head -1 || true)"
  if [ "$rc" != 0 ]; then
    if [ -n "$when" ] && printf '%s' "$when" | grep -qE 'while step [0-9]+ of 2 \((tapOn|extendedWaitUntil)\)'; then
      echo "caught: $when"
      return 0
    fi
    echo "missed: the flow failed without naming when and which step — $(printf '%s' "$out" | tail -4 | tr '\n' ' ')"
    return 1
  fi
  line="$(printf '%s' "$out" | grep -o 'not seen — watched [0-9]* times over [0-9]* ms, longest gap [0-9]* ms' | head -1 || true)"
  if [ -z "$line" ]; then
    echo "missed: the flow passed and the watch reported no gap — $(printf '%s' "$out" | tail -4 | tr '\n' ' ')"
    return 1
  fi
  gap="$(printf '%s' "$line" | sed -E 's/.*longest gap ([0-9]+) ms.*/\1/')"
  if [ "$gap" -ge "$flash_ms" ]; then
    echo "cannot: the watch's own longest gap was ${gap} ms, no shorter than the ${flash_ms} ms flash, so it could fall inside it unseen ($line)"
    return 2
  fi
  echo "missed: a ${flash_ms} ms flash went unseen while the watch never left a gap that long ($line)"
  return 1
}

fixture_flash_ms() {
  python3 - "$1" "$2" <<'PY'
import re, sys

kind, path = sys.argv[1], sys.argv[2]
src = open(path, encoding="utf-8").read()
if kind == "kotlin":
    m = re.search(r"overlay\s*=\s*true\s*\n\s*delay\((\d+)\)", src)
    ms = int(m.group(1)) if m else None
else:
    m = re.search(r"overlay\s*=\s*true\s*\n\s*DispatchQueue\.main\.asyncAfter\(deadline:\s*\.now\(\)\s*\+\s*([0-9.]+)\)", src)
    ms = round(float(m.group(1)) * 1000) if m else None
if ms is None:
    print(f"no flash duration found in {path}", file=sys.stderr)
    sys.exit(1)
print(ms)
PY
}

if [ "${BASH_SOURCE[0]}" = "$0" ] && [ "${1:-}" = "--selftest" ]; then
  set -u
  fail() { echo "flash-verdict selftest FAIL: $*"; exit 1; }
  expect() { # $1 wanted status, $2 exit, $3 output, $4 flash ms
    local said st
    said="$(flash_verdict "$2" "$3" "$4")" && st=0 || st=$?
    [ "$st" = "$1" ] || fail "wanted $1, got $st: $said"
  }
  seen='FAIL: neverVisible: { id="o" } appeared after 709 ms, while step 2 of 2 (extendedWaitUntil) was running — the watch had looked 12 times'
  clear() { echo "WARN: neverVisible { id=\"o\" }: not seen — watched 19 times over 1991 ms, longest gap $1 ms"; }

  expect 0 1 "$seen" 400
  # Looked often enough and missed it: this is what the verdict exists for.
  expect 1 0 "$(clear 214)" 400
  expect 1 0 "$(clear 399)" 400
  # Could have looked away for the whole flash: no verdict.
  expect 2 0 "$(clear 400)" 400
  expect 2 0 "$(clear 3187)" 400
  # A pass with no gap reported is smix failing to say how it looked.
  expect 1 0 "summary: 1 steps" 400
  # A failure that is not a sighting is not a catch.
  expect 1 3 "FAIL [DRIVER_ERROR]: the runner went away" 400

  T="$(mktemp -d)"
  trap 'rm -rf "$T"' EXIT
  # The delay before the overlay is not its duration.
  printf 'onClick = {\n  scope.launch {\n    delay(300)\n    overlay = true\n    delay(250)\n    overlay = false\n' > "$T/a.kt"
  [ "$(fixture_flash_ms kotlin "$T/a.kt")" = 250 ] || fail "kotlin duration not read"
  printf 'Button("Flash") {\n  DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {\n    overlay = true\n    DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {\n' > "$T/a.swift"
  [ "$(fixture_flash_ms swift "$T/a.swift")" = 250 ] || fail "swift duration not read"
  printf 'nothing here\n' > "$T/b.kt"
  if fixture_flash_ms kotlin "$T/b.kt" 2>/dev/null; then fail "an unreadable source gave a duration"; fi
  echo "flash-verdict selftest: ok"
fi
