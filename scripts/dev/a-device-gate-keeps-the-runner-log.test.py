#!/usr/bin/env python3
"""The check finds a gate that drives the Android runner and drops its log — and only it."""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
CHECK = os.path.join(HERE, "a-device-gate-keeps-the-runner-log.py")

KEEPS = '''#!/usr/bin/env bash
. "$REPO_ROOT/scripts/lib/android-runner-log.sh"
since="$(android_device_now "$SERIAL")"
smix runner up "$SERIAL" --platform android || collect_android_runner_log "$SERIAL" "$since" out
'''
DROPS = '''#!/usr/bin/env bash
smix runner up "$SERIAL" --platform android
'''
NO_CLOCK = '''#!/usr/bin/env bash
. "$ROOT/scripts/lib/android-runner-log.sh"
smix runner up "$E2E_ANDROID" || collect_android_runner_log "$SERIAL" "" out
'''
IOS = '''#!/usr/bin/env bash
smix runner up "$SIM" --platform ios
'''
NAMES = ("android-behaviour-gate.sh", "device-e2e-tier.sh", "ship.sh")


def run(files: dict) -> subprocess.CompletedProcess:
    d = tempfile.mkdtemp()
    os.makedirs(os.path.join(d, "scripts", "release"))
    for name, body in files.items():
        with open(os.path.join(d, "scripts", "release", name), "w") as f:
            f.write(body)
    return subprocess.run([sys.executable, CHECK, "--root", d], capture_output=True, text=True)


def main() -> int:
    ok = run({**{n: KEEPS for n in NAMES}, "corpus-gate.sh": IOS})
    assert ok.returncode == 0, ok.stdout
    assert "3 gate(s)" in ok.stdout, ok.stdout
    bad = run({**{n: KEEPS for n in NAMES}, "ship.sh": DROPS})
    assert bad.returncode == 1 and "ship.sh: drives the Android runner and does not source" in bad.stdout, bad.stdout
    assert "device-e2e-tier.sh: drives" not in bad.stdout, bad.stdout
    clock = run({**{n: KEEPS for n in NAMES}, "device-e2e-tier.sh": NO_CLOCK})
    assert clock.returncode == 1 and "never reads the device's clock" in clock.stdout, clock.stdout
    lost = run({**{n: KEEPS for n in NAMES}, "ship.sh": IOS})
    assert lost.returncode == 1 and "not found as gates" in lost.stdout, lost.stdout
    empty = run({})
    assert empty.returncode == 1, empty.stdout
    print("a-device-gate-keeps-the-runner-log.test: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
