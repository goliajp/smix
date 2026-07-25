#!/usr/bin/env bash
# Report when the runner this session is driving through goes away.
#
# Between two tool calls, the app under test can crash and the runner can
# die, and nothing says so: the next sense call simply reports that an
# element is not there. That reads as a selector problem and gets debugged
# as one.
#
# Every line this prints becomes a notification, so silence is the normal
# state and only transitions are worth saying. A monitor that narrates
# each poll is one that gets turned off.
set -uo pipefail

PORT="${SMIX_RUNNER_PORT:-22087}"
INTERVAL="${SMIX_WATCH_INTERVAL_S:-5}"
# Bounded runs are how this is tested; unset means run for the session.
MAX="${SMIX_WATCH_ITERATIONS:-0}"

alive() {
  # A bare TCP connect would call a wedged runner healthy. /health is what
  # every other part of smix asks.
  curl -sf -m 2 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1
}

# `unknown` rather than assuming: the first observation is a reading, not
# a change, and announcing "the runner is down" to a session that never
# started one is noise.
state="unknown"
count=0

while :; do
  if alive; then
    now="up"
  else
    now="down"
  fi

  if [ "$now" != "$state" ]; then
    case "$state:$now" in
      unknown:down)
        # Nothing was driving when the watch began. Not an event.
        ;;
      unknown:up)
        ;;
      up:down)
        echo "smix: the runner on port ${PORT} stopped answering — the app under test or the runner died; smix_use will bring it back"
        ;;
      down:up)
        echo "smix: the runner on port ${PORT} is answering again"
        ;;
    esac
    state="$now"
  fi

  count=$((count + 1))
  if [ "$MAX" -gt 0 ] && [ "$count" -ge "$MAX" ]; then
    exit 0
  fi
  sleep "$INTERVAL"
done
