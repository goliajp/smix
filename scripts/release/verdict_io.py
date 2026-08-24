"""Reading the thing a verdict judges, or saying why it cannot.

Every one of the eight verdict scripts opened its payload the same way —
`json.load(open(sys.argv[1]))` — and every one of them died the same way
when the file was empty or was not JSON:

    json.decoder.JSONDecodeError: Expecting value: line 1 column 1

A traceback says the gate broke; a verdict says what the gate found. They
want opposite responses, and the 7.0.0 ship spent thirty-one minutes
reaching one of these and then could not tell anyone which it was.

An empty or unparseable payload is not a rare shape. `curl` writes a
zero-byte file when the runner dies mid-request, and a proxy that has
given up answers `<html>504 Gateway Timeout</html>` with a 200 — both
land here as "the screen said nothing", which is a real finding and one
the gate should report rather than crash on.

One loader rather than eight guards: eight copies of one sentence is how
two Android gates came to disagree about how to fix the same refusal.
"""

import json
import os
import sys


def read_json(path: str, what: str) -> dict:
    """Parse `path`, or exit 1 with a sentence naming what was expected.

    `what` is the caller's word for the payload — "the tree", "/windows"
    — because "could not parse the file" tells a reader nothing about
    which step of a device gate produced nothing.
    """
    try:
        raw = open(path, encoding="utf-8").read()
    except OSError as e:
        print(f"{what} could not be read from {path}: {e}", file=sys.stderr)
        sys.exit(1)

    if not raw.strip():
        size = os.path.getsize(path) if os.path.exists(path) else 0
        print(
            f"{what} came back empty ({size} bytes at {path}). Nothing here "
            f"proves anything — the usual cause is the runner going away "
            f"mid-request, which is a finding about the device rather than "
            f"about what was on screen.",
            file=sys.stderr,
        )
        sys.exit(1)

    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        head = raw.strip().splitlines()[0][:120] if raw.strip() else ""
        print(
            f"{what} was not JSON ({e}). It starts {head!r} — a proxy or a "
            f"gateway answering in HTML is the usual cause, and that is a "
            f"finding about the connection rather than about the screen.",
            file=sys.stderr,
        )
        sys.exit(1)
