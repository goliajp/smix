#!/usr/bin/env python3
"""The check finds a device gate that drops the evidence — and only it."""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
CHECK = os.path.join(HERE, "a-device-gate-keeps-crash-evidence.py")

KEEPS = '''#!/usr/bin/env bash
. "$REPO_ROOT/scripts/lib/crash-evidence.sh"
"$SMIX_BIN" run "$yaml" --device "$SIM" > log
keep_crash_evidence_if_the_app_left ios "$SIM" "$yaml" "$t" log out
'''
DROPS = '''#!/usr/bin/env bash
"$SMIX_BIN" run "$flow" --device "$SIM" > log
'''
DRY = '''#!/usr/bin/env bash
"$SMIX_BIN" run "$flow" --device dry --dry-run
'''


def run(files: dict) -> subprocess.CompletedProcess:
    d = tempfile.mkdtemp()
    os.makedirs(os.path.join(d, "scripts", "release"))
    for name, body in files.items():
        with open(os.path.join(d, "scripts", "release", name), "w") as f:
            f.write(body)
    return subprocess.run([sys.executable, CHECK, "--root", d], capture_output=True, text=True)


def main() -> int:
    ok = run({"corpus-gate.sh": KEEPS, "stress-gate.sh": KEEPS, "dry.sh": DRY})
    assert ok.returncode == 0, ok.stdout
    assert "2 gate(s)" in ok.stdout, ok.stdout
    bad = run({"corpus-gate.sh": KEEPS, "stress-gate.sh": DROPS})
    assert bad.returncode == 1 and "stress-gate.sh: runs flows" in bad.stdout, bad.stdout
    assert "corpus-gate.sh: runs" not in bad.stdout, bad.stdout
    lost = run({"corpus-gate.sh": KEEPS, "stress-gate.sh": DRY})
    assert lost.returncode == 1 and "not found as gates" in lost.stdout, lost.stdout
    empty = run({})
    assert empty.returncode == 1, empty.stdout
    print("a-device-gate-keeps-crash-evidence.test: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
