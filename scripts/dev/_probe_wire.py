"""Reading what the probe answers, in one place.

Three gates parsed `adb shell content call ... --method tree` output with
their own copy of the same regex, and all three went red the day the probe
started returning the screen size beside the tree: `tree=[...]}]` became
`tree=[...], screenW=1080, screenH=2340}]` and a pattern anchored on the
closing brace stopped matching. The probe was answering perfectly; three
gates said it was not there.

A Bundle's text form is not JSON and has no quoting, so this scans brackets
rather than matching a shape — which is also why it survives the next field
somebody adds.
"""

import json
import subprocess
import time


def probe_call(device, app, method, arg=None, extra=None):
    cmd = ["adb", "-s", device, "shell", "content", "call",
           "--uri", f"content://{app}.smixprobe", "--method", method]
    if arg:
        cmd += ["--arg", arg]
    for k, v in (extra or {}).items():
        # `content call` wants a typed binding. Untyped, adb prints its own
        # usage, which a caller reading for a result sees as "no answer".
        cmd += ["--extra", f"{k}:s:{v}"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout


def bundle_field(out, key):
    """One field out of a Bundle's text form, as a string."""
    marker = f"{key}="
    i = out.find(marker)
    if i < 0:
        return None
    i += len(marker)
    if out[i] in "[{":
        opens, closes = ("[", "{"), ("]", "}")
        depth, j = 0, i
        while j < len(out):
            if out[j] in opens:
                depth += 1
            elif out[j] in closes:
                depth -= 1
                if depth == 0:
                    return out[i:j + 1]
            j += 1
        return None
    j = i
    while j < len(out) and out[j] not in ",}":
        j += 1
    return out[i:j].strip()


def probe_tree(device, app):
    """The probe's roots, or None when it did not answer."""
    raw = bundle_field(probe_call(device, app, "tree"), "tree")
    if raw is None:
        return None
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return None


def wait_for_front(device, app, timeout_s=30):
    """Wait until the app is the resumed activity. The activity, or None.

    Two of these gates read the accessibility tree and the semantics tree
    and reconcile them. Neither owned the precondition that both trees
    are of the SAME screen -- the probe lives in the app's process and
    answers with its sixteen tags whatever is on screen, while the
    accessibility side answers with whatever is actually in front. When
    the app had not arrived yet, the reconciliation ran anyway and said
    `compose_input is on the semantics side and not the accessibility
    side, and no rule says why`: a sentence about the release's headline
    feature, and about the wrong thing entirely.

    It was a `time.sleep(2)`. Two seconds is enough on an idle machine
    and not enough after two and a half hours of ship, which is when it
    stopped the eighth one. Same shape as the fixed waits already taken
    out of `android-behaviour-gate`: a fixed wait makes a slow screen
    look like a wrong answer.
    """
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        r = subprocess.run(
            ["adb", "-s", device, "shell", "dumpsys", "activity", "activities"],
            capture_output=True, text=True,
        )
        for line in r.stdout.splitlines():
            if "topResumedActivity" in line and app in line:
                return line.strip()
        time.sleep(0.5)
    return None


def bring_to_front(device, app, timeout_s=30):
    """Put the app on screen and wait until it is actually there.

    Two of these gates read the accessibility tree and the semantics tree
    and reconcile them. Neither of them owned the precondition that both
    trees are of the SAME screen -- so when the fixture was not in the
    foreground, the probe answered with the app's sixteen tags (it lives
    in the app's process and does not care what is on screen) while the
    accessibility side answered with the system status bar. The
    reconciliation then ran, and said `compose_input is on the semantics
    side and not the accessibility side, and no rule says why`: a
    sentence about the release's headline feature, and about the wrong
    thing entirely. Measured 2026-08-29 -- it stopped the eighth ship.

    Returns the resumed activity when the app is up, None when it never
    came. Waiting on the activity rather than sleeping a fixed number of
    seconds, for the reason written in `android-behaviour-gate`: a fixed
    wait makes a slow screen look like a wrong answer.
    """
    subprocess.run(
        ["adb", "-s", device, "shell", "monkey", "-p", app,
         "-c", "android.intent.category.LAUNCHER", "1"],
        capture_output=True, text=True,
    )
    return wait_for_front(device, app, timeout_s)


def wait_for_probe(device, app, timeout_s=30):
    """Wait until the probe answers with a tree. True, or False on timeout.

    Three gates force-stopped the app, started it, and slept two
    seconds. Measured 2026-08-29 on this emulator: after a force-stop the
    app's nodes reach the accessibility tree about three seconds later,
    and the probe is not far behind. Two seconds sat on that boundary --
    fine on an idle machine, not fine after two and a half hours of
    ship, and what came out then was a verdict about the release's
    feature rather than about a screen that had not arrived.

    A fixed wait makes a slow screen look like a wrong answer. Ask.
    """
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if probe_tree(device, app) is not None:
            return True
        time.sleep(0.5)
    return False
