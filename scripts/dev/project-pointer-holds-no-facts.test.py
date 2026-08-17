#!/usr/bin/env python3
"""Harness for project-pointer-holds-no-facts.py.

Each case writes a fixture writer and runs the scanner against it via
SMIX_POINTER_FILE, asserting the fixture really is what the case needs
before checking the verdict — a red must be the scanner's judgement, not
a broken fixture. Covers empty-predicate both sides: presence (the writer
exists) and absence (no device fact in it).
"""
import os
import subprocess
import sys
import tempfile

SCANNER = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                       "project-pointer-holds-no-facts.py")

CLEAN = '''
pub fn set_project_alias(path: &Path, project_key: &str, alias: &str) -> Result<(), E> {
    let store = open_store(path)?;
    store.project_devices().put(project_key, alias.as_bytes())?;
    store.sync()
}
'''

WITH_FACT = '''
pub fn set_project_alias(path: &Path, project_key: &str, alias: &str) -> Result<(), E> {
    let store = open_store(path)?;
    // MUTATION: smuggling a device fact into the pointer
    let sim = RegisteredSim { udid: alias.to_string(), ..Default::default() };
    store.project_devices().put_json(project_key, &sim)?;
    store.sync()
}
'''

NO_WRITER = '''
pub fn something_else() -> u8 { 0 }
'''

fails = []


def run(body):
    fd, path = tempfile.mkstemp(suffix=".rs")
    os.write(fd, body.encode())
    os.close(fd)
    try:
        env = dict(os.environ, SMIX_POINTER_FILE=path)
        return subprocess.run([sys.executable, SCANNER], env=env,
                              capture_output=True, text=True)
    finally:
        os.unlink(path)


def expect(cond, msg):
    if not cond:
        fails.append(msg)


# present-clean: writer exists, only the alias -> green
r = run(CLEAN)
expect("set_project_alias" in CLEAN, "clean fixture missing the writer")
expect(r.returncode == 0, f"clean: expected exit 0, got {r.returncode}: {r.stdout}{r.stderr}")

# M1 (a device fact in the writer) -> red
r = run(WITH_FACT)
expect("udid" in WITH_FACT.lower(), "M1 mutation did not land (no fact token)")
expect(r.returncode == 1, f"M1: expected exit 1, got {r.returncode}")
expect("must hold" in r.stdout or "second place" in r.stdout, f"M1: wrong verdict: {r.stdout}")

# M2 (no writer at all) -> reading air
r = run(NO_WRITER)
expect("set_project_alias" not in NO_WRITER, "M2 fixture unexpectedly has the writer")
expect(r.returncode == 1, f"M2: expected exit 1, got {r.returncode}")
expect("reading" in r.stdout.lower() or "certifying air" in r.stdout, f"M2: wrong verdict: {r.stdout}")

if fails:
    print("project-pointer-holds-no-facts.test: FAIL")
    for f in fails:
        print(f"  - {f}")
    sys.exit(1)
print("project-pointer-holds-no-facts.test: clean — 3 cases (present-clean + M1 fact-smuggled + M2 reading-air), each verdict is the scanner's own")
