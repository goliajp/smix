#!/usr/bin/env python3
"""Every rule in the port scan can still refuse something.

Written after three gates slipped past it in one version. Both misses
were in what the scan could SEE, not in what it decided:

  * the override's name was allowed letters only, so a default behind
    `${SMIX_C5_ANDROID_PORT:-22097}` was never looked at — and a literal
    that is not looked at reads exactly like a port asked of the OS;
  * the line had to begin with the binary, so a `runner up` behind an
    environment prefix (`ANDROID_SERIAL=… "$SMIX" runner up`) meant the
    whole script was never checked at all.

So the cases below are mostly about seeing. Fixture trees, because the
scripts that get this wrong are the ones the repository has just
finished fixing — and a self-test that reads the working tree passes on
the day it is written and answers about nothing afterwards.

Both directions, per the rule card: each accident refused, each correct
form allowed, and a tree with no runner in it must not read as clean.

Each case names the sentence it expects, not merely an exit code. The
first draft did not, and it stayed green when the environment-prefix
rule was taken out: the fixture stopped being seen as a gate at all, the
floor rule ("no script was found starting a runner") answered instead,
and a refusal for the opposite reason counted as the rule working.
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "gate-port-scan.py")
LIB = os.path.join(os.path.dirname(HERE), "lib", "gate-port.sh")
SHELL_LINES = os.path.join(HERE, "_shell_lines.py")

# A caller, so the scan's second half has something to read. Without one
# it appends "nothing invokes a runner-starting gate" to every verdict
# and each case below would be refused for a reason it is not about.
CALLER = 'bash scripts/dev/probe-e2e.sh "$DEVICE"\n'

ASKS_THE_OS = """#!/usr/bin/env bash
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
"$SMIX" runner up "$UDID" --runner-port "$PORT"
"""

# A Python gate that attaches to its caller's runner, so the scan's Python
# half has a member in every fixture tree; cases about Python replace it.
PY_TAKER = """import argparse
ap = argparse.ArgumentParser()
ap.add_argument("--port", required=True)
"""

CASES = [
    (
        "a literal default behind an override whose name has digits",
        """#!/usr/bin/env bash
AND_PORT="${SMIX_C5_ANDROID_PORT:-22097}"
SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" runner up "$SERIAL" --runner-port "$AND_PORT"
""",
        True,
        "pins a host port to a literal",
    ),
    (
        "a literal default behind a letters-only override",
        """#!/usr/bin/env bash
PORT="${SMIX_GATE_PORT:-22090}"
"$SMIX" runner up "$UDID" --runner-port "$PORT"
""",
        True,
        "pins a host port to a literal",
    ),
    (
        "a bare literal",
        '#!/usr/bin/env bash\nPORT=28080\n"$SMIX" runner up "$UDID" --runner-port "$PORT"\n',
        True,
        "pins a host port to a literal",
    ),
    (
        "a runner started behind an environment prefix, on a pinned port",
        """#!/usr/bin/env bash
PORT="${SMIX_C6_PORT:-22099}"
ANDROID_SERIAL="$SERIAL" "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT"
""",
        True,
        "pins a host port to a literal",
    ),
    (
        "the default port, taken by saying nothing",
        '#!/usr/bin/env bash\n"$SMIX" runner up "$UDID"\n',
        True,
        "probe-e2e.sh:2: talks to a runner on the machine's default port",
    ),
    ("a port asked of the OS", ASKS_THE_OS, False, "clean"),
    (
        "a second port asked of the OS for the other platform",
        """#!/usr/bin/env bash
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/gate-port.sh"
IOS_PORT="$SMIX_RUNNER_PORT"
gate_free_port AND_PORT
"$SMIX" runner up "$IOS_UDID" --runner-port "$IOS_PORT"
"$SMIX" runner up "$AND_SERIAL" --platform android --runner-port "$AND_PORT"
""",
        False,
        "clean",
    ),
    (
        # Beside a correct gate, because prose on its own leaves a tree
        # with no runner in it and the floor rule answers first — which
        # is what the first draft of this case actually measured.
        "prose about the command, in a script that asks the OS",
        ASKS_THE_OS
        + '# PORT=22087 is the default for `smix runner up`\n'
        + 'log "run: smix runner up X --runner-port 22087"\n',
        False,
        "clean",
    ),
]

# Scripts that talk to a runner without starting one, and the commands that
# are not what they look like. Each sits in the caller's file, beside a
# correct gate, so the floor rule has its subject and the verdict is about
# this line alone.
TALK_CASES = [
    ("a single-shot verb on the default port",
     CALLER + '"$SMIX" tap id:btn-submit\n', True,
     "run-the-gates.sh:2: talks to a runner on the machine's default port"),
    ("a flow run on the default port",
     CALLER + '"$SMIX_RUN" --device "$UDID" flow.yaml\n', True,
     "run-the-gates.sh:2: talks to a runner on the machine's default port"),
    ("a plugin session, whose server dials the environment's port",
     CALLER + '( cd "$W" && claude --plugin-dir "$PLUGIN" -p hi )\n', True,
     "run-the-gates.sh:2: talks to a runner on the machine's default port"),
    ("every command naming the port its caller gave, across a continuation",
     CALLER + '"$SMIX_RUN" --device "$D" \\\n  --runner-port "$PORT" flow.yaml\n', False, "clean"),
    ("an android runner stopped by device, which never reads the port",
     CALLER + '"$SMIX" runner down --platform android --device "$SERIAL"\n', False, "clean"),
    ("the same stop without --platform android is the iOS form",
     CALLER + '"$SMIX" runner down --device "$SERIAL"\n', True,
     "run-the-gates.sh:2: talks to a runner on the machine's default port"),
    ("a case pattern naming the command",
     CALLER + 'case "$c" in\n  *"smix runner up"*) ;;\nesac\n', False, "clean"),
    ("the server's name in an install line",
     CALLER + 'cargo install smix-cli smix-mcp --locked\n', False, "clean"),
]

# A port taken here does not cross ssh.
REMOTE_CASES = [
    ("a runner brought up over ssh on the far default",
     ASKS_THE_OS + 'rssh "cd r && target/release/smix runner up $U --bundle b" \\\n  || fail up\n',
     True, "sends a runner command over ssh without naming its port"),
    ("a runner stopped over ssh on the far default",
     ASKS_THE_OS + 'rssh "cd r && target/release/smix runner down" || true\n',
     True, "sends a runner command over ssh without naming its port"),
    ("a runner over ssh on a port asked of the far machine",
     ASKS_THE_OS + 'gate_free_port_on NODE_PORT rssh\n'
     'rssh "cd r && target/release/smix runner up $U --runner-port $NODE_PORT" || fail up\n',
     False, "clean"),
]

# The Python half: a port the caller gives, or none at all.
PY_CASES = [
    ("a python gate defaulting its port to a literal",
     'import argparse\nap = argparse.ArgumentParser()\nap.add_argument("--port", default="22095")\n',
     True, "attach-gate.py:3: --port defaults to 22095"),
    ("a python proxy defaulting where it listens",
     'import argparse\nap = argparse.ArgumentParser()\nap.add_argument("--port", required=True)\n'
     'ap.add_argument("--listen", type=int, default=28090)\n',
     True, "--listen defaults to 28090"),
    ("a python gate falling back to the default port",
     'import argparse, os\nap = argparse.ArgumentParser()\nap.add_argument("--port", required=True)\n'
     'P = os.environ.get("SMIX_RUNNER_PORT", "22087")\n',
     True, "falls back to port 22087 when SMIX_RUNNER_PORT is unset"),
    ("a python gate starting a runner with no port",
     PY_TAKER + 'import subprocess\nsubprocess.run(["smix", "runner", "up", "U"])\n',
     True, "starts a runner without --runner-port"),
    ("a python gate starting a runner on a port it names",
     PY_TAKER + 'import subprocess\nsubprocess.run(["smix", "runner", "up", "U", "--runner-port", P])\n',
     False, "clean"),
    ("a literal in a python string is not a default",
     PY_TAKER + 'USAGE = "attach-gate.py --port 22095"\n', False, "clean"),
    ("no python gate takes a port from its caller",
     'print("nothing")\n', True, "the Python half is reading air"),
]

# The caller-side half: a literal that reaches a correct gate through
# whoever runs it is the same fixed socket, one step further away.
CALLER_CASES = [
    ("a caller handing the gate a literal",
     'bash scripts/dev/probe-e2e.sh --port 22091\n', True,
     "hands a runner-driving script a literal port"),
    ("a caller handing the gate a pinned variable",
     'PORT=22091\nbash scripts/dev/probe-e2e.sh "$PORT"\n', True,
     "which is pinned to a literal"),
    ("a caller handing the gate what it was given", CALLER, False, "clean"),
]


def run(gate_body: str, caller_body: str = CALLER, py_body: str = PY_TAKER):
    """Run the scan over a fixture tree: these two scripts and one Python gate."""
    with tempfile.TemporaryDirectory() as root:
        dev = os.path.join(root, "scripts", "dev")
        lib = os.path.join(root, "scripts", "lib")
        os.makedirs(dev)
        os.makedirs(lib)
        # The real helper, so "sources gate-port.sh" means the same thing
        # here as in the tree. A stub would let this pass after the
        # helper stopped exporting anything.
        shutil.copy(LIB, os.path.join(lib, "gate-port.sh"))
        shutil.copy(SHELL_LINES, os.path.join(dev, "_shell_lines.py"))
        with open(os.path.join(dev, "probe-e2e.sh"), "w") as fh:
            fh.write(gate_body)
        with open(os.path.join(dev, "run-the-gates.sh"), "w") as fh:
            fh.write(caller_body)
        with open(os.path.join(dev, "attach-gate.py"), "w") as fh:
            fh.write(py_body)
        # The scan reads ROOT from its own location: put it where the
        # fixture's scripts/ is the one it walks.
        gate = os.path.join(dev, "gate-port-scan.py")
        shutil.copy(GATE, gate)
        return subprocess.run([sys.executable, gate], capture_output=True, text=True)


def judge(name, r, must_refuse, expected):
    """Exit code AND sentence. Either alone can be right by accident."""
    said = (r.stdout + r.stderr).strip()
    refused = r.returncode != 0
    last = said.splitlines()[-1][:100] if said else "(silence)"
    if refused != must_refuse:
        return [f"{name}: expected {'a refusal' if must_refuse else 'a pass'}, got {last}"]
    if expected not in said:
        return [
            f"{name}: {'refused' if refused else 'passed'} for the wrong reason — "
            f"nothing said {expected!r}, it said {last}"
        ]
    print(f"  {name} → {'refused' if refused else 'allowed'} ({expected!r})")
    return []


def main() -> int:
    failures = []
    for name, body, must_refuse, expected in CASES:
        failures += judge(name, run(body), must_refuse, expected)

    for name, caller, must_refuse, expected in CALLER_CASES:
        failures += judge(name, run(ASKS_THE_OS, caller), must_refuse, expected)

    for name, caller, must_refuse, expected in TALK_CASES:
        failures += judge(name, run(ASKS_THE_OS, caller), must_refuse, expected)

    for name, body, must_refuse, expected in REMOTE_CASES:
        failures += judge(name, run(body), must_refuse, expected)

    for name, py, must_refuse, expected in PY_CASES:
        failures += judge(name, run(ASKS_THE_OS, CALLER, py), must_refuse, expected)

    failures += judge("a caller handing a python gate a literal port",
                      run(ASKS_THE_OS, CALLER + 'python3 scripts/dev/attach-gate.py --port 22095\n'),
                      True, "hands a runner-driving script a literal port")

    # The floor. A tree with no runner in it must be a failure and not a
    # clean verdict: this scan's own history is two ways of seeing
    # nothing and calling it agreement.
    r = run('#!/usr/bin/env bash\necho hello\n', 'echo nothing to run\n')
    if r.returncode == 0:
        failures.append("a tree that starts no runner passed — the scan is reading air")
    else:
        print("  a tree that starts no runner → refused (reading air)")

    if failures:
        print("gate-port-scan.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(
        f"gate-port-scan.test: {len(CASES) + len(CALLER_CASES) + len(TALK_CASES) + len(REMOTE_CASES) + len(PY_CASES) + 2} cases pass — "
        f"a pinned literal, a prefixed invocation and an empty tree are each judged"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
