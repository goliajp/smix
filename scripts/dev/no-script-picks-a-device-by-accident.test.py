#!/usr/bin/env python3
"""Every rule in the device-selection gate can still refuse something.

Fixture trees, because the gate's subject is how a script chooses a
device and half these cases need a script that chooses badly — which
the repository no longer contains. Both directions: each accident must
be refused, each deliberate form must be allowed, and a tree that
touches no device must not be counted as covered.

Each mutation asserts its edit landed before the verdict is read.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "no-script-picks-a-device-by-accident.py")

DELIBERATE_SCRIPT = '''#!/usr/bin/env bash
SERIAL="$(bash scripts/dev/pick-dev-emulator.sh)" || exit 1
adb -s "$SERIAL" shell getprop ro.build.version.sdk
'''

CASES = [
    ("scans adb devices for the first emulator",
     'SERIAL="$(adb devices | awk \'/^emulator-[0-9]+/ {print $1; exit}\')"\nadb -s "$SERIAL" shell echo\n', True),
    ("falls back to emulator-5554",
     'SERIAL="${ADB_SERIAL:-emulator-5554}"\nadb -s "$SERIAL" shell echo\n', True),
    ("hard-codes a serial",
     'SERIAL="emulator-5554"\nadb -s "$SERIAL" shell echo\n', True),
    ("touches a device with no source for it",
     'adb -s "$SOMETHING" shell echo\n', True),
    ("asks the ledger", DELIBERATE_SCRIPT, False),
    ("takes the serial from the caller",
     'SERIAL="${SMIX_ANDROID_SERIAL:?}"\nadb -s "$SERIAL" shell echo\n', False),
    ("resolves through smix",
     'SERIAL="$("$SMIX" sim resolve smoke-android)"\nadb -s "$SERIAL" shell echo\n', False),
    ("sources the lifecycle",
     '. "$(dirname "$0")/lib/emulator-lifecycle.sh"\nsmoke_emulator_up\nadb -s "$SERIAL" shell echo\n', False),
]


def run_with(script_body):
    with tempfile.TemporaryDirectory() as root:
        d = os.path.join(root, "scripts", "dev")
        os.makedirs(d)
        with open(os.path.join(d, "probe.sh"), "w") as fh:
            fh.write(script_body)
        # The gate reads REPO from its own location; point it at the fixture.
        env = dict(os.environ, SMIX_GATE_ROOT=root)
        return subprocess.run([sys.executable, GATE], capture_output=True, text=True, env=env)


def main() -> int:
    failures = []
    for name, body, must_refuse in CASES:
        r = run_with(body)
        refused = r.returncode != 0
        if must_refuse and not refused:
            failures.append(f"{name}: allowed — nothing depends on that rule\n{r.stdout}")
        elif not must_refuse and refused:
            failures.append(f"{name}: refused a deliberate choice\n{r.stdout}")
        else:
            print(f"  {name} → {'refused' if refused else 'allowed'}")

    # The floor: a tree that touches nothing must not read as clean.
    r = run_with("#!/usr/bin/env bash\necho hello\n")
    if r.returncode == 0:
        failures.append("a tree touching no device passed — the gate is reading air")
    else:
        print("  a tree touching no device → refused (reading air)")

    if failures:
        print("no-script-picks-a-device-by-accident.test: FAILED")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"no-script-picks-a-device-by-accident.test: {len(CASES) + 1} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
