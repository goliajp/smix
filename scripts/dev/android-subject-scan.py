#!/usr/bin/env python3
"""An Android gate does not take a system app's version as a contract.

`v2.14-c2` tapped `search_src_text` — a resource id inside whatever
version of Settings the emulator happens to run. On this one it does not
exist, so the tap missed, nothing took focus, 120 characters went
nowhere, and the gate reported that typing had failed. It had not. The
investigation went at the input path, which was working, because the
message pointed there.

That is the same defect the iOS corpus has: twenty of its twenty-one
flows name `com.apple.settings.*` ids that differ by iOS version and
device model, which is why C4a grew a portable tier. This is the Android
half of the same rule.

`test-fixtures/android-app` ships in this repository and is built by the
gate that uses it, so a flow driving it needs nothing from the host but
an emulator.

A script may still drive a system app — some things only a system app
has — but it says so in EXEMPT below, with a reason. Silence is not the
way in: that is how one arrived and stayed.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEV = os.path.join(ROOT, "scripts", "dev")

# Naming a system app, or reaching inside one for a resource id.
SYSTEM_SUBJECT = re.compile(
    r"com\.android\.settings|com\.android\.launcher|com\.google\.android\.\w+|"
    r"\bsearch_src_text\b|\bsearch_action_bar\b|android:id/\w+"
)

# Scripts that drive a system app on purpose, and why. A device gate
# whose subject IS the platform belongs here; one that merely needed a
# text field does not.
EXEMPT = {
    "v2.3-c15-addressability-e2e.sh":
        "its subject is the platform's refusal to address an unregistered device, "
        "not any app",
    "v2.13-c6-guard-e2e.sh":
        "it checks that a blanket `simctl shutdown all` is refused — the verb is "
        "the subject",
}

problems: list[str] = []
checked = 0

for name in sorted(os.listdir(DEV)) if os.path.isdir(DEV) else []:
    if not name.endswith("-e2e.sh"):
        continue
    path = os.path.join(DEV, name)
    try:
        body = open(path, encoding="utf-8").read()
    except OSError:
        continue
    # Comments are excluded, and not as a nicety: the comment explaining
    # why `search_src_text` was removed names it, so a scan reading the
    # whole file fails on the fixed script. Three separate gates in this
    # cycle were fooled by prose containing the thing they forbid.
    code = "\n".join(l for l in body.splitlines() if not l.lstrip().startswith("#"))
    if "android" not in code.lower() and "adb " not in code:
        continue
    checked += 1
    hits = sorted({m.group(0) for m in SYSTEM_SUBJECT.finditer(code)})
    if not hits:
        continue
    if name in EXEMPT:
        continue
    problems.append(
        f"{name} drives a system app: {', '.join(hits)}. Those ids belong to "
        f"whatever version the emulator runs — when one is missing the tap "
        f"lands nowhere and the failure surfaces somewhere else. Drive "
        f"`test-fixtures/android-app`, or add a line to EXEMPT saying why the "
        f"platform is the subject."
    )

for name in EXEMPT:
    if not os.path.isfile(os.path.join(DEV, name)):
        problems.append(f"{name} is exempted here and no longer exists — drop it")

# A scan that reads no Android scripts agrees with any suite at all.
if checked == 0:
    problems.append("no Android e2e script was found — this scan is reading air")

if problems:
    print("android-subject-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"android-subject-scan: clean — {checked} Android e2e script(s), "
    f"{len(EXEMPT)} drive the platform on purpose"
)
