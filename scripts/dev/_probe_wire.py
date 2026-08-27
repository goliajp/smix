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
