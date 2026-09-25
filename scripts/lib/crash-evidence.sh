#!/usr/bin/env bash
# What the device recorded when the app under test went away mid-flow.
#
# A device gate that sees a flow fail with APP_NOT_RUNNING has, until
# now, kept nothing but the flow's own log. CI's fixture left mid-flow
# twice in one run (K2, 2026-09-25), the rerun was green, and there was
# no crash report and no system log to say whether it crashed, was
# killed, or was never there — the second run could not be compared with
# anything.
#
# Source this and call
#
#     collect_crash_evidence <ios|android> <device> <app> <since-epoch> <outdir>
#
# It writes into <outdir>:
#   * summary.txt — what was looked at and what was found, including
#     "looked, found none" when that is the answer. A folder that could
#     not be read says so and the call returns 2: "nothing there" and
#     "could not look" are different answers.
#   * on iOS, each crash report (.ips) for <app> written since
#     <since-epoch> in this machine's report folder, copied as-is, and
#     sim-log.txt: the simulator's own log lines naming <app> since then
#     (termination reasons live there, and a killed app writes no .ips).
#   * on Android, crashes.txt: `smix sim crashes <device> --app <app>`.
#
# The report folder is this machine's (`~/Library/Logs/DiagnosticReports`):
# simulator reports are not divided by device, so the app and the window
# are what attribute one to this run. SMIX_CRASH_REPORT_DIR points
# elsewhere, for the self-test.
#
#     bash scripts/lib/crash-evidence.sh --selftest

# Copy the .ips reports for <app> written at or after <since> from <dir>
# into <outdir>; prints how many. Returns 2 when <dir> cannot be read.
crash_reports_since() {
  local dir="$1" app="$2" since="$3" outdir="$4" n=0 f mtime
  [ -d "$dir" ] && [ -r "$dir" ] || return 2
  for f in "$dir"/*.ips; do
    [ -e "$f" ] || continue
    # GNU first: on Linux `stat -f` is a filesystem query that succeeds with
    # a paragraph of output, so trying BSD first never reaches the fallback.
    mtime="$(stat -c %Y "$f" 2>/dev/null || stat -f %m "$f")"
    [ "$mtime" -ge "$since" ] || continue
    # The header is the first line, and it names the bundle.
    head -1 "$f" | grep -q "\"bundleID\" *: *\"$app\"" || continue
    cp -p "$f" "$outdir/"
    n=$((n + 1))
  done
  printf '%s\n' "$n"
}

collect_crash_evidence() {
  local platform="$1" device="$2" app="$3" since="$4" outdir="$5"
  local dir="${SMIX_CRASH_REPORT_DIR:-$HOME/Library/Logs/DiagnosticReports}" n rc=0
  mkdir -p "$outdir"
  {
    printf 'app: %s\ndevice: %s (%s)\nsince: %s (epoch %s)\n' \
      "$app" "$device" "$platform" "$(date -r "$since" 2>/dev/null || date -d "@$since")" "$since"
  } >"$outdir/summary.txt"
  case "$platform" in
    ios)
      if n="$(crash_reports_since "$dir" "$app" "$since" "$outdir")"; then
        if [ "$n" = 0 ]; then
          printf 'crash reports: looked in %s, found none for %s since then\n' "$dir" "$app" >>"$outdir/summary.txt"
        else
          printf 'crash reports: %s copied from %s\n' "$n" "$dir" >>"$outdir/summary.txt"
        fi
      else
        printf 'crash reports: COULD NOT LOOK — %s is not a readable folder\n' "$dir" >>"$outdir/summary.txt"
        rc=2
      fi
      local start
      start="$(date -r "$since" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date -d "@$since" '+%Y-%m-%d %H:%M:%S')"
      if xcrun simctl spawn "$device" log show --style compact --start "$start" \
          --predicate "eventMessage CONTAINS \"$app\"" >"$outdir/sim-log.full.txt" 2>"$outdir/sim-log.err"; then
        tail -400 "$outdir/sim-log.full.txt" >"$outdir/sim-log.txt"
        rm -f "$outdir/sim-log.full.txt" "$outdir/sim-log.err"
        printf 'simulator log: %s line(s) naming %s since then (last 400 kept in sim-log.txt)\n' \
          "$(grep -c . "$outdir/sim-log.txt")" "$app" >>"$outdir/summary.txt"
      else
        printf 'simulator log: COULD NOT READ — %s\n' "$(head -2 "$outdir/sim-log.err" | tr '\n' ' ')" >>"$outdir/summary.txt"
        rc=2
      fi
      ;;
    android)
      if "${SMIX:?SMIX is the binary this gate drives}" sim crashes "$device" --app "$app" \
          >"$outdir/crashes.txt" 2>&1; then
        printf 'crash buffer: %s\n' "$(head -1 "$outdir/crashes.txt")" >>"$outdir/summary.txt"
      else
        printf 'crash buffer: COULD NOT READ — see crashes.txt\n' >>"$outdir/summary.txt"
        rc=2
      fi
      ;;
    *)
      printf 'platform: %s is not one this knows how to read\n' "$platform" >>"$outdir/summary.txt"
      rc=2
      ;;
  esac
  return "$rc"
}

# Whether a flow's log says the app under test was not running. The code
# is smix's own wire string, printed as `FAIL [CODE]`.
app_left_mid_flow() {
  grep -q 'FAIL \[APP_NOT_RUNNING\]' "$1"
}

# The app a flow drives: its header's `appId:`, quotes stripped.
flow_app_id() {
  sed -n '/^---/q; s/^appId:[[:space:]]*//p' "$1" | head -1 | tr -d "\"'" | tr -d '[:space:]'
}

# The whole step for a gate: if <log> says the app left, collect what the
# device recorded into <outdir>/<name> and say where. Never fails the
# gate by itself — the flow's own verdict already did; a collector that
# could not look says so in its summary and on this line.
keep_crash_evidence_if_the_app_left() {
  local platform="$1" device="$2" yaml="$3" since="$4" log="$5" outdir="$6" app rc=0
  app_left_mid_flow "$log" || return 0
  app="$(flow_app_id "$yaml")"
  collect_crash_evidence "$platform" "$device" "${app:-unknown}" "$since" "$outdir" || rc=$?
  echo "  crash evidence for ${app:-unknown} on $device: $outdir ($(sed -n '4p' "$outdir/summary.txt"))$([ "$rc" = 0 ] || echo ' — could not look everywhere, see summary.txt')"
}

if [ "${BASH_SOURCE[0]}" = "$0" ] && [ "${1:-}" = "--selftest" ]; then
  set -uo pipefail
  T="$(mktemp -d)"
  trap 'rm -rf "$T"' EXIT
  fail() { echo "crash-evidence selftest FAIL: $*"; exit 1; }
  mkdir -p "$T/reports" "$T/out1" "$T/out2" "$T/out3"
  now="$(date +%s)"
  printf '{"app_name":"SmixFixture","bundleID":"jp.golia.smix.fixture"}\n{}\n' >"$T/reports/SmixFixture-2026-09-25-100000.ips"
  printf '{"app_name":"Other","bundleID":"com.example.other"}\n{}\n' >"$T/reports/Other-2026-09-25-100000.ips"
  printf '{"app_name":"SmixFixture","bundleID":"jp.golia.smix.fixture"}\n{}\n' >"$T/reports/SmixFixture-old.ips"
  touch -t 202001010000 "$T/reports/SmixFixture-old.ips"
  n="$(crash_reports_since "$T/reports" jp.golia.smix.fixture "$((now - 60))" "$T/out1")" \
    || fail "a readable folder answered 'could not look'"
  [ "$n" = 1 ] || fail "expected the one in-window report for the app, counted $n"
  [ -f "$T/out1/SmixFixture-2026-09-25-100000.ips" ] || fail "the in-window report was not copied"
  [ ! -e "$T/out1/Other-2026-09-25-100000.ips" ] || fail "another app's report was copied"
  [ ! -e "$T/out1/SmixFixture-old.ips" ] || fail "a report from before the window was copied"
  n="$(crash_reports_since "$T/reports" jp.golia.smix.nothing "$((now - 60))" "$T/out2")" \
    || fail "a readable folder answered 'could not look'"
  [ "$n" = 0 ] || fail "no report for this app, counted $n"
  crash_reports_since "$T/no-such-folder" jp.golia.smix.fixture 0 "$T/out3" >/dev/null && rc=0 || rc=$?
  [ "$rc" = 2 ] || fail "a missing folder must be 'could not look' (2), got $rc"
  printf 'STEP 4 → FAILED\nerror: sdk: FAIL [APP_NOT_RUNNING]: step 4\n' >"$T/gone.log"
  printf 'error: sdk: FAIL [DRIVER_ERROR]: step 4\n' >"$T/other.log"
  app_left_mid_flow "$T/gone.log" || fail "APP_NOT_RUNNING was not recognised"
  ! app_left_mid_flow "$T/other.log" || fail "DRIVER_ERROR was read as the app leaving"
  printf "appId: 'jp.golia.smix.fixture'\n---\n- launchApp\n- appId: not-me\n" >"$T/flow.yaml"
  [ "$(flow_app_id "$T/flow.yaml")" = jp.golia.smix.fixture ] || fail "flow_app_id read '$(flow_app_id "$T/flow.yaml")'"
  echo "crash-evidence: selftest ok"
fi
