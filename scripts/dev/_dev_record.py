"""Where the development record is.

The record — the charter, the decision log, the plans, the ledgers, the
correspondence with consumers — is not version-controlled, so no path to
it is written anywhere in the repository. Whoever runs a gate that reads
it names its root:

    SMIX_DEV_RECORD=<root> python3 scripts/dev/contract-scan.py

Under the root the record keeps its own layout: `CLAUDE.md`, `docs/`,
`rule/`, `dogfood/`, `settings.json`.

Unset is not "nothing to check". A gate that needs the record and is not
told where it is says so and does not pass (`gate/no-empty-predicate`):
green must never mean "read nothing".
"""

from __future__ import annotations

import os

VAR = "SMIX_DEV_RECORD"


def root(explicit: str | None = None) -> str | None:
    """The record's root: an explicit argument, else the variable, else None."""
    value = explicit or os.environ.get(VAR)
    return os.path.abspath(value) if value else None


def path(*parts: str, explicit: str | None = None) -> str | None:
    """A path inside the record, or None when no record was named."""
    base = root(explicit)
    return os.path.join(base, *parts) if base else None


def not_named(gate: str) -> str:
    """What a gate says when nobody named the record."""
    return (
        f"{gate}: CANNOT RUN\n"
        f"  - {VAR} is not set. This gate reads the development record, which is "
        f"not version-controlled and so is in no checkout; set {VAR} to the "
        f"record's root to run it. Refusing rather than passing: green must "
        f"never mean \"read nothing\"."
    )
