#!/usr/bin/env bash
# v0.7 C1 — long-run 100-case stability smoke wrapper.
# Runs scripts/long-run-100.ts and asserts the JSON output meets the
# Success Criteria [5] gate (total/passed/failed = 100/100/0 + >=1 restart).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT_FILE="/tmp/longrun-100-$$.json"
trap 'rm -f "$OUT_FILE"' EXIT

bun scripts/long-run-100.ts > "$OUT_FILE"

TOTAL="$(jq -r '.total' "$OUT_FILE")"
PASSED="$(jq -r '.passed' "$OUT_FILE")"
FAILED="$(jq -r '.failed' "$OUT_FILE")"
RESTARTS="$(jq -r '.restarts' "$OUT_FILE")"
ELAPSED="$(jq -r '.elapsed_ms' "$OUT_FILE")"

test "$TOTAL" = '100' || { echo "total != 100: $TOTAL" >&2; exit 1; }
test "$PASSED" = '100' || { echo "passed != 100: $PASSED" >&2; exit 1; }
test "$FAILED" = '0' || { echo "failed != 0: $FAILED" >&2; exit 1; }
test "$RESTARTS" -ge 1 || { echo "restarts < 1: $RESTARTS" >&2; exit 1; }
test "$ELAPSED" -lt 720000 || { echo "elapsed_ms >= 720000: $ELAPSED" >&2; exit 1; }

jq -e '.restart_reasons | map(.reason) | index("every-50-cases") != null' "$OUT_FILE" > /dev/null \
  || { echo "missing every-50-cases in restart_reasons" >&2; exit 1; }

cat "$OUT_FILE"
echo '{"all_ok":true}'
