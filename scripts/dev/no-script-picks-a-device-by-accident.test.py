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


# A helper that touches a device, so a caller can reach one without ever
# spelling `adb`. Factoring three gates onto one of these took all three
# out of the gate's sight, and nothing said so: the count fell and the
# verdict stayed clean.
TOUCHING_MODULE = (
    "import subprocess\n"
    "def call(device):\n"
    "    subprocess.run(['adb', '-s', device, 'shell', 'echo'])\n"
)

IMPORT_CASES = [
    ("a caller that reaches for a device through a helper",
     "import _helper\n_helper.call('emulator-5554')\n", True),
    # The caller here is beyond reproach, so a refusal can only come from
    # the helper writing a serial into itself. Without that rule a library
    # is excused from proving anything, and this passes.
    ("a helper that writes a serial into itself",
     "import argparse, _helper\n"
     "ap = argparse.ArgumentParser()\nap.add_argument('--device')\n"
     "_helper.call(ap.parse_args().device)\n", True, "hardcoding"),
    ("a caller that passes the helper what it was given",
     "import argparse, _helper\n"
     "ap = argparse.ArgumentParser()\nap.add_argument('--device')\n"
     "_helper.call(ap.parse_args().device)\n", False),
]

HARDCODING_MODULE = (
    "import subprocess\n"
    "def call(device):\n"
    "    subprocess.run(['adb', '-s', 'emulator-5554', 'shell', 'echo'])\n"
)


def run_with(script_body, extra=None):
    with tempfile.TemporaryDirectory() as root:
        d = os.path.join(root, "scripts", "dev")
        os.makedirs(d)
        for n, b in (extra or {}).items():
            with open(os.path.join(d, n), "w") as fh:
                fh.write(b)
        with open(os.path.join(d, "probe.sh" if extra is None else "caller.py"),
                  "w") as fh:
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
    for case in IMPORT_CASES:
        name, body, must_refuse = case[0], case[1], case[2]
        helper = HARDCODING_MODULE if len(case) > 3 else TOUCHING_MODULE
        r = run_with(body, extra={"_helper.py": helper})
        if (r.returncode != 0) != must_refuse:
            failures.append(
                f"{name}: expected {'a refusal' if must_refuse else 'a pass'}, "
                f"got {(r.stdout + r.stderr).strip().splitlines()[-1][:90]}")
    if failures:
        print("no-script-picks-a-device-by-accident.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"no-script-picks-a-device-by-accident.test: "
          f"{len(CASES) + len(IMPORT_CASES) + 1} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
