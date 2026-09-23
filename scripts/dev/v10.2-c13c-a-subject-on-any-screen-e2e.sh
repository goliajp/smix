#!/usr/bin/env bash
# v10.2-C13c: the thing C5 measures is there on any screen, because the
# fixture works it out — not because this screen happens to be shaped
# that way.
#
# C5 needs one row cut by the bottom edge with its middle below it: that
# is the state where the old stop rule returned and the tap that
# followed missed. Which row does not matter; that one exists does.
#
# It used to be left to arithmetic nobody controlled. With rows at a
# fixed pitch, whether such a row exists is decided by
# `(viewport height − first row's top) mod pitch`:
#
#   [0, pitch/2)      a row straddles AND its middle is outside  — the subject
#   [pitch/2, height) a row straddles, middle still inside       — no subject
#   [height, pitch)   the edge falls in a gap, nothing straddles — no subject
#
# Measured on 2026-09-23 before the fixture computed it: rows 110px at a
# 132px pitch, first row at y=214, screen 2340 → remainder 14. The
# subject existed with 41px to spare, and about 42% of screen heights
# would have had none. C5 had been red for a day for exactly that
# reason, and read as a product defect (open-items O1).
#
# So this drives the same screen at several heights. Each one must have
# the subject. A height that does not is this gate's whole point.
#
# Three codes, per the contract: 0 judged and right, 1 judged and wrong
# or the setup this run owns did not come up, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
ALIAS="${SMIX_C13C_ANDROID:-sim-smix-android-01}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

# Heights that span a whole row pitch, not heights that were red once.
#
# Three numbers chosen against the geometry of the day would go on
# passing after the fixture stopped computing anything — they would be
# this file's own version of the number copied next to the thing it
# counts (§14.8). Five heights 70px apart span 280px, which is more than
# the 275px pitch, so whatever phase the screen starts in, one of them
# MUST land in the half of the pitch that has no subject unless the
# fixture puts it there. That is the difference between a sweep that
# proves the subject is constructed and one that hopes it is.
HEIGHTS="${SMIX_C13C_HEIGHTS:-2260 2330 2400 2470 2540}"
WIDTH="${SMIX_C13C_WIDTH:-1080}"

log()  { printf '[c13c-subject] %s\n' "$*" >&2; }
fail() { printf '[c13c-subject] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c13c-subject] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_UPPED=0 WE_BOOTED=0 SIZE_CHANGED=0
cleanup() {
  # The display first, and unconditionally: leaving somebody's emulator
  # at 1080x2260 is a change this script made to a machine it borrowed.
  if [ "$SIZE_CHANGED" = 1 ] && [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" shell wm size reset >/dev/null 2>&1 || \
      printf '[c13c-subject] warning: the display size was NOT restored — run: adb -s %s shell wm size reset\n' "$SERIAL" >&2
  fi
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[c13c-subject] warning: the runner was not stopped\n' >&2
  fi
  # Only one this run started: an emulator somebody else is using is
  # not this script's to close.
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tail -1)"
[ -n "$SERIAL" ] || cannot_judge "no device registered as $ALIAS"
case "$SERIAL" in
  emulator-*) : ;;
  # It installs a fixture and resizes the display. A registered phone
  # belongs to somebody (§9 #1).
  *) cannot_judge "$ALIAS resolves to $SERIAL, which is not an emulator — refusing" ;;
esac
if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $SERIAL"
  "$SMIX" sim boot "$SERIAL" >/dev/null 2>&1 || fail "could not boot $SERIAL"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
# And the one THESE sources build: the path existing says a build
# happened, not which sources it happened over (open-items O1).
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

# The rows, as the flow's own eyes read them, with the screen they are on.
#
# The tree arrives in the environment, not on stdin: `python3 -` reads
# its program from stdin, so a heredoc and a pipe cannot both be there —
# the first version of this function had both and died with a traceback,
# which is a crash where a verdict belongs.
subject_of() { # $TREE_JSON: tree json
  python3 - <<'PY'
import json, os
d = json.loads(os.environ["TREE_JSON"])
root = d["root"]
sh = root["bounds"]["h"]
rows = []
def walk(n):
    ident = (n.get("identifier") or "").split("/")[-1]
    b = n["bounds"]
    if ident.startswith("scroll_row_"):
        rows.append((int(ident.split("_")[-1]), b["y"], b["h"]))
    for c in n.get("children") or []:
        walk(c)
walk(root)
rows.sort()
if not rows:
    print("no-rows")
    raise SystemExit
for idx, y, h in rows:
    if y < sh < y + h / 2:
        print(f"row{idx} showing={sh - y:.0f}px of {h:.0f} (its middle is {y + h/2 - sh:.0f}px below the edge)")
        raise SystemExit
# Say which of the two ways it is missing: a straddler whose middle is
# inside is a different screen from one where nothing straddles at all.
straddling = [idx for idx, y, h in rows if y < sh < y + h]
if straddling:
    idx, y, h = next((r for r in rows if r[0] == straddling[0]))
    print(f"none-centre-in (row{idx} straddles, showing {sh - y:.0f}px of {h:.0f} — its middle is still on screen)")
else:
    print("none-no-straddler (the bottom edge falls between two rows)")
PY
}

for height in $HEIGHTS; do
  adb -s "$SERIAL" shell wm size "${WIDTH}x${height}" >/dev/null 2>&1 \
    || fail "could not set the display to ${WIDTH}x${height}"
  SIZE_CHANGED=1
  adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
  adb -s "$SERIAL" shell am start -n "$APPID/.ScrollActivity" >/dev/null 2>&1 \
    || fail "could not start the scrolling screen at ${WIDTH}x${height}"

  # Polled, not read once: straight after the start the probe answers
  # with every row at (0,0,0,0) for a frame or two, and a single look
  # lands there and reports a screen with no rows on it.
  verdict="no-rows"
  for _ in $(seq 1 30); do
    tree="$("$SMIX" tree --json --device "$SERIAL" --port "$PORT" 2>/dev/null | grep -v '^kevy:')" || tree=""
    case "$tree" in
      \{*) verdict="$(TREE_JSON="$tree" subject_of)" ;;
      *)   verdict="no-tree" ;;
    esac
    case "$verdict" in
      row*) break ;;
    esac
    sleep 0.5
  done

  case "$verdict" in
    row*) log "${WIDTH}x${height}: subject=$verdict" ;;
    *) fail "${WIDTH}x${height}: $verdict — the screen C5 measures has no subject at this size, so C5 would be red about the product when the product is fine" ;;
  esac
done

log "C13C-SUBJECT-E2E-PASS on $SERIAL (a row cut by the bottom edge with its middle outside, at every height tried)"
