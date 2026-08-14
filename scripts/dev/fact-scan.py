#!/usr/bin/env python3
"""Check that user-facing surfaces state facts the source agrees with.

hygiene-scan answers "does this read as internal?"; this answers "is it
true?". The gap between the two is how a landing page shipped a tool
count double the real one and three install coordinates nobody could
install: hygiene-scan strips quoted spans before matching, and every
visitor-visible word on a web page is a quoted span, so the whole
marketing surface was structurally exempt — and no gate compared any
number against the code it described.

Checks:
  1. Version coordinates — every version-bearing install coordinate on a
     user-facing surface equals the workspace version.
  2. Tool-count claims — every "N tools" / "N MCP tools" claim equals
     the number of #[tool(...)] registrations in smix-mcp.
  3. User-facing string noise — the hygiene noise patterns, run WITHOUT
     quote-stripping, over the surfaces whose quoted strings are the
     product (web/, dashboard/ sources).
  4. Installability — the DEPLOYED site tells visitors to install the
     workspace version. If no release tag for it exists, the site is
     promising a package the registries do not serve, so the page must
     say so. Checks 1-3 all passed while smix.golia.jp shipped
     "npm install @goliapkg/smix@2.0.0" against a registry whose latest
     was 1.0.27: matching the workspace version is not the same as being
     installable.

  5. Swept coordinates — checks 1's patterns, found across the tree
     rather than read off a hand-written file list. The list is how the
     Kotlin SDK README kept telling readers to depend on
     jp.golia.smix:smix-sdk:0.1.0 from 1.x into 2.0: nobody added that
     file to it. Paths exempt from the sweep must say why, out loud.
  6. Swift product names — a coordinate can name the wrong THING as
     easily as the wrong version. llms.txt, the file agents read, spent
     two minor lines pointing at `product: Smix`, which Package.swift
     has never offered.

Exit non-zero on any mismatch, so it can gate a release.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return f.read()


def workspace_version():
    m = re.search(r'^version = "([^"]+)"', read("Cargo.toml"), re.M)
    if not m:
        raise SystemExit("fact-scan: no workspace version in Cargo.toml")
    return m.group(1)


# Each entry: (file, regex with ONE capture group = the version it states).
# A file listed here that stops matching is an error, not a skip — a
# coordinate that silently vanishes from the check is how drift starts.
VERSION_COORDINATES = [
    ("README.md", r'jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)'),
    ("README.md", r'from: "([0-9][0-9a-zA-Z.\-]*)"'),
    ("npm/smix-rn/package.json", r'"version":\s*"([^"]+)"'),
    (
        "android-runner/app/src/main/kotlin/dev/smix/runner/SmixRunner.kt",
        r'const val VERSION[^=]*= "([^"]+)"',
    ),
    ("android-runner/sdk/build.gradle.kts", r'val mavenCentralVersion = "([^"]+)"'),
    ("web/src/data/site.ts", r'@goliapkg/smix@([0-9][0-9a-zA-Z.\-]*)'),
    ("web/src/data/site.ts", r'jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)'),
    ("llms.txt", r'jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)'),
    # The lockfile records the workspace packages' own versions, and
    # nothing here looked at it: it sat at 2.3.0 through three majors
    # while every `package.json` moved. It stayed invisible because a
    # lockfile is not a surface anybody reads — until CI's bun got
    # strict enough to compare the two, and then every job that runs
    # `bun install --frozen-lockfile` went red at once, on a commit that
    # had not touched a single npm file.
    ("bun.lock", r'"@goliapkg/smix-cli"[^}]*?"version":\s*"([^"]+)"'),
]

# Check 5 — the same coordinates, found rather than listed.
#
# The list above is a list, and a stale coordinate in a file nobody
# added to it is invisible: android-runner/sdk/README.md told readers to
# depend on jp.golia.smix:smix-sdk:0.1.0 through the whole 1.x line and
# into 2.0, while the root README beside it said 2.0.0 and every check
# passed. So these patterns are swept across the tree instead, and any
# file that is NOT to be swept has to say why, out loud, below.
# One pattern per coordinate form the root README's Install block
# offers. Adding a form there without adding it here is the same hole
# one level up: the Swift Package coordinate went stale at 0.1.0 while
# the swept Maven and npm ones beside it were correct.
DISCOVERED_COORDINATE_PATTERNS = [
    r"jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)",
    r"@goliapkg/smix@([0-9][0-9a-zA-Z.\-]*)",
    r"smix-cli --version ([0-9][0-9a-zA-Z.\-]*)",
    r'from: "([0-9][0-9a-zA-Z.\-]*)"',  # Swift Package Manager
    r'\.exact\("([0-9][0-9a-zA-Z.\-]*)"\)',  # SPM pinned
]

# Paths whose version coordinates are HISTORY and must not be rewritten:
# shipped-version notes to a consumer, and design records, are true as
# of their date. Reported like hygiene-scan's exemptions so a directory
# cannot be quietly excused.
COORDINATE_EXEMPT = [
    (".claude/docs/archive/dogfood-archive/", "shipping notes, true as of the version they announce"),
    (".claude/docs/archive/plan-history/", "archived plans, kept as written"),
    (".claude/docs/v2.md", "decision log; quotes the coordinates it discusses"),
    (".claude/rfcs/", "design records dated to the version they targeted"),
    ("CHANGELOG.md", "every release's coordinates, by definition"),
]

# Check 6 — a coordinate can name the wrong THING, not just the wrong
# version. llms.txt, which is the file agents read, told them to depend
# on `product: Smix` for two minor lines; Package.swift has never
# offered a product by that name. Wrong-version and wrong-product fail
# a build identically, so they are checked identically.
SPM_PRODUCT_CLAIM = re.compile(r"product:\s*\"?([A-Za-z][A-Za-z0-9_]*)\"?")


def spm_products():
    """Product names Package.swift actually declares."""
    return set(re.findall(r'\.library\(name:\s*"([^"]+)"', read("Package.swift")))


# Check 7 — documented constants.
#
# Every "polls 250ms" / "default 3000 ms" / "first 20 nodes" in the
# docs is a number a reader will plan around, and each one lives
# somewhere in the source as a literal. All four claims happen to be
# true; nothing was keeping them that way.
#
# PINS maps a claim to the file whose literal decides it. It is a
# hand-written mapping — but an INCOMPLETE one fails: any numeric claim
# the scanner finds that no pin covers is an error, so adding a
# documented constant without pinning it cannot pass quietly. That is
# the property the hand-listed VERSION_COORDINATES lacked.
CONSTANT_CLAIM = re.compile(
    r"(polls?|default|bare form:|first)\s+(\d+)\s*(ms|nodes|seconds|s)\b"
    r"|(\d+)\s*(ms)\s+(apart)"
    r"|(\d+)\s*(ms)\s+(by default)",
    re.I,
)

PINS = [
    # (claim keyword, unit, source file, regex that must contain the number)
    ("polls", "ms", "swift-bridge/Sources/SmixSDK/Locator.swift", r"milliseconds\((\d+)\)"),
    (
        "polls",
        "ms",
        "android-runner/sdk/src/main/kotlin/dev/smix/sdk/Locator.kt",
        r"(\d+)\.milliseconds",
    ),
    ("first", "nodes", "swift-bridge/Sources/SmixSDK/App.swift", r"prefix\((\d+)\)"),
    (
        "first",
        "nodes",
        "android-runner/sdk/src/main/kotlin/dev/smix/sdk/App.kt",
        r"take\((\d+)\)",
    ),
    (
        "bare form:",
        "ms",
        "crates/smix-adapter-maestro/src/parser.rs",
        r"ceiling_ms: (\d+)",
    ),
    (
        "default",
        "ms",
        "crates/smix-adapter-maestro/src/runtime.rs",
        r"unwrap_or\((\d+)\)",
    ),
    # `runner up`'s wait, which the errors guide names when it tells a
    # reader a cold rebuild may need longer than it.
    (
        "default",
        "s",
        "crates/smix-capsule/src/runner.rs",
        r"SMIX_RUNNER_UP_TIMEOUT_SECS[\s\S]{0,200}?unwrap_or\((\d+)\)",
    ),
    # The parity page said long-press was 700 ms and not configurable;
    # it is 500 and takes `{ duration: N }`.
    (
        "by default",
        "ms",
        "crates/smix-adapter-maestro/src/parser.rs",
        r"LONG_PRESS_DEFAULT_MS: u64 = (\d+)",
    ),
    (
        "apart",
        "ms",
        "android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt",
        r"(\d+)ms is below most",
    ),
]

# Markdown surfaces whose numbers are promises to a reader.
CONSTANT_SURFACES = ("docs/ai-guide/", "swift-bridge/README.md",
                     "android-runner/sdk/README.md", "npm/smix-rn/README.md",
                     "README.md")

TOOL_COUNT_CLAIM = re.compile(r"\b(\d+)\s+(?:MCP\s+)?tools\b", re.I)

# Surfaces whose quoted strings are user-visible copy. Checked raw.
STRING_SURFACES = ("web/src", "dashboard/src", "dashboard/index.html")
STRING_EXTS = (".ts", ".tsx", ".mdx", ".html")

NOISE = {
    "consumer-name": re.compile(r"\binsight\b", re.I),
    "consumer-bundle": re.compile(r"focusai", re.I),
    "ticket-ref": re.compile(r"gol-611", re.I),
    "version-cluster": re.compile(r"v\d+\.\d+(\.\d+)?\s*(c\d|Cluster|Phase)", re.I),
    "round-ref": re.compile(r"\bround-\d"),
    "ask-ref": re.compile(r"\bAsk \d"),
    "plan-ref": re.compile(r"plan-(cold|hot)"),
}

# Where tool-count claims are fair to make and must be true.
TOOL_CLAIM_SURFACES = [
    "README.md",
    "llms.txt",
    "crates/smix-mcp/README.md",
]


def release_tag_exists(version):
    """Is there a tag for this version? Tagging is what publishes.

    Raises when the checkout has no tags at all, rather than answering
    "no". `actions/checkout` fetches none by default, so in CI this
    function saw an empty list and concluded every shipped version was
    unpublished — it turned every run red the moment 5.0.1's
    pre-release banner came out, and it would have said the same about
    a release that had been live for a year. An absent list is not an
    empty one, and the difference is the whole verdict.
    """
    any_tags = subprocess.run(
        ["git", "tag", "--list", "swift-v*"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if not any_tags.stdout.strip():
        raise RuntimeError(
            "this checkout has no release tags at all, so whether "
            f"{version} is published cannot be read from it. In CI, give "
            "the checkout `fetch-tags: true` (or `fetch-depth: 0`); "
            "locally, `git fetch --tags`."
        )
    result = subprocess.run(
        ["git", "tag", "--list", f"v{version}", f"swift-v{version}"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return bool(result.stdout.strip())


# Wording that tells a visitor the version is not out yet. The deployed
# site must carry one of these while the tag is missing.
UNRELEASED_MARKERS = ("not published yet", "unreleased", "pre-release")


def mcp_tool_count():
    return read("crates/smix-mcp/src/main.rs").count("#[tool(")


def iter_surface_files():
    for base in STRING_SURFACES:
        full = os.path.join(ROOT, base)
        if os.path.isfile(full):
            yield base
            continue
        for dirpath, _, names in os.walk(full):
            for name in names:
                if name.endswith(STRING_EXTS):
                    rel = os.path.relpath(os.path.join(dirpath, name), ROOT)
                    yield rel


# What a guide's prose says about behaviour, where the sentence and the
# code can disagree silently. `guide_gate` asks the behavioural half of
# each of these by running the example; these are the half that is a claim
# in words, and words are this scanner's jurisdiction.
#
# The actions guide printed `dispatch: daemonProxy` alongside an `id`
# selector for as long as it did because the prose said one thing and the
# function said another, and nothing asked them the same question.
PROSE_CLAIMS = [
    (
        "docs/ai-guide/04-actions.md",
        "### Tap with explicit dispatch",
        [
            ("/tap-at-norm-coord", True,
             "describes the default tap without naming the route it takes"),
            ("_XCT_synthesizeEvent", False,
             "attributes IOHID synthesis to the default tap; that is "
             "`dispatch: daemonProxy`"),
            ("Path A", False,
             "still describes a Path A / Path B fallback between the two "
             "routes; there is no fallback, `/tap-by-id` is opt-in"),
            ("Path B", False,
             "still describes a Path A / Path B fallback between the two "
             "routes; there is no fallback, `/tap-by-id` is opt-in"),
        ],
    ),
]


def check_prose_claims(failures):
    for rel, boundary, claims in PROSE_CLAIMS:
        text = read(rel)
        if boundary not in text:
            failures.append(
                f"{rel}: no longer contains {boundary!r}, which bounds the "
                f"section these claims are about — the check went blind"
            )
            continue
        section = text.split(boundary)[0]
        for needle, want_present, why in claims:
            if (needle in section) != want_present:
                failures.append(f"{rel}: {why}")


def main():
    failures = []
    version = workspace_version()
    check_prose_claims(failures)

    for rel, pattern in VERSION_COORDINATES:
        text = read(rel)
        stated = re.findall(pattern, text)
        if not stated:
            failures.append(
                f"{rel}: expected a version coordinate matching /{pattern}/ "
                f"and found none — the coordinate moved and this check went blind"
            )
            continue
        for v in stated:
            if v != version:
                failures.append(
                    f"{rel}: states version {v}, workspace is {version} (/{pattern}/)"
                )

    tools = mcp_tool_count()
    if tools == 0:
        failures.append("crates/smix-mcp/src/main.rs: zero #[tool(] found — extraction broke")
    for rel in TOOL_CLAIM_SURFACES + [f for f in iter_surface_files()]:
        path = os.path.join(ROOT, rel)
        if not os.path.isfile(path):
            continue
        for lineno, line in enumerate(read(rel).splitlines(), 1):
            for m in TOOL_COUNT_CLAIM.finditer(line):
                if int(m.group(1)) != tools:
                    failures.append(
                        f"{rel}:{lineno}: claims {m.group(1)} tools, smix-mcp registers {tools}"
                    )

    for rel in iter_surface_files():
        for lineno, line in enumerate(read(rel).splitlines(), 1):
            for name, pattern in NOISE.items():
                if pattern.search(line):
                    failures.append(f"{rel}:{lineno}: {name} in user-facing copy")

    # Check 5 — swept coordinates.
    tracked = subprocess.run(
        ["git", "ls-files", "*.md", "*.ts", "*.tsx", "*.kts", "*.txt"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    swept = 0
    for rel in tracked:
        exempt = next((why for pre, why in COORDINATE_EXEMPT if rel.startswith(pre)), None)
        if exempt:
            continue
        try:
            text = read(rel)
        except (OSError, UnicodeDecodeError):
            continue
        for pattern in DISCOVERED_COORDINATE_PATTERNS:
            for lineno, line in enumerate(text.splitlines(), 1):
                for stated in re.findall(pattern, line):
                    swept += 1
                    if stated != version:
                        failures.append(
                            f"{rel}:{lineno}: install coordinate says {stated}, "
                            f"workspace is {version}"
                        )
    if swept < len(VERSION_COORDINATES) // 2:
        failures.append(
            f"coordinate sweep found only {swept} coordinates — the patterns "
            f"stopped matching and this check would pass by knowing nothing"
        )
    for prefix, why in COORDINATE_EXEMPT:
        print(f"fact-scan: {prefix} — coordinates not swept ({why})")

    # Check 6 — SPM product names.
    products = spm_products()
    if not products:
        failures.append("Package.swift: no .library products found — extraction broke")
    product_claims = 0
    for rel in tracked:
        if any(rel.startswith(pre) for pre, _ in COORDINATE_EXEMPT):
            continue
        try:
            text = read(rel)
        except (OSError, UnicodeDecodeError):
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            # Only claims about THIS package's products.
            if "goliajp/smix" not in line and "package: \"smix\"" not in line:
                continue
            for claimed in SPM_PRODUCT_CLAIM.findall(line):
                product_claims += 1
                if products and claimed not in products:
                    failures.append(
                        f"{rel}:{lineno}: names Swift product `{claimed}`, "
                        f"Package.swift declares {sorted(products)}"
                    )
    if product_claims == 0:
        failures.append(
            "no Swift product claims found on any surface — the pattern "
            "stopped matching and this check would pass by knowing nothing"
        )

    # Check 7 — documented constants.
    constant_claims = 0
    for rel in tracked:
        if not rel.endswith(".md") or not rel.startswith(CONSTANT_SURFACES):
            continue
        if any(rel.startswith(pre) for pre, _ in COORDINATE_EXEMPT):
            continue
        try:
            text = read(rel)
        except (OSError, UnicodeDecodeError):
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            for groups in CONSTANT_CLAIM.findall(line):
                # One regex, three alternations: whichever matched
                # leaves its three groups filled and the rest empty.
                filled = [g for g in groups if g]
                if len(filled) != 3:
                    continue
                if filled[0].isdigit():
                    number, unit, keyword = filled
                else:
                    keyword, number, unit = filled
                keyword = keyword.lower()
                unit = unit.lower()
                unit = "s" if unit == "seconds" else unit
                # Exact (keyword, unit): a looser match let "first N
                # seconds" borrow the pin for "first N nodes" and report
                # a drift where the real answer is "nothing pins this".
                pins = [p for p in PINS if p[0] == keyword and p[1] == unit]
                if not pins:
                    failures.append(
                        f"{rel}:{lineno}: states `{keyword} {number} {unit}` and no "
                        f"pin says which source literal decides it — add one to PINS "
                        f"so the number cannot drift unnoticed"
                    )
                    continue
                constant_claims += 1
                for _, _, src_rel, src_pattern in pins:
                    literals = re.findall(src_pattern, read(src_rel))
                    if number not in literals:
                        failures.append(
                            f"{rel}:{lineno}: states `{keyword} {number} {unit}`, "
                            f"{src_rel} says {literals or 'nothing matching'}"
                        )
    if constant_claims == 0:
        failures.append(
            "no documented constants found — the claim pattern stopped "
            "matching and this check would pass by knowing nothing"
        )

    site = "\n".join(read(rel) for rel in iter_surface_files() if rel.startswith("web/"))
    said_unreleased = [m for m in UNRELEASED_MARKERS if m in site.lower()]
    try:
        tagged = release_tag_exists(version)
    except RuntimeError as cannot_tell:
        # Refused, not guessed, and said in a sentence rather than a
        # traceback: a stack trace and a verdict leave the same exit
        # code, and the reader of a red build has to be told which one
        # this is.
        failures.append(str(cannot_tell))
        tagged = None
    if tagged is None:
        pass
    elif not tagged:
        if not said_unreleased:
            failures.append(
                f"web/: the site states {version} install coordinates, no release "
                f"tag for {version} exists, and no copy says so — visitors are "
                f"told to install a version the registries do not serve"
            )
    elif said_unreleased:
        # The other direction, and the one that lasted longer. The banner
        # above is what silences the check above it, so once written it
        # both told visitors the wrong thing AND kept the gate quiet about
        # it: through 4.0 and 4.1 the site said "4.1.0 is not published
        # yet ... the registries currently serve 1.0.27" while npm served
        # 4.1.0. An escape hatch nothing checks the far side of becomes
        # the place a falsehood hides.
        failures.append(
            f"web/: a release tag for {version} exists and the site still says "
            f"{said_unreleased[0]!r} — that copy exists to cover the gap before "
            f"a release and has to come out with it"
        )

    if failures:
        print(f"fact-scan: {len(failures)} falsehood(s) on user-facing surfaces")
        for f in failures:
            print(f"  {f}")
        return 1
    print(
        f"fact-scan: clean — coordinates all say {version}, "
        f"tool claims all say {tools}, no noise in user-facing strings"
    )
    return 0



if __name__ == "__main__":
    sys.exit(main())
