# 05 — CLI reference

> Every subcommand + flag of the `smix` binary. Reference, not tutorial — for tutorial see [01-quickstart.md](01-quickstart.md).

## `smix run` — execute a flow YAML

### `smix run <YAML_FILE>` — run a flow

```bash
smix run [FLAGS] <FLOW.yaml>
```

| Flag | Env | Default | Description |
|---|---|---|---|
| `--device <DEVICE>` | `SMIX_UDID` | (none, required) | Target simulator UDID or Android device id (e.g. `emulator-5554`) |
| `--bundle-id <ID>` | `SMIX_BUNDLE_ID` | (unset) | Default app under test (overrideable by `appId:` in YAML) |
| `--runner-port <PORT>` | `SMIX_RUNNER_PORT` | `22087` | Runner HTTP port. iOS = 22087, Android = 28080 by convention |
| `--platform <ios\|android>` | `SMIX_PLATFORM` | `ios` | Target platform |
| `--apps-config <PATH>` | `SMIX_APPS_CONFIG` | (unset) | Path to `apps.yaml` for cross-platform `app:` logical key resolution |
| `--no-launch` | — | (off) | Skip initial `foreground()` call. Use when you launched the app via other means |
| `--trace-dir <PATH>` | — | `./.smix/trace/<runid>` | Where screenshots / video / JSON traces go |
| `--dry-run` (alias `--check`) | — | (off) | Parse-only gate: validates every listed yaml (+ `runFlow:` includes) and reports `parse OK/FAIL` per file with step counts. No runner, no simulator. Exit 0 on clean parse, 2 on any error |
| `--retry <N>` | — | `1` | Per-flow attempt count; attempts recorded in `~/.local/share/smix/flow-attempts.json` for `smix diagnostic dump` attribution |
| `--debug-output <DIR>` | — | (unset) | Per-step JSON + on-fail screenshot/tree artifacts |
| `--nodes <PATH>` | — | (unset) | Distributed run across machines; see below |

Environment variables consumed by `smix run` (beyond the flag-bound ones above):

| Env | Default | Description |
|---|---|---|
| `SMIX_WEBVIEW_BRIDGE_PORT` | `28080` | Port of the in-app `SmixWebViewBridge` that `webviewEval` posts to on iOS. Infrastructure port like `SMIX_RUNNER_PORT`, not one of the behaviour switches below — it has no `.smix/config.yaml` equivalent |
| `SMIX_AUTO_OCR_FALLBACK` | (off) | `1`/`true`/`yes` — auto-lift bare-string selectors to `fallback: [text, ocrText]` at parse time; `A\|B` regex-OR strings split per alternative on the OCR tiers |
| `SMIX_TAP_OCR_POLL_MS` | `3000` | Poll budget for `tapOn` fallback chains that contain `ocrText` (250 ms cadence) |

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 2 | YAML parse error (`RunError::Parse`) |
| 3 | runtime SDK failure (`RunError::Sdk`) — sim crashed, app died, etc. |
| 4 | unknown key / direction (`RunError::UnknownKey` / `RunError::UnknownDirection`) |
| 5 | runFlow cycle / file IO (`RunError::RunFlowCycle` / `RunError::Io`) |
| 6 | runner unreachable (runner not up / port wrong) |

### Output formats

- stdout has step-by-step progress + final summary JSON
- stderr has DEBUG / WARN lines (when `RUST_LOG=info` or higher)
- Final line is `summary: N steps, X warnings, Y skipped, Z expanded subflows` on success

### Distributed runs across machines (`--nodes`)

`smix run <flows...> --nodes <roster.yaml>` shards the listed flows
round-robin across every device of every node in a roster, runs each
shard remotely over ssh, and merges the results into one JSON document
on stdout. Nodes list simulators/emulators only — the simulator-only
invariant holds across machines.

Roster shape (conventionally `.smix/nodes.yaml`):

```yaml
nodes:
  - name: studio
    host: localhost
    repo: /Users/me/workspace/smix
    devices: [sim-smix-02]
    runnerPort: 22097        # optional; forwarded as --runner-port
  - name: mini
    host: mini
    repo: /Users/me/workspace/smix
    devices: [sim-simx-001]
```

Node preparation is the operator's job, not the CLI's. Before a run,
each remote node needs two steps (the scheduler repo is the authority):

```bash
rsync -a --exclude target/ --exclude .git/ ./ <host>:<repo>/
ssh <host> 'cd <repo> && cargo build --release -p smix-cli && touch target/.smix-fed-stamp'
```

The CLI runs a per-node readiness gate first (binary present, stamp
present, no source newer than the stamp) and fails fast if any node is
stale or unreachable — the gate only judges, it never rebuilds. Flow
paths are repo-relative and must exist at the same path on every node;
a flow missing on the scheduler fails before any ssh is dialed.

```bash
smix run smoke.yaml checkout.yaml --nodes .smix/nodes.yaml --debug-output ./artifacts
```

- **Output**: one merged JSON document on stdout, shaped
  `{"nodes":[{"node":"studio","exit":0,"flows":[…]}],"aggregateExit":0}` —
  the flow leaves are each node's `--format json` report lines verbatim.
- **Exit**: worst of nodes. `255` means an ssh transport failure on at
  least one node (never produced by smix itself, so it cannot be masked).
- **`--debug-output <dir>`**: each node stages its artifacts under
  `.smix/fed-artifacts` in its repo (overwritten in place, never
  pre-cleaned), and the CLI rsync-pulls them back into `<dir>/<node>/`
  after the run. A failed pull fails the whole run.
- **Mutually exclusive** with `--device`, `--also-device` and
  `--parallel`: device placement belongs to the roster. Note that an
  exported `SMIX_UDID` counts as `--device` being present and triggers
  the conflict — unset it when using `--nodes`.
- **Not consulted** in this lane: `--format` (the merged report is
  always the JSON document; remote leaves always run `--format json`)
  and `--runner-port` (ports are per-node — set `runnerPort` in the
  roster instead).

## `smix` — environment + low-level

### Sim management (`smix sim …`)

```bash
smix sim list                  # all sims (JSON: --json)
smix sim resolve <ALIAS>       # alias → UDID
smix sim boot <ALIAS|UDID>     # boot
smix sim shutdown <ALIAS|UDID> # shutdown
smix sim erase <ALIAS|UDID>    # wipe (reset content)
smix sim screenshot <ALIAS|UDID> <out.png>   # simulator → simctl, Android → adb, physical iPhone → the runner (must be up)
smix sim launch <ALIAS|UDID> <bundle-id>
smix sim terminate <ALIAS|UDID> <bundle-id>
smix sim install <ALIAS|UDID> <path/to.app|.apk>   # simulator → simctl, Android → adb; a physical iPhone is refused
smix sim uninstall <ALIAS|UDID> <bundle-id|package>  # every kind; on a physical one, after allow-destructive
smix sim openurl <ALIAS|UDID> <url>   # deeplink; app-level, no device lease needed
smix sim appearance <ALIAS|UDID> light|dark
smix sim keychain-reset <ALIAS|UDID>
smix sim allow-destructive <ALIAS|UDID>   # physical devices only; once, not per command
smix sim exec <ALIAS|UDID> <verb> [args...]
```

### Physical devices

A physical device must be **registered before it can be addressed** — smix
never reaches "whatever happens to be plugged in":

```bash
smix sim register <ALIAS> --udid <UDID-or-SERIAL> --kind physical-ios --name "my phone"
smix sim register <ALIAS> --udid <SERIAL> --kind physical-android
```

The identifier is taken as given for a physical device — a UDID for iOS, an
adb serial for Android — and is not checked against a catalogue, because
there is no catalogue of phones to check against. Registration is the
deliberate act; that is why it is required.

A virtual device is checked against the catalogue its own platform keeps:
`--kind simulator` (the default) against `simctl list devices`, `--kind
emulator` against `adb devices`. Each kind also has its own identifier shape —
a CoreSimulator UDID for a simulator, `emulator-<port>` for an emulator — and
registering one under the other kind is refused, naming the shape that kind
actually uses.

One MCP caveat: `smix_use` cannot *start* a runner on a physical iPhone —
building one needs the registry and a signing team, which are the CLI's to
resolve. Run `smix runner up <alias> --bundle <id>` first; every MCP tool then
drives the phone through that runner exactly as it would a simulator.

Apple identifiers are normalised to upper case, because `devicectl` will not
match a lower-case spelling of a UDID it accepts in upper case. adb serials are
stored and returned verbatim, because `adb` matches them byte for byte.

Destructive actions (`erase`, `uninstall`, `keychain-reset`) are **refused on a
physical device** until you allow them once:

```bash
smix sim allow-destructive <ALIAS>
```

Recorded in the registry, not confirmed per command — a confirmation that has to
be typed every time ends up pasted into a script, which is the same as not
having one. Simulators are never gated: they can be erased and rebuilt in a
minute, and a phone in somebody's pocket cannot.

**Sim safety hook**: bare `xcrun simctl <verb>` is BLOCKED for mutating verbs. Read-only `simctl list` is allowed. To pass-through unmapped subcommands use `smix sim exec <ALIAS|UDID> <verb> ...` (this is accepted by the hook because the device id is explicit).

`<ALIAS>` resolves against this machine's device registry, under
`$XDG_DATA_HOME/smix/devices` (or `~/.local/share/smix/devices`), created and
populated by `smix sim register <alias> --udid <UDID>`. A simulator is an
operating-system object: its UDID, its runtime version and whether destruction
has been allowed on it do not change when you `cd`, so they are recorded once
for the machine rather than once per checkout.

A `.smix/` registry inside a checkout still resolves, as a read-only fallback,
and a device only a checkout knows about is named as such — no other tree can
see it. `smix sim migrate` folds those in; it adds and never removes, so it is
safe to run twice. `smix sim list --registered` shows every recorded device and
which of the two it came from.

A raw identifier works without a registry **only if the platform itself claims
it**: a UDID `simctl` lists, or an adb serial of the form `emulator-NNNN`.
Anything else — an unregistered phone's UDID, an unregistered adb serial — is
refused before the command runs:

```
$ smix sim erase D51116A4-B2AD-5432-8A75-6FBB13F17B58
error: D51116A4-… is not a device smix may address: it is not registered here,
and neither simctl nor adb calls it one of theirs.
If it is a phone or tablet, say so once and it becomes addressable:
  smix sim register <name> --udid D51116A4-… --kind physical-ios|physical-android
```

This is the enforcement of "registered before it can be addressed". It is a
separate question from whether a destructive action is allowed: registering a
phone makes it *reachable*, and `allow-destructive` is still needed before
anything may be taken away from it. Two gates, asked in that order.

### Runner management

`smix runner down` stops the runner **this workspace started**. If the port is
held by a runner it has no record of, it says so and stops rather than ending
it — that runner may belong to another session:

```
$ smix runner down
error: port 22087 is held by a runner this workspace has no record of
(pid 27155), and it is still running.
It may belong to another session — check before ending it:
  ps -o lstart=,command= -p 27155
If it should go, say so:
  smix runner down --include-unrecorded
```

`smix runner up` refuses the same port for the same reason. The two commands
agree deliberately: ending somebody else's runner used to be one keystroke
away, and silent.

They take the same `--runner-port` too. They did not until 2.4.0 —
`down` read `SMIX_RUNNER_PORT` and rejected the flag — so a teardown
written as the obvious mirror of the bring-up failed its argument parse
and left the runner running.

The runner is the on-device server smix drives. `runner up` blocks until
its `/health` answers, so when the command returns you can run a flow.

**iOS** — drives `xcodebuild` against the XCUITest runner:

```bash
smix runner up <ALIAS|UDID> --bundle com.example.app
smix runner up <ALIAS|UDID> --bundle com.example.app --supervise
smix runner list
smix runner down
```

`runner list` is the one to reach for before touching anything: it reads
this machine's ledgers *and* the listening sockets, and where only one of
them has something it says which. A runner nobody wrote down is exactly
the one you cannot decide about from the ledger alone.

**A physical iPhone or iPad** takes the same command, once the device is
registered with `--kind physical-ios`:

```bash
smix runner up <ALIAS> --bundle com.example.app
smix runner up <ALIAS> --bundle com.example.app --team <TEAM_ID>
```

Two things happen that do not on a simulator, both without asking you to
configure anything:

- **The build is signed.** The team is discovered from this machine's
  signing identities; `--team` is only needed when several could sign,
  which is a question smix refuses to answer for you.
- **A port forward opens first.** The runner listens on the *device's*
  loopback, so smix runs a forwarder that makes `127.0.0.1:<port>` reach
  it. It is a separate process (`smix runner forward`, visible in `ps`)
  because it has to outlive the command that started it, and it is
  recorded in the device ledger so a later teardown can find it.
  `runner down` closes both, in that order — the runner's last requests
  still travel through the pipe.

`--bundle` is required: it binds the runner's `XCUIApplication`, and
without it every `/tree` reads the wrong app. `--supervise` attaches a
sidecar that re-cycles the runner if it dies; `runner down` cascades to
it. Port comes from `--runner-port`, else the alias's `runnerPort` in
the registry, else 22087.

**Android** — extracts the runner project if it is not already
installed, builds and installs the instrumentation APK, forwards the
port, and `am instrument`s the Kotlin runner:

```bash
smix runner up emulator-5554 --platform android
smix runner down --platform android --device emulator-5554
```

Nothing to fetch or build first: the runner project ships inside smix,
the same way the Swift one does. The first `up` on a machine extracts it
to `~/.local/share/smix/android-runner/` and runs a gradle build there,
which takes a build's worth of time; later runs find the APK already
built. An Android SDK is required, the way the iOS side requires Xcode.

The device is the adb serial — there is no registry indirection on this
path. Default port is 28080; `--runner-port` sets the host side, which
adb forwards to the runner's own port inside the device. `runner up` is
idempotent: if `/health` already answers on that port it says so and
returns rather than stacking a second instrumentation onto it.

`runner down --platform android` requires `--device`, because an adb
command without a serial acts on whichever device happens to be
attached — and a developer's own phone is often plugged in next to the
emulator. Every adb call smix makes names its device explicitly for the
same reason; note that `gradlew install*` does **not**, so prefer these
commands over gradle install tasks.

It stops the instrumentation and closes the forward — the one adb
actually has, read back from `adb forward --list` rather than assumed
from the port passed in. The package stays installed so the next `up`
does not re-push 50 MB. When you do want it gone — and on a device that
is not yours, you do:

```bash
smix runner uninstall --platform android --device <SERIAL>
```

It asks whether the package is there before removing it. Android answers a
missing package with `DELETE_FAILED_INTERNAL_ERROR`, which is also what a
device policy refusing the removal returns — reading idempotence out of that
string would swallow the refusal. Absent is reported as absent; a refusal
stays an error.

Bringing the Android runner up by hand, if you need to:

```bash
adb -s emulator-5554 install -r -t \
  ~/.local/share/smix/android-runner/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb -s emulator-5554 forward tcp:28080 tcp:28080
adb -s emulator-5554 shell am instrument -w \
  -e class dev.smix.runner.RunnerTest#runServerForever \
  dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner
```

A typo in any of those three coordinates produces `OK (0 tests)` — a
silent no-op that reads as success.

### First run (`smix init`)

`smix init` is the bootstrap: it registers a simulator under an alias, creating
the `.smix` registry that alias-form device refs resolve against. Give it an
`.app` and it boots the device, installs the app, and reads the bundle id out of
the bundle, so the command it prints next is runnable as it stands.

```bash
smix init --device <UDID>                      # register it as `dev`
smix init --device <UDID> --app ./MyApp.app    # …and install the app on it
smix init --device <UDID> --alias staging      # a name other than `dev`
```

It does not choose between devices: with several simulators available and no
`--device`, it lists them and exits non-zero. It also never repoints an alias
that already exists — an alias is what every later command resolves through, so
silently moving one would take over every flow written against it.

### Doctor (housekeeping)

`smix doctor` answers whether this machine can drive anything yet and, when it
cannot, prints the command to run next. Checks stop at the first blocked one:
being told to run `smix init` with no Xcode command-line tools installed would
send you to a command that cannot succeed. `--json` gives the same verdict as
`{ready, checks[], next}` for scripts.

```bash
smix doctor                    # verdict + the next command to run
smix doctor --json             # same, machine-readable
smix down                      # close what is no longer held, then sweep for residue
```

`down` works from the device ledgers first: it closes each recorded resource by
name and prints what it closed — the supervisor before the runner it watches,
because a supervisor exists to restart a runner it finds dead. Pattern matching
on process names runs afterwards, as a backstop for things no ledger covers, and
anything it finds is reported rather than silently swept.

A ledger whose holder is still running, and is not this process, is named and
left alone. The ledgers describe the machine, so the directory holds other
people's sessions as well as yours; "which directory it is in" was never the
question, and "is its holder still there" is one that can be answered.

`down` differs from `smix lease reconcile` in a way worth knowing: `reconcile`
settles what a *dead* holder left behind and refuses to touch a live session,
while `down` is you saying "close what I started" — a running runner is exactly
what it will close.

If XCUITest / xcodebuild processes remain after teardown, start with `smix
runner list`: it prints every runner on this machine with its port and its
device, and says whether the ledgers know about it. `pkill xctrunner` reaches
every runner on the machine including other people's sessions, which is the
failure this ledger exists to prevent — end the one you meant by its pid.

### Screen recording (`smix record …`)

```bash
smix record start <DEVICE> --output run.mov   # begin recording
smix record status <DEVICE>                   # is it recording, and where is it writing
smix record stop <DEVICE>                     # stop, letting the file finish properly
```

A recording is written into the device ledger, which is what makes it
survive the command that started it. `simctl io recordVideo` writes an
mp4's trailer when it receives SIGINT and at no other time, so a recording
whose only handle is one process's memory becomes an unplayable file the
moment that process is killed. The ledger row means `smix record stop`
works from a different shell than `smix record start`, and that a
recording left behind by a killed session is closed properly by
`smix lease reconcile` rather than left writing into nothing.

`status` reports the path, not just yes-or-no — the question people
actually have is where the footage is going. A row whose process is gone
is reported as left behind, with the command that closes it, rather than
as recording.

### Device ledger (`smix lease …`)

`smix runner up` records what it opened on a device in
`~/.local/share/smix/leases/<UDID>.json`. On the machine, not in the checkout: a
lease says who holds a device, what they opened on it and on which port, and
every one of those is a fact about this machine. Stored per checkout, a runner
could hold a port while the tree asking about it saw an empty ledger directory.
When the process holding a session dies without tearing it down — Ctrl-C on
`xcodebuild`, an IDE restart, a CI timeout — that ledger is what lets the next
command find the runner it left behind and stop it the way the dying process
never got to: SIGINT first, so `testmanagerd` ends the XCUITest session cleanly
instead of the runner dying by SIGABRT and macOS raising a crash-report dialog.

```bash
smix lease list                # every device with a ledger, and whether it is in use
smix lease status <DEVICE>     # the holder, what is open, and what is owed
smix lease owner <DEVICE>      # who booted it — exit 0 yes, 3 no record, 1 cannot ask
smix lease reconcile <DEVICE>  # close what an abandoned session left open
smix lease prune               # drop ledgers that no longer describe anything
smix lease prune --dry-run     # ...or say what it would drop, and drop nothing
smix lease migrate --from <DIR>  # fold a checkout's old ledgers into this machine's
smix runner list               # every runner here: port, device, and who knows about it
```

`reconcile` never preempts a live session — it reports the holder and stops.
A session counts as live while anything it started is still running, even
after the command that started it has exited, which is the normal state
once `smix runner up` returns.

Two things it will not do, both deliberate:

- **It never signals a pid it cannot re-verify.** Each row records the process
  start time alongside the pid, because a pid alone gets reissued to unrelated
  processes. A row whose pid now belongs to something else is reported and left
  untouched.
- **It only shuts down a device it booted.** Finding a device already up and
  turning it off would take away someone else's session as the price of
  cleaning up after yours.

### Perf regression (`smix bench`)

```bash
smix bench                      # measure the in-process corpus, compare to the committed baseline, fail on >5% drift
smix bench --update-baseline    # overwrite the baseline with this run (do this on the machine that will gate)
```

| Flag | Default | Meaning |
|---|---|---|
| `--update-baseline` | (off) | Write this run's measurement as the new baseline instead of comparing |
| `--baseline-file <PATH>` | `crates/smix-cli/bench/baseline.json` | Baseline JSON to compare against |
| `--current-file <PATH>` | (unset) | Read the "current" measurement from a file instead of measuring — for tests / CI reproduction |

The absolute `perf_gate` budgets catch a spike; this catches slow drift under them. The baseline holds absolute times from the machine that captured it, so run `--update-baseline` on the machine that will gate (see `crates/smix-cli/bench/README.md` on cross-machine use).

### Low-level probes (use a running runner)

These act on the runner's currently-active sim. Used for ad-hoc debugging without writing YAML.

The selector is a positional argument in `<kind>:<value>` shorthand —
`id:` / `text:` / `label:` / `role:`.

```bash
smix tap id:home-increment-btn
smix tap "text:Submit"

smix find id:home-counter-label            # prints exists=<bool>, exits 0 either way
smix wait-for id:loading-spinner --timeout 5    # seconds, not ms
smix wait-for id:loading-spinner --absent  # wait until it is GONE

smix fill id:form-email-input --text alice@example.com
smix press-key return                      # positional key name
smix scroll "text:Row #5000" --direction down
smix swipe down                            # one gesture; `down` reveals what is below
smix hide-keyboard

smix tree --json | jq .                    # full a11y tree
smix tree                                  # human-readable outline
smix tree --keyboard                       # …including the keyboard's keys
smix describe                              # visible interactive elements
smix system-popups                         # list active popups
smix system-popup-action <popup-id> <button-id>
```

**`find` prints, `wait-for` asserts.** `smix find` writes
`exists=<bool>` and exits 0 whichever it is, so a shell script has to
read its output to branch. `smix wait-for` polls until the element is
there and exits non-zero when the timeout passes, which is what lets it
stand alone in a `&&` chain. `--absent` is the same command waiting for
the opposite: it returns as soon as the element is gone, and fails if it
is still there when time runs out. Use it for a spinner or a modal you
need off the screen before the next step.

**`swipe` is one gesture, `scroll` stops at something.** `smix scroll`
takes a selector and keeps going until it is visible. `smix swipe` does
a single swipe and returns. In both, the direction names what you want
to see — `down` reveals what is below — not which way a finger travels.

**The software keyboard's keys are collapsed.** A key per letter plus
`Next keyboard`, `Dictate`, shift and delete is around sixty nodes that
are the same sixty on every screen of every app, and this output is
read by an AI paying for each one. The keyboard node itself always
shows — a keyboard covering the element you wanted is the explanation
for a failure — and the outline says how many keys were left out.
`smix tree --keyboard` includes them; `smix describe` never enumerates
them, because it is the summary view. To press a key, `smix press-key`
names them directly.

Every one of these takes `--port` and `--device`, and resolves the port
by the precedence at the bottom of this page. `--device` is narrower
here than on `smix run`: it names a UDID or a registry alias purely so
the port that device is registered on can be looked up. It does not
change which simulator the call reaches — the port already does that,
because a runner is a process listening on one.

```bash
smix sim register jp --udid <UDID> --runner-port 22088
smix tap id:home-tab --device jp           # dials 22088, not 22087
```

### Run-script driver (sequential YAML of `smix` subcommands)

```yaml
# script.yaml
- name: boot
  cmd: sim boot
  args: [<device>]
- name: runner
  cmd: runner up
  args: [{from: outputs.boot.udid}, --bundle, com.example.app]
- name: tap-home
  cmd: tap
  args: ["id:tab-home"]
```

```bash
smix run-script script.yaml
```

Use for matrix runs, repeated probes, anything that needs sequential CLI calls. Not for app UI flows — use `smix run` for that.

### Device registry

```bash
smix sim list                                # find the UDID
smix sim register dev --udid <UDID>          # record it under an alias
smix sim register jp --udid <UDID> --locale ja-JP --runner-port 22088
```

`register` writes to this machine's registry, creating it when absent
(`$XDG_DATA_HOME/smix/devices`, or `~/.local/share/smix/devices`;
`SMIX_MACHINE_DIR` moves the whole of smix's machine data, and
`SMIX_SIMS_JSON` names one registry and only that one). Device name,
runtime, and device type are read from `simctl`, so only the UDID and
alias are yours to choose. After this, every command accepts the alias
where it accepts a UDID — from any directory on the machine, not just
the tree you registered it from.

`smix sim unregister <alias>` is the other half: it forgets a name, not
a device, so another alias for the same device keeps working.

What is recorded, per device:

```
ALIAS                UDID                                     KIND       SCOPE
dev                  47ACEAE5-36BA-4C62-811B-F09B397910D7     simulator  machine
```

`smix sim list --registered` prints it. Each row carries the device name,
runtime, device type, and — for a physical device — whether destructive
actions have been allowed on it. `--locale` and `--runner-port` are optional
and set at registration.

These live in a store rather than a file you edit. `smix sim register` and
`smix sim unregister` are the way in and out; a JSON file written by hand is
not read, except for a pre-2.1 `.smix/sims.json`, which is imported once and
then left alone.

## Migration and authoring

### `smix migrate` — maestro yaml to smix

```bash
smix migrate flow.yaml              # rewritten yaml to stdout
smix migrate flows/                 # every .yaml under a directory
```

Renames verbs to their smix spellings (`tapOn` → `tap`,
`extendedWaitUntil` → `expect` + `timeoutMs`, `retry.max` →
`retry.maxRetries`) and drops deprecated argument forms. A verb it does
not recognise is passed through unchanged with a `WARN:` line on
stderr — migrate never silently drops a step it cannot translate.

It also warns about selector keys v2 refuses (`enabled:` is one), since
those fail at parse time and it is better to hear it here than on the
next run.

### `smix authoring` — compose against a live sim

Suggest selectors matching a partial spec, and capture or diff
accessibility-tree baselines for visual gates. Requires a runner.

**Record → generate.** A recording of a live session becomes a flow. The
capture leg is platform-specific (iOS `RecordingApp`, Android accessibility
events, web Playwright injection) but every leg emits the same
`smix-authoring-ir::IRAction` stream, so the generator produces a
byte-identical maestro or rust flow from any of them.

```bash
# live Android session -> maestro flow (records --duration seconds while you drive):
smix authoring tap-record --device emulator-5554 --format maestro -o flow.yaml --duration 15

# an IRAction JSON file (from any capture leg) -> flow, device-free:
smix authoring generate events.json --format maestro -o flow.yaml
smix authoring generate events.json --format rust -o flow.rs --test-fn-name my_test
```

`--format maestro` writes a maestro-compatible yaml flow; `--format rust`
writes an XCUITest-style rust test. `tap-record` is Android-only today (the
Android runner emits IRAction directly); web capture goes through the
`@goliapkg/smix-web-record` Playwright bridge, which writes an IRAction JSON
file that `generate` then consumes. Web recordings generate native flows —
there is no in-browser replay.

`smix authoring propose` closes the loop on a failed run: it reads the
flow plus the on-disk bundle a failed run left behind, asks the local
`claude` CLI to propose edits, applies them, and writes the amended
flow. It is device-free — producing the bundle is the caller's step:

```bash
# 1) produce the bundle by running the (failing) flow on a device:
smix run --device <SERIAL> --platform android --debug-output ./bundle --format json corrupt.yaml > ./bundle/failure.json
# 2) device-free: propose + amend from the on-disk bundle via local claude:
smix authoring propose corrupt.yaml --bundle ./bundle -o amended.yaml
```

The proposal step is non-deterministic (a model is in the loop) and
fenced like `smix-ai-tier`: deletable, opt-in, never on the sense/act
path.

### `smix annotate` — draw on a screenshot

Circle, arrow, text, box and line primitives over a PNG, for failure
reports a human or an agent has to read.

### `smix capsule` — one-command bring-up and teardown

Headless boot, capture and runner start together (`up`), and the
reverse (`down`). The guard rejects a windowed session by default;
`--soft` accepts the soft-capsule fallback.

iOS Simulators only, and it says so rather than trying: all three of its
legs — the Simulator.app guard, `simctl boot`, the `/live` capture — are
simulator machinery with no counterpart on an emulator or a physical
device. Those are brought up with `smix runner up` instead
(`--platform android` for an emulator, `--physical` for a phone).

### `smix diagnostic store` — read the persisted state

```bash
smix diagnostic store               # this workspace's .smix (flows and traces; devices and leases are the machine's)
smix diagnostic store --root PATH   # another store
```

Prints everything smix has persisted, as JSON: the device registry,
runner handles, capsule records and the diagnostic buffers. State used
to be JSON files you could `cat`; this is what replaces that. A value
that is not valid JSON is shown as hex rather than stopping the dump,
because this is what you run when something is already wrong.

## Common command recipes

### Smoke test the running sim quickly

```bash
smix find "text:Welcome" || echo "not on welcome screen"
smix tree | head -50   # see what's on screen right now
smix system-popups     # check for blocking system alerts
```

### Reset to known state between tests

```bash
smix sim exec <device> terminate com.example.app
smix sim exec <device> launch com.example.app
sleep 2
smix find id:home-container   # confirm fresh launch
```

### Capture per-step screenshot (debug aid)

```bash
smix sim exec <device> io screenshot /tmp/s.png && open /tmp/s.png
```

### iOS + Android in one terminal

```bash
# iOS port 22087 (default)
smix run --device <iosudid> --runner-port 22087 --no-launch flow.yaml

# Android port 28080
smix run --device emulator-5554 --platform android \
  --apps-config apps.yaml \
  --runner-port 28080 --no-launch flow.yaml
```

## Environment-variable precedence

```
1. Explicit CLI flag (highest)
2. ENV var (e.g. SMIX_UDID)
3. this machine's device registry, then a checkout's, for aliases
4. Hardcoded default in code (lowest)
```
