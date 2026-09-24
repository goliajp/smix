#!/usr/bin/env bash
# Source this: it gives the script "$SMIX_RUN", `smix run` judged by the
# code smix reported (see `smix-run` beside it).
#
# A run that fails on anything but a verdict about the screen ends the
# whole script from wherever it was called — inside `$(…)`, under
# `>/dev/null 2>&1`, behind `|| rc=$?` — with smix's own code and line.
# That needs the script's own shell, so it is set up here: smix-run
# signals this pid, and the reason travels through a file because the
# call's own stderr may have been sent anywhere; this trap prints it on
# the script's.
#
# The binary is taken from "$SMIX", else "$SMIX_BIN", as they stand when
# this is sourced, and exported under its own name: a script that set
# SMIX_BIN without exporting it had smix-run find nothing, exit 1 — and
# a smoke expecting its flow to fail read that as the flow failing.

SMIX_RUN_BIN="${SMIX:-${SMIX_BIN:-}}"
if [ -z "$SMIX_RUN_BIN" ] || [ ! -x "$SMIX_RUN_BIN" ]; then
  printf 'judged-run.sh: no smix binary to run (SMIX / SMIX_BIN: %s)\n' "${SMIX_RUN_BIN:-<unset>}" >&2
  exit 1
fi
SMIX_RUN="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/smix-run"
SMIX_E2E_PID=$$
SMIX_E2E_ABORT="${TMPDIR:-/tmp}/smix-e2e-abort.$$"
export SMIX_RUN SMIX_RUN_BIN SMIX_E2E_PID SMIX_E2E_ABORT
trap 'cat "$SMIX_E2E_ABORT" >&2 2>/dev/null; rm -f "$SMIX_E2E_ABORT"; exit 1' USR1
