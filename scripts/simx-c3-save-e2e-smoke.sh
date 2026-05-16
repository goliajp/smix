#!/usr/bin/env bash
# v0.5 C3 REPL `.save` e2e smoke — drives a real dev sim through REPL,
# verifies the generated TS file lands on disk, passes `bun x tsc`, and
# the resulting test runs green under `simx run`.
#
# Emits a single JSON line on stdout with pass/fail signals so a jq gate
# in the parent loop can assert on every field.

set -u  # do not -e: we capture exit codes deliberately

cd "$(dirname "$0")/.." || exit 99

DEV_UDID="$(tr -d '[:space:]' < .simx/dev-sim.txt 2>/dev/null || echo "")"

if [[ -z "$DEV_UDID" ]]; then
  echo '{"error":"no dev-sim udid"}'
  exit 1
fi

NAME="c3-save-smoke"
TARGET="examples/${NAME}.test.ts"
rm -f "$TARGET"

SCRIPT="$(mktemp)"
cat > "$SCRIPT" <<EOF
launch com.apple.Preferences
.save as ${NAME}
.exit
EOF

REPL_OUT="$(mktemp)"
bun src/cli/index.ts repl --udid "$DEV_UDID" < "$SCRIPT" > "$REPL_OUT" 2>&1
REPL_EXIT=$?

FILE_PRESENT="no"
if [[ -f "$TARGET" ]]; then
  FILE_PRESENT="ok"
fi

TSC_OK="fail"
if bun x tsc -p tsconfig.examples.json --noEmit > /dev/null 2>&1; then
  TSC_OK="ok"
fi

BODY_HAS_LAUNCH="no"
BODY_HAS_TEST="no"
if [[ -f "$TARGET" ]]; then
  grep -q "await app.launch" "$TARGET" && BODY_HAS_LAUNCH="yes"
  grep -q "^test(" "$TARGET" && BODY_HAS_TEST="yes"
fi

RUN_EXIT=99
if [[ -f "$TARGET" ]]; then
  bun src/cli/index.ts run "$TARGET" > /dev/null 2>&1
  RUN_EXIT=$?
fi

rm -f "$TARGET" "$SCRIPT" "$REPL_OUT"

printf '{"repl_exit":%d,"file_present":"%s","tsc_ok":"%s","run_exit":%d,"body_has_launch":"%s","body_has_test":"%s"}\n' \
  "$REPL_EXIT" "$FILE_PRESENT" "$TSC_OK" "$RUN_EXIT" "$BODY_HAS_LAUNCH" "$BODY_HAS_TEST"
