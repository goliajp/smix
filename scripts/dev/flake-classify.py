#!/usr/bin/env python3
"""Did this flow pass, need a retry, or fail?

`smix run --retry N` records every attempt, and `smix diagnostic dump
--json` reads them back under `runner.recentFlows`. This asks that and
answers for one flow, so the corpus gate can say FLAKE where it used to
say only FAIL — "needed two tries" and "does not work" have always been
the same red, and they call for different work.

Asking smix rather than reading a file, because the file this first
read — `~/.local/share/smix/flow-attempts.json` — has not been written
since July. The attempts moved into smix\'s own store and the old JSON
was left behind, so a classifier parsing it answered NORECORD for every
flow while looking like it worked: the gate printed twenty-one passes
and its summary said GREEN, with the classification doing nothing at
all. A fossil with the right shape is worse than no file.

It classifies and stops there. Whether a FLAKE fails the gate is the
gate's decision, and the gate says yes: retrying is here to tell the two
apart, never to turn one into a pass. The `--retry` this restores was
removed once for exactly that reason — it had been quietly absolving a
tap that reported itself missed because the hit chain was snapshotted
after the touch.

Prints one of PASS / FLAKE / FAIL / NORECORD and exits 0. An exit code
would be a second answer to the same question, and the caller would have
to decide which to believe.
"""

# `X | None` in an annotation is evaluated at definition time on
# Python 3.9, which is the interpreter a login shell finds first here
# (Xcode's, 3.9.6) — while preflight runs under whichever python3 is
# on the interactive PATH (3.14 here). This scan passed every local
# run and died in the ship, which starts with `bash -lc`. Deferring
# annotations makes the file mean the same thing under both.
from __future__ import annotations


import argparse
import json
import os
import shutil
import subprocess
import sys


def recent_flows(smix: str) -> list | None:
    """`runner.recentFlows`, or None when smix could not be asked."""
    out = subprocess.run(
        [smix, "diagnostic", "dump", "--json"],
        capture_output=True,
        text=True,
        check=False,
    )
    if out.returncode != 0:
        return None
    try:
        payload = json.loads(out.stdout)
    except json.JSONDecodeError:
        return None
    flows = payload.get("runner", {}).get("recentFlows")
    return flows if isinstance(flows, list) else None


def classify(records: list, flow: str) -> str:
    # The last entry for this name, not the first.
    #
    # The file keeps the most recent 32 flows, so running a 21-flow
    # corpus twice leaves two entries under every name. Reading the
    # earlier one answers about the previous batch while looking exactly
    # like an answer about this one.
    attempts = None
    for record in records:
        if not isinstance(record, dict):
            continue
        # camelCase on the wire, snake_case in the older file this used
        # to read. Both accepted so a fixture can be written either way.
        name = record.get("flowName", record.get("flow_name"))
        if name == flow:
            attempts = record.get("attempts") or []
    if attempts is None:
        return "NORECORD"
    if not attempts:
        return "NORECORD"

    def index_of(a: dict) -> int:
        return a.get("attemptIndex", a.get("attempt_index", 0))

    ordered = sorted(attempts, key=index_of)
    if ordered[0].get("status") == "ok":
        return "PASS"
    if any(a.get("status") == "ok" for a in ordered[1:]):
        return "FLAKE"
    return "FAIL"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("flow", help="flow name as `smix run` recorded it")
    ap.add_argument(
        "--attempts",
        help="read records from this JSON file instead of asking smix (tests)",
    )
    ap.add_argument("--smix", default=os.environ.get("SMIX_BIN") or shutil.which("smix"))
    args = ap.parse_args()

    # Silence is NORECORD, never PASS — an unreachable smix, an
    # unparseable dump, a flow nobody recorded. Reading absence as a
    # pass is how a gate reports coverage it does not have.
    if args.attempts:
        try:
            with open(args.attempts, encoding="utf-8") as fh:
                records = json.load(fh)
        except (OSError, json.JSONDecodeError):
            print("NORECORD")
            return 0
    elif args.smix:
        records = recent_flows(args.smix)
    else:
        records = None

    if not isinstance(records, list):
        print("NORECORD")
        return 0

    print(classify(records, args.flow))
    return 0


if __name__ == "__main__":
    sys.exit(main())
