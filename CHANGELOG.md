# Changelog

All notable changes to the `smix` workspace are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) at the wire, ABI, and CLI surface.

## [10.0.0] — 2026-08-28

**Compose apps can be driven by id everywhere, including inside a dialog** —
if the app opts in. Everything works exactly as before without it, and says
so rather than answering worse.

smix has always perceived through the accessibility tree. For React Native
that is a faithful mirror: N real views, one node each. For Jetpack Compose
it is not — the whole UI is one `AndroidComposeView` and the nodes are
synthesised from semantics, asynchronously and lossily. That assumption cost
a shipped regression at 6.4.0, where a consumer's Android suite went from
eleven passing to twenty red while every action it judged had worked.

### Added

- **An optional in-process probe: `jp.golia.smix:smix-probe`.** One line in
  an app's debug build (`debugImplementation`), no app code, no release
  footprint. smix then reads the semantics tree itself. Measured on the
  fixture: with a Compose dialog open the accessibility tree carries **zero**
  of the app's ids and the probe carries seventeen, and `smix find
  id:compose_submit` answered `exists=false` about a button plainly on
  screen.
- **`smix tree` says which reader answered**, in `--json` as `source` and in
  the outline as a header line naming what that reader cannot see.
- **A flow can be run from XCTest and from JUnit.** `SmixFlow.parse` in both
  Swift and Kotlin reads the report `smix run --format junit` already writes,
  so a failure arrives as whatever the host framework calls one and the flow
  executes on exactly the path CI uses.

### Changed

- **A tap that cannot land is refused instead of reported as a success.** On
  iOS a modal leaves what is behind it in the accessibility tree and the
  presentation swallows touches aimed there; smix exited 0 and the app did
  not move. It now fails and names what is covering the element. A runner
  that does not report hit-testing says nothing, and silence still proceeds.
- **`smix find` adds `reachable=false`** for an element that is on screen and
  covered. It still answers `exists=true`, because it is there.
- **The probe stages the screen; the touch stays real.** Its action surface
  is only what brings a node within reach of a real touch. A semantics
  `OnClick` fires the composable's lambda with no hit-testing at all —
  measured firing through a dialog's scrim onto a button a real touch could
  not reach — so it is refused, and the refusal names what to use instead.
- **With a modal open, what is behind it is not addressable through the
  probe.** The probe can see it; a user cannot touch it.

### Breaking

- **Reading the tree returns the reader with it.** `HttpRunnerClient::get_tree`
  hands back `PerceivedTree { source, root }`.
- **Screen nodes carry whether a touch would land.** `A11yNode` has a
  `hittable` field.
- **The TypeScript addon's tree arrives in the same envelope.**
  `snapshotTree()` returns `{ source, root }`. Take `.root` for the tree.

`PerceivedTree` is breaking on purpose: the compiler asking each call site
which reader answered is the mechanism, not a side effect. A screen the
accessibility reader has gone blind on and a screen with nothing on it print
identically otherwise.

## [9.0.0] — 2026-08-26

**Flows and CLI are unchanged.** The major is one Rust enum:
`RunnerTransportError` is now `#[non_exhaustive]` and has a variant for a
refusal that names itself. If you do not match on it in your own Rust,
this is a patch wearing a major's number.

### Changed

- **`hideKeyboard` says WHICH failure it was.** A consumer met
  `runner /hide-keyboard answered ok:false — the action did not happen`
  with the keyboard unmistakably on screen, and could not tell it from
  the answer they would have got for no keyboard at all. Three
  situations reached them as that one sentence: every dismiss strategy
  ran and the keyboard stayed, XCUITest raised while looking, and the
  request context was lost. They want opposite responses — look at the
  screen, look at the runner, or do nothing.

  The route names its case now (`keyboard_did_not_close` /
  `keyboard_state_unknown`), carries what it observed (which strategies
  ran, and which element still holds keyboard focus), and the failure
  hint points at the next step for that case. An exception is no longer
  reported as evidence the keyboard is up.

  The two successes stay one answer: whether the keyboard was already
  gone or was just dismissed, the caller's next step is the same.
  **`hideKeyboard` was already idempotent** — no keyboard is `ok:true`,
  and has been — so a guard around it was never needed. The old failure
  just made it look that way.

- **The outside touch aims at the keyboard's own edge.** The strategy
  that dismisses a React Native keyboard by touching outside it used a
  flat 15% of the app frame, which on a login form is where the *other*
  field is: the touch moved focus rather than releasing it, and the
  keyboard stayed. It now taps just above the keyboard's own top edge,
  and falls back to the fixed point only when the keyboard's frame reads
  as empty.

### Breaking

- **Transport errors are `#[non_exhaustive]`.** An exhaustive `match`
  on it needs a `_` arm.
- **A refusal can name itself: `RefusedNaming { endpoint, kind, saw }`.**
  Handled by the `_` arm above; match it if you want the detail.
- **The hide-keyboard handler returns an `Outcome`, not a `Bool`.**
  Swift only, and only if you construct the server yourself.

`#[non_exhaustive]` is the "pay once" move, and it is the right one here
for the reason it was wrong for `Step`: a transport error a caller has
not heard of lands in their `_` arm, which is where it would have landed
anyway — while a verb they have not heard of would be silently skipped.

## [8.0.1] — 2026-08-25

### Changed

- **A runner that went away says so, once, instead of leaking a socket
  error per step.** A consumer's Android instrumentation was killed
  mid-suite under memory pressure, and every step after that reported
  `error sending request for url (http://127.0.0.1:22089/tree)` — seven
  in a row, reading as seven problems when it was one. A refused
  connection now names the runner, the usual Android cause, and the way
  back, and says that the steps after it will report the same thing.
  Narrow: a timeout is a runner that IS there and struggling, and
  "restart it" is the wrong instruction for that.

### Documentation

- **`smix tree --json` says what `bounds` is measured in**, which is not
  the same on both platforms: iOS frames are **points** and Android
  bounds are **pixels**. A 44pt rule compares directly on iOS; a 48dp
  rule needs `px / (densityDpi / 160)` first, which on a 440dpi device
  makes it 132px. Ported without the conversion, a size checker either
  passes everything or fails everything, and both look like a working
  checker. Asked by a consumer porting theirs from Android to iOS.
- **`smix wait-for` is named beside `extendedWaitUntil`.** The flow verb
  was the only one documented, so a harness driving the runner from
  outside a flow wrote a one-step yaml to ask "is the app up" before
  finding `wait-for` in `--help`. The choice is "in a flow or around
  one", not which is better.
- **`swipe: { over: … }` takes `fallback:`**, with an example — a flow
  driving both platforms needs it when the same element is named
  differently on each. A consumer wrote it on a guess because neither
  the guide nor the support matrix said so.
- **A bare `from:`/`to:` share is the **y** axis** with `x` centred. The
  guide said "the axis" without naming it, and a horizontal drag written
  with bare numbers runs down the middle instead — two numbers that mean
  something else rather than an error.

## [8.0.0] — 2026-08-25

**Flows and CLI are unchanged.** No verb was renamed, no flag removed,
nothing moves on the wire. The major is four Rust signatures and six new
enum variants, and two answers that are different on purpose. If you
write YAML and run `smix`, read the two behaviours below; if you depend
on the crates, [Migrating to smix 8.0](docs/migrating-to-8.md) is short.

### Changed

- **A port is checked against the device you named, before anything is
  driven.** `--device` is what a caller aims with, and nothing checked
  that the port it was paired with went anywhere near that device. An
  adb forward can point a local port at a different emulator — or at a
  phone on the desk — while every guard in §9 #1 passes, because they
  all read the `--device` string rather than asking who answers.

  It cost an hour of this cycle to find out: an investigation ran
  against `localhost:28080` believing it was `emulator-5554`, and it was
  forwarded to `emulator-5556`. Two second-hand facts disagreed for that
  hour and both were telling the truth about different machines.

  The runner cannot settle it — both runners' `/health` are byte
  identical across devices — so the authority is host-side and live:
  `adb forward --list` on Android, the process bound to the device on
  Apple. Mismatch names both sides and says which device it would have
  acted on. It refuses only for that: something serving the port that
  cannot say which device it reaches (a tunnel, the behaviour gate's
  recording proxy) proceeds with a warning, and a port nothing holds at
  all is left to the connection, which reports "no runner is listening"
  — the sentence a reader can act on. Both narrowings were taught by a
  real setup this guard had wrongly stopped.

- **`inputText` on iOS refuses when nothing has focus.** It reported
  success — eighteen characters, no warnings, no field changed — while
  Android refused the same flow by name. The wire has carried
  `Input-Dispatch-Mode` since v1 and no runner read it, so
  `--force-key-events` changed nothing; and the condition deciding
  whether to resolve focus excluded `_focused_`, the one selector the
  mode is about. Both platforms now say the same sentence.

- **`waitForAnimationToEnd` tells "could not look" from "still
  moving".** Capture backpressure inside the wait is absorbed and
  retried; a capture that never succeeds is reported as such rather than
  as motion.

- **`DeviceControl::set_animations_quiet` no longer defaults to
  success.** It returned `Ok(())` three lines under a comment saying
  that a switch reporting success while the device keeps animating is
  worse than no switch. Every backend overrides it, so the default was
  waiting for the next one to inherit a silent no-op.

- **A ledger row written by a newer smix is now read, kept, and refused —
  not choked on.** 7.0.0's release notes described an older smix stopping
  dead against a device whose ledger held a claim, and called that the
  safe direction. It was, and it was also not the whole answer, because
  nothing stopped the same thing happening again at the next row.

  Being exact about which half was inherent: **for 6.x it was.**
  `Resource` is internally tagged, serde rejects a tag it has never heard
  of, and 6.x's parser had already shipped — no row 7.0.0 could write
  would have changed that. **For 7.x onward it is not.** Forward
  compatibility can only live in the reader, and can only be added before
  the writer needs it, so it is added now: a row this binary cannot name
  parses into `Row::Unnamed` holding the row's whole object.

  It keeps the object rather than just the tag, because a reader that
  remembers the name and forgets the contents destroys the row the next
  time it writes the file back. Every `retain` in the store defaults an
  unreadable row to kept for the same reason.

  What it does not do is act. An unnamed row withholds `may_shut_down`
  even from a holder that booted the device — something is open there that
  nothing here can close — it is not a service, so nothing adopts past it,
  it counts as possibly-alive rather than dead, and it appears in the
  cleanup plan as `CleanupAction::CannotClose { kind }`. A teardown
  carrying one reports `Outcome::Failed` naming the kind, because a plan
  that silently omitted what it could not read would report itself
  complete.

  Rust API: `Lease::resources` is `Vec<Row>` rather than `Vec<Resource>`,
  `Lease::known_resources()` iterates the ones this binary understands,
  `unnamed_kinds()` names the rest, and `CleanupAction` gained a variant.

### Added

- **`swipe: { over: <selector>, from: …, to: … }`** — a swipe aimed
  inside a named element, by shares of *its* box rather than of the
  screen. A consumer with a nameable timeline still had to measure it
  (45.3–50.5% of screen height on Android, 47.7–53.2% on iOS, 49% taken
  as the overlap, and a note that a differently shaped device would need
  measuring again). Both ends resolve from one tree read, and a missing
  element is refused rather than falling back to a share of the display
  — which is how a drag that missed comes to look like one that worked.

- **`role:keyboard` in the Android tree.** The runner already knew:
  `keyboardIsUp()` reads the window type and `hide-keyboard` decides on
  it. Nothing lifted it into the tree, which is the one place every verb
  can reach, so `extendedWaitUntil { visible: { role: keyboard } }`
  worked on iOS and timed out on Android with the keyboard on screen.

- **`ACTION_PLATFORMS`** — what each kind of device can and cannot do,
  in one table both backends read. §9 #1 has required a loud refusal
  since physical devices landed; `DevicectlClient` did refuse seventeen
  actions by name, but they were seventeen sentences in seventeen method
  bodies with nothing able to say whether the set was complete, and the
  other three kinds of device were never asked. Two answers per cell,
  never three, and "refuses by name" means it can say what, why, and
  what to do instead.

### Fixed

- A runner answering 404 with `{"error":"not_found"}` reached the reader
  as `DRIVER_ERROR` with the wire body attached. It is an element that
  was not found, and says so — narrowly, so a genuinely broken route
  stays a driver error.

- `known-unstable-scan` returned an empty list when its file was
  missing, looped over nothing and exited 0. An empty table states that
  nothing is excused; a missing file states nothing while the corpus
  gate goes on excusing.

- Both Android clipboard actions were documented as working and have
  been refused since Android 10 sealed the clipboard to the foreground
  app.

## [7.0.0] — 2026-08-24

**Nothing moves on the wire, in the YAML verb table, or on the CLI.** The
major is one Rust API change in `smix-lease`, which is not one of the ten
ABI-frozen crates: `Resource` gained a variant and became
`#[non_exhaustive]`. A flow, a runner, a `smix` command line and a data
directory written by 6.8.0 all mean exactly what they meant. If you do not
match on `smix_lease::Resource` in your own Rust, this is a patch wearing a
larger number, and the number is the arbiter's rather than a claim about
your upgrade.

This release began as a dependency move. The ship stopped forty minutes in
on a device gate that was right, and fixing what it found is the rest of it.

### Breaking

- **`smix_lease::Resource` is `#[non_exhaustive]` and gained `Claimed`.**
  This is the whole of the major. The list has grown five times
  and adding to an exhaustive public enum is what `cargo semver-checks`
  charges a major for, so the charge is paid once rather than again at the
  next kind of thing a holder can open. Downstream Rust matching on
  `Resource` needs a `_` arm; nothing else is affected.

  **An older smix stops against a device whose ledger holds a claim**, and
  it is worth being exact about this rather than calling it a parse
  detail. It does not skip the row: it reports `unknown variant
  'claimed'` and refuses the command. That is the safe direction — a
  reader that silently ignored a resource row it did not understand could
  tear down a device while blind to something still open — but on a
  machine running both versions, 6.x will stop on any device 7.x has
  claimed. Measured rather than reasoned about: a 6.8.0 on this machine
  refused `emulator-5554` the moment 7.0.0 claimed it.
  `smix lease release <device>` with the newer binary, or removing that
  device's file under `~/.local/share/smix/leases/`, restores it. Nothing
  else in a ledger is affected, and no data of yours is in one.

### Added

- **`smix lease claim <device>` / `smix lease release <device>` — answer for
  a device this machine did not boot.**

  Gates and scripts ask `lease owner` before driving anything, and the only
  answer that let them through was "smix booted it". So a machine's own
  dedicated emulator, started by hand and sitting idle, was drivable by
  nobody: the row that made a device drivable was the same row that made it
  shut-downable, and nothing may claim to have booted what it did not.

  The way past it was `SMIX_ANDROID_SERIAL`, a per-command environment
  variable — it records nothing, it is gone when the command exits, and so
  every run made the same decision again with no way to read the last one.

  A claim grants exactly one of the two things a boot row grants: **yours to
  drive, still not yours to switch off.** `may_shut_down` and every teardown
  path keep reading only the boot row, because the claim's own content is
  "nothing here booted this". It is refused while a live session holds the
  device, and the ledger ends it when the device goes off.

  `lease owner` answers `0` for a claim as well as a boot and says which in
  words, so callers acting on its exit code inherit this without changing —
  `pick-dev-emulator` and `pick-dev-sim` were not touched, and the Android
  instrumentation gate now passes with no environment pin at all. The
  refusal messages point at the claim first and say plainly that the
  environment variable records nothing.

### Changed

- **The embedded store is built against `kevy-embedded` 5.4.1.** 5.4's
  headline is the packed row — a declared table's row stored as one
  allocation rather than a per-row hash table — and the reason to mention
  it is that it does not reach an embed. `packed-rows` is a `[server]`
  config key in kevy's server binary; the published `kevy-embedded` does
  not contain the word, and the setter behind it is neither called nor
  re-exported by the facade. A declared table would be needed on top of
  that, and smix declares none. A row here keeps the general
  representation whatever the server's default becomes.

  Nothing moves on the wire or on disk, and that was checked rather than
  read off a release note — both directions, against the 7.3 MB store one
  machine has been writing for months rather than a fixture. 5.4.1 and 5.3
  each read the same bytes to the same 258 records, byte for byte; then
  5.4.1 wrote to a copy, 5.3 reopened it, replayed clean, and returned all
  35 keys, the only one changed being the record that write touches. **A
  6.8.0 smix still opens a data directory a 6.8.1 smix has written.**

### Gates

- **Every fuzz lockfile must still satisfy the manifests above it.** The
  kevy bump left four of them pinning 5.3.0 against a requirement that now
  reads 5.4.1, and a full green CI run said nothing: no gate read those
  files, and `--locked` appeared nowhere in the workflow. The bump before
  it left the same wreck and was tidied by the next release regenerating
  everything — which reads like a convention and is really the next
  person's cargo command rewriting a file nobody checked. An unsatisfiable
  lockfile pins nothing.

  The check is `cargo metadata --locked` over every `crates/*/fuzz`. It
  deliberately does not compare versions against the workspace lockfile:
  those trees resolve independently and 332 transitive packages differ
  between them today, every one of them a valid resolution.

  Two of the fifteen fuzz crates shipped with cargo-fuzz's default
  `.gitignore`, which ignores `Cargo.lock`. Both are tracked now —
  otherwise the way to pass this gate is to stop tracking the file.

- **The A4 window verdict can report what it finds.** It died as a
  `TypeError` with this release's own ship at thirty-one minutes:
  `sorted({...})` over window packages, one of which was `None`. A window
  whose root cannot be read has no package to report — which is also why
  the branch written to tell "not attached" from "attached but unreadable"
  could never be reached, since readability was only ever asked of windows
  already matched by package. Both are fixed, readability is asked first,
  and the judgement moved into `android-a4-verdict.py` so a self-test can
  drive it on the exact payload that killed it, with no device.

- **A4 waits for the window instead of assuming three seconds.** The same
  red was a race: nine seconds after the instrumentation gate finished
  driving the same emulator, the fixture's window was attached and its
  root not yet readable. It now polls to a deadline
  (`SMIX_A4_SETTLE_S`, 30s), so a red says the window never became
  readable rather than that nothing waited long enough.

## [6.8.0] — 2026-08-23

### Fixed

- **A field named by the layout around it can be typed into again.**
  6.7.1 asked the runner for a focused field containing the tap point,
  to stop a named fill reaching whichever field held focus before. An
  app that carries the contentDescription on the wrapping layout and
  nothing on the input — how hand-written Kotlin views are written —
  resolves to the wrapper, whose centre sits on whichever child is
  tallest. Tapping a label focuses nothing, so every fill in such an
  app answered `no_focused_field`. Reported against 6.7.1 the day it
  shipped.

  A fill aimed at something not itself typeable, holding exactly one
  thing that is, now taps that one — exactly one, because with two the
  choice is a guess. And the focus check takes the named element's box
  rather than its centre: the field meant is the one lying inside what
  was named, which is still not the field stacked below it.

### Added

- **Every verb now says what it does with every selector spelling.**
  `ocrText:`, `localizedText:` and `anchored:` describe something the
  accessibility tree cannot show, so a verb either reads them above the
  resolver or refuses them by name — and the refusal says what to write
  instead. Which verb does which is one table, generated into
  [the selector guide](docs/ai-guide/03-selectors.md) from the one the
  code decides by, so the two cannot drift apart.

  Eight cells that used to fail silently are dispatches now:
  `assertVisible`, `doubleTapOn`, `longPressOn` and `inputText` read
  OCR; `doubleTapOn`, `longPressOn` and `inputText` take an anchor and
  a shift; `repeatTap` reads a locale map. The rest refuse — an OCR
  miss is not evidence of absence, and `copyTextFrom` reads what an
  element holds rather than what its pixels look like.

- `App::double_tap_at_coord` and `App::long_press_at_coord`, the
  counterparts of `tap_at_coord` for the verbs that reach a place
  rather than an element.

### Changed

- **`anchored:` parses wherever a selector parses.** It used to be
  readable only inside a `fallback:` chain, so the same spelling was
  writable in one verb and a parse error in another, and nothing said
  which. Whether a verb *reads* it is now decided in the table rather
  than by which parser happened to see it.

- **`inputText`'s mapping form takes any selector, not only `id:`.**
  `text:` is what to type, so it cannot also name the element — but a
  field addressed by label, role, OCR or a fallback chain simply could
  not be typed into, and the guides said otherwise.

- A verb handed a form it does not read now fails with that reason
  before it does anything, rather than resolving against the tree,
  finding nothing, and reporting the target absent.

## [6.7.1] — 2026-08-22

### Fixed

- **`localizedText:` now works in every verb that takes a selector.** It
  is rewritten to a plain `Text` selector by a pure function of the
  current locale, and three of the twelve verbs called it — the rest
  handed the locale map to the resolver, which matches nothing against
  one. Measured: the same element found by `assertVisible` and not by
  `longPressOn`, same selector, same screen.

  The gate selectors are the more serious half. `when.visible` /
  `when.notVisible` probe through the same path, so a gate written
  against a locale map fired because the question went unasked — a
  wrong branch taken in silence rather than an error anyone sees.

- The guides said "any selector position accepts `ocrText:`" directly
  above the list of the four verbs that fire it. They now say which
  verbs read it, and what the others do instead.

## [6.7.0] — 2026-08-22

### Fixed

- **A fill into a masked field is no longer judged to have landed
  nowhere.** A password field's accessibility node reports one bullet
  per character and never the characters, so the read-back — which
  compares the node's text with what was typed — was false for every
  such fill that ever worked. Masked fields are now judged by how much
  longer they got, keyed on the node reporting itself as a password
  rather than on the text looking like a mask: a plaintext field holding
  `aaaa` is still checked by content.

- **A named fill goes to the field it names.** `fill` taps the field to
  focus it and then asks the runner to clear and type, and the runner
  acted on whatever held focus at that instant. Focus does not move
  synchronously with the tap, so both halves could still reach the
  previously focused field — the characters landed in the wrong field
  and the wrong field was emptied first. The request now carries where
  the caller tapped, and the runner waits for focus to reach that field.
  A scalar `inputText`, which names no field, still means "wherever
  focus is". `clear` had the same shape and is fixed with it.

- **A `fallback:` chain is tried layer by layer in every verb.** It
  worked in `extendedWaitUntil`, `tapOn` and the OCR probe, and matched
  nothing anywhere else — `assertVisible` with a chain failed on both
  platforms. Order is a promise: `[id, text]` prefers the id even when
  both match. Where a verb returns every match, a chain now gives the
  first layer that matched rather than the union. A layer whose pattern
  cannot compile no longer discards the layers after it.

- **A failure says which step it happened at.** Errors read
  `step N (verb): …`, and a `STEP N: verb → FAILED` line joins the
  skipped one. A subflow's inner step is named once; the `runFlow`
  containing it does not claim the failure.

### Changed

- **An absence check refuses to pass on a question it never asked.**
  `assertNotVisible` and `waitForNotVisible` now fail loudly when the
  selector contains `ocrText`, `localizedText` or `anchorRelative` —
  forms this layer does not read from the accessibility tree. They
  matched nothing, and "matched nothing" was reported as "absent":
  `assertNotVisible: { ocrText: 'smix fixture' }` passed against a
  screen showing those words. A chain counts too, since a chain that
  was not evaluated to the end says nothing either. `assertVisible`
  keeps failing as before but now says which part was never checked.

  If a flow relies on one of those passing, it was not checking
  anything; name the element by `id`, `text`, `label` or `role`, or use
  a verb that evaluates the form — `tapOn`, `extendedWaitUntil`,
  `scrollUntilVisible`.

- `localizedText` inside a `fallback:` chain is now desugared. The layer
  used to stay a locale map, match nothing, and let the chain fall
  through to the next one, so the locale the author wrote was never
  consulted.

### Added

- A test that every selector form either resolves or is listed as
  unresolvable with a reason — and that each listed reason is still
  true. `Fallback` spent its whole existence matching nothing because
  the resolver said the adapter would handle it and nothing checked.

- The Android behaviour gate drives a masked field (A9) and a fill that
  names a second field while another holds focus (A10). Its assertion
  count is derived rather than typed; it said 9/9 while the file held
  nine and would have gone on saying it while holding ten.

## [6.6.0] — 2026-08-21

### Added

- **`smix-contract`** — what an app owes, with an id, so coverage can be
  reconciled. It reads contract files and per-platform claims and
  answers three sets: which requirements nobody claims, which some but
  not all of the expected platforms claim, and which all of them do.

  It reports who **claimed** a requirement and never that anyone
  verified it. Those are different words: a claim says a suite means to
  cover something, not that the test is good, that it passed, or that
  two platforms' tests check the same thing — the last of which is not
  mechanically decidable. Reporting it as coverage would make this one
  more cheap signal standing in for the thing to be proven.

  What it refuses matters more than what it parses. An entry with no id
  cannot be claimed by anything. An entry with an empty statement passes
  a check for the field and stands for nothing. One id on two entries
  makes every claim on it ambiguous. A claim naming an id no contract
  carries leaves the requirement it meant looking unclaimed while
  somebody believes they are covering it. Each refusal names the field
  and where it was read.

  **A test declares what it covers in a comment:** `// covers: CTR-0001`,
  above the case. A comment rather than an annotation, because an
  annotation would require the product's unit-test target to depend on
  smix — which inverts what this is for. One line may claim several ids,
  case and spacing are forgiven, and `coverage:` is not: forgiving a
  different word would stop "looks like a claim" and "is a claim" being
  tellable apart.

  **The whole tree reconciles at once**, in every notation it holds —
  contracts in a file, claims in a file, claims in the source. A source
  file's platform is read from its path and refused when the path does
  not say, because guessing wrong makes a requirement covered on one
  platform read as covered on both.

  **A ratchet forbids losing coverage without demanding more of it.** A
  platform that used to claim a requirement and does not is a failure. A
  new requirement nobody covers yet is not — that is work to do, and
  calling it a failure would be a coverage target wearing another name.
  A deleted requirement is said out loud and does not block. The baseline
  lists ids and platforms rather than a count, so losing one shows up in
  a diff as a line with a name on it.

  **The verdict names what to act on and carries no percentage.** Each
  line gives the requirement, the sentence it stands for, which platform
  is missing, and where the claim that exists was read. There is no score,
  and a test asserts there is none: a percentage is met by writing claims
  rather than by covering anything, and adding one later should be a
  deliberate act against a test.

### Fixed

- **`back` now taps the way `tap` taps.** `XCUIElement.tap()` dispatches
  through Apple's gesture recognizer chain, and this repository has
  measured that chain failing to reach handlers a raw IOKit touch
  reaches — which is why `/tap` carries a synthesized-touch mode at all.
  `back` never had it.

  The failure it fixes was a corpus flow that failed on CI and passed
  everywhere else: the right button (identifier `BackButton`), hittable,
  tapped, and the title unmoved for the full budget on both strategies,
  while the same flow passed on retry. A hittable correct button that a
  tap does not move is the shape that mode exists for.

  It is a second strategy, not a replacement. The ordinary tap runs
  first and unchanged, so nothing that works today takes a different
  path.

- **`runner cycle` says it has no Android path instead of answering
  about iOS.** It took neither a platform nor a device, dispatched into
  the iOS path, and read iOS's `state.json` — so typed against an
  Android runner it looked in another platform's records, found
  nothing, and answered "no runner recorded". A consumer reported that
  sentence as a state problem, because that is what it reads as. The
  verb had never worked there: there is no `cycle` in the Android
  runner at all.

  It takes `--platform` now, the same shape `down` and `up` use, and
  refuses Android by name with the two commands that do the job.

- **`/back` says what it saw.** `settledBy: gaveUp` reported a spent
  budget and nothing else, covering "there was no back button", "it was
  tapped and the title never moved" and "the edge gesture did nothing"
  with one word. The response now carries a `saw` field — which button,
  whether it was hittable, the title before and last seen, and how many
  frames had no navigation bar — on every path, not only the refusals.
  A green run is where you find out which strategy carried it.

## [6.5.0] — 2026-08-21

6.4.0 replaced three answers that came back `ok` without being asked with
predicates that read the result back. On a Compose app those read-backs
returned false failures: a consumer's Android suite went from 11 of 20
passing to 20 of 20 red, and every action judged failed had in fact been
performed. That is worse than what it replaced — "did it and said it did
not" blocks everyone, where "did not and said it did" only misleads.

Three separate causes, each reproduced here on a Compose screen before
being touched:

- the read-back never refreshed the node, so it read the value the field
  held when the framework handed the node over — the value from before
  the typing. The tree walk in the same file has carried a comment about
  exactly this since it was written.
- `findFocus(FOCUS_INPUT)` is the wrong instrument on Compose, which
  keeps focus in its own semantics layer. On a freshly composed screen it
  comes back empty about a field the tree reports as focused in the same
  instant, with the IME already up. That is the "first fill on a screen"
  shape.
- `pressBack()` returns whether the key event was injected, not whether
  the keyboard went. It answered false with the IME gone from the window
  list one call later.

### Fixed

- **`/input-text` no longer refuses fills that landed.** The node is
  refreshed before it is read, the read is repeated until the characters
  are there or two seconds pass, and it is the same node that was typed
  into rather than a fresh focus query. A Compose field publishes to its
  accessibility node asynchronously; a consumer measured 17, 15 and 0
  characters from one action, so no single instant can answer the
  question.
- **`/input-text` finds focus a second way when the first has nothing.**
  The framework's input-focus query is still asked first. When it is
  empty, the window tree is walked for a focused node that accepts text —
  which is what the tree route already does, and it costs a traversal
  only where the cheap answer failed.
- **`/hide-keyboard` judges by the window list, not by `pressBack`.**
  The IME's own window is the evidence, and it is the same evidence the
  no-op decision already reads. The wait is because the dismissal is
  animated and outlives the key press by a frame or two.

### Changed

- **Step lines print as each step starts.** They were all written before
  the first one ran — a listing wearing the words "per-step progress".
  A failure then landed after the whole list and read as having happened
  at the last step; a consumer attributed a failure to the wrong verb on
  that evidence and reported it against the wrong route. The last `STEP`
  line in a log is now the step that was in flight. The count moves to
  one `flow: N steps` line, and `STEP N: … → SKIPPED: <reason>` is
  unchanged.
- **`hideKeyboard` answering `ok` now means the keyboard is gone**, not
  that a key event was injected. Flows that treated its `ok:false` as
  informational will see fewer of them.

### Notes

- The fixture app gained a Compose screen, and the release gates drive
  it. That is the root fix rather than a test addition: 6.4.0's gates
  only ever drove an `android.widget.EditText`, whose accessibility
  semantics are the simple case, so a predicate true of that and false
  of Compose could not be seen. A gate cannot find what its subject
  cannot exhibit.
- No migration. Nothing on disk or on the wire changed shape.

## [6.4.0] — 2026-08-19

Three answers that came back `ok` without having been asked anything. A
consumer driving a fresh Android emulator reported the first two; the
third turned up underneath them, and it is the one that had been hiding
the others.

`input text` types into whatever holds focus and cannot report that
nothing did, so the handler dispatched it and answered OK unconditionally
— a fill onto a screen with no focused field typed into nothing and said
success. `hideKeyboard` had no keyboard to hide and pressed back anyway,
which with nothing to close closes the app: `ok:true`, and the fixture
app on the launcher. And `runner up` read `/health`, which says the HTTP
server answers and nothing about whether the instrumentation still sees
the device — so a runner whose accessibility connection had gone stale
was reported as already up.

Underneath all three: the socket read those predicates are built on never
got an answer. It waited for a close the runner does not send, on a
deadline that raced the runner's own five-second one. `automation_sees_an_app`
has been passing everything since the day it was written, because it had
nothing to judge.

### Changed

- **`hideKeyboard` does nothing when no keyboard is up, instead of
  pressing back.** The old behaviour pressed back unconditionally; with a
  keyboard up that closes the keyboard, and with none it closes whatever
  is in front. A flow that dismisses a keyboard defensively — iOS needs
  it, the keyboard covers the button the next step taps — was backing out
  of the app on Android and reporting success. It now reads the IME's own
  window from the same list `/windows` serves, and still hides a keyboard
  that is up.
- **`/input-text` waits for a focused editable field and reads the field
  back afterwards.** It used to answer OK whatever happened. It now
  answers `no_focused_field` when nothing takes focus within two seconds
  — the tap that focuses it lands immediately before, and a cold IME
  needs a moment, which is why the second fill on a screen always worked
  — and `text_did_not_land` when the characters are not in the field
  afterwards. A flow whose fill was going nowhere now fails where it used
  to go green.
- **`runner up --platform android` refuses a runner whose view of the
  device is stale, and `--force` recovers it.** The flag was documented
  in the shared CLI help and never reached the Android path. The refusal
  names what is in front, what the runner sees instead, and the command
  that fixes it.

### Fixed

- **The runner-facing socket read now reads to `Content-Length` on a
  deadline past the runner's own.** It read to EOF, and the runner
  announces `Connection: close` and then leaves the socket open; its
  five-second deadline raced NanoHTTPD's five-second `SOCKET_READ_TIMEOUT`,
  which is how long a connection is held waiting for a second request on
  it. Every predicate built on that read has been answering "cannot tell"
  on every real device.

### Notes

- No migration. Nothing on disk or on the wire changed shape; the CLI
  surface gained no verb and lost none.
- Flows that were passing while typing nothing, or while backing out of
  the app, will now fail. That is the point of the release, and it is
  worth expecting: a green flow built on either defect was green without
  having done the thing it describes.

## [6.3.0] — 2026-08-18

A flow that ran green came back as `left no attempt record` in the middle
of a release, and the gate was right to stop: the record it reads had
been overwritten by another smix on the same machine.

Flow attempts were one machine-global blob, rewritten whole, on a write
that skipped itself when someone else held the store lock. Three ways to
lose a record fall out of that shape, and the gate cannot tell any of
them from a flow that never ran — a write skipped as busy, with no next
attempt to persist it, because `smix run` records once and exits; a
read-modify-write split across two lock intervals, so the second writer
put its own snapshot back over the first; and a 32-entry cap on a shared
list, where a neighbour's traffic could evict a record before its reader
arrived.

### Changed

- **A flow's attempt record moved from the shared blob `one:flow-attempts`
  to a key of its own, `attempt:<flowName>`.** Put, trim and sync now
  happen inside a single `Store::open` — blocking, not best-effort.
  Waiting a few milliseconds behind a neighbour is what makes the record
  exist at all. Two smix processes recording different flows no longer
  overwrite each other, and the cap evicts by age rather than by whoever
  wrote last.
- **Reads merge the old blob, so upgrading keeps the history.** The
  singleton is read, never rewritten — migrating it would mean writing a
  whole blob again, which is the shape being removed. Records written
  before this version have no timestamp and are treated as the oldest.
- **Downgrading is one-way for this data.** A 6.2.x smix reads only the
  old blob, so diagnostic records written by 6.3.0 are not visible to it.
  Nothing else is affected: this is `smix diagnostic dump`'s recent-flows
  section and the retry attribution a release gate reads from it. Older
  records stay readable in both directions.
- **kevy-embedded 5.1 → 5.3.** Both intervening releases state nothing
  changed on the wire and that a 5.1 data directory opens as-is in either
  direction; what moved is the wasm feature set and the Lua dialect, and
  smix uses neither.

### Gates

- Two concurrency gates that fail on the parent commit with their own
  wording rather than a compile error: a neighbour's record surviving
  this process recording its own, and a record written while a neighbour
  holds the store lock.
- The cap gate cannot be red on the parent commit — the old in-process
  cap satisfied it — so its three assertions were each falsified by
  mutation instead: raising the limit, reversing the trim order, and
  reversing the returned order.

`subprocess_ring` keeps its best-effort write. It runs after every
`xcrun simctl` call, losing one diagnostic record there is acceptable,
and blocking that path behind another process is not.

## [6.2.0] — 2026-08-17

Two releases in one. The device under your hand belongs to someone, and
Android was being driven as if it were an iOS simulator with a different
name.

A second person driving smix on the same machine took its emulator out
from under a release a dozen times in one day, and two release gates
picked their device instead of ours — because the ledger that answers
"did smix boot this" existed for simulators and had no Android half.
Separately, a consumer moving an app off raw adb hit five gaps that were
two design faults wearing five hats: the platform was an argument that
defaulted to iOS rather than a property of the device you named, and the
same capability was reachable through a flow and refused from the CLI.

The v6.1 device-ownership work is folded into this release and is not
published on its own.

### Added

- **The boot-ownership ledger covers Android emulators.** "Did smix boot
  this device, and may it stop it" had an answer for simulators and none
  for Android: `Boot` and `Shutdown` were `simctl`-only, so an emulator
  had no smix-owned boot path and nothing recorded who started it. smix
  now boots and shuts an emulator it owns, records that it did, and
  refuses to stop one a person booted by hand. The ledger was already
  platform-neutral; only the wiring was missing.
- **`smix session state`** — a read-only report of which device the
  session is bound to (`bound` / `udid` / `port`). It is the one verb
  that answers without a bound app, reading the session directly rather
  than through an open app.
- **`smix diagnostic dump`** — surfaces what the bound runner has open,
  as a thin pass-through to the runner's diagnostic endpoint.
- **MCP `diagnostic-dump` and `session-state` tools** — the two verbs
  above through the external-agent surface. `session-state` is the only
  tool that answers unbound; neither opens nor closes a session, so they
  do not overlap the write path `launch_app` already owns.
- **`smix run --platform` is optional and inferred from `--device`.**
  When omitted, the platform is read from the named device's registered
  kind — emulator or physical-Android to Android, simulator or
  physical-iOS to iOS. An explicit `--platform` still wins, for a device
  that is not registered.
- **A dedicated device per project, resolved without `--device`.** `smix
  init`'s alias, omitted, is now derived from the project directory's
  name rather than always `"dev"` (two projects no longer register as the
  same alias), and init records that alias as the project's default. The
  record is a pointer, not a fact: a `project-device:` namespace in the
  machine store maps the project's path to an alias string. What the
  alias resolves to (UDID, kind, opt-in) stays in the machine registry —
  the pointer may live with a project, the facts may not (§9 #9), and it
  is machine-scoped, not committed. `smix run` with no `--device` then
  resolves the project's default: explicit `--device` wins, then the
  pointer, then the old iOS-default behaviour for anyone without one. It
  never reaches for whatever is attached, and the platform is still read
  from the resolved device.

### Changed

- **`smix run` reads the platform from the device, not a default of iOS.**
  `--platform` used to default to iOS regardless of `--device`, so an
  Android flow's `launchApp` was dispatched to `simctl` (`Invalid
  device: emulator-5560`) even when the registry knew the device was an
  emulator. Naming an unregistered device with no `--platform` is now an
  error that says to register it or pass `--platform`, rather than
  silently guessing iOS. With the platform inferred, Android `launchApp`
  runs `am start` as it always could and brings the app to the front —
  the earlier `ok: true` with nothing foregrounded was the mis-inferred
  platform, not a missing capability.
- **CLI verbs dial their driver from the device, as flows always did.**
  `fill`, `find`, `tap`, `tap-then-screenshot`, `wait-for`, `scroll`,
  `swipe`, `swipe-between`, `press-key` and `hide-keyboard` went through
  a hardcoded simctl driver regardless of `--device`, so `smix fill` and
  `smix find text:` answered `501` on an Android runner while the same
  device's flow `inputText` worked. Each now selects the driver by the
  device's platform. `tree`, `describe` and system-popup reads stay
  platform-neutral — they already spoke the same wire on both.
- **`tree`'s human-readable outline prints element text.** It read only
  identifier and label, which was enough for iOS (whose serializer sends
  label/value/title and no `text`) and blind on Android (whose `text`
  attribute carries the words, so a button reading "SUBMIT" showed
  nothing). Text now prints when non-empty; iOS output is unchanged.
- **A device is the ledger's answer or the caller's, never the first one
  `adb` lists.** Resolution no longer falls through to whatever emulator
  `adb` happens to enumerate first, and the six smoke scripts stopped
  defaulting to `emulator-5554` and `emu kill`-ing it on teardown,
  whoever it belonged to. One teardown rule now covers both platforms.
- **The `--device` help no longer claims it does not change dispatch.**
  For verbs that behave differently by platform, `--device` now decides
  the driver, and the help says so.
- **The MCP server says what smix drives, not just what it drives.** Its
  introduction opened "smix drives an iOS Simulator" — true of that one
  server (it binds one simulator via `SMIX_UDID`) but read as the whole
  product. It now says so: this server drives one iOS Simulator, and
  smix's CLI drives Android emulators and registered physical devices
  too, with a `--device` and the platform read from it.
- **The adb-guard refusal leads with smix.** Blocking an unpinned adb
  command, it now names smix first — the intended way to drive a device,
  which takes the device up front so a bare command cannot fall through to
  an attached phone — and keeps pinned raw adb as the fallback.

### Fixed

- **Landscape visibility is judged in the right space.** The tree
  serializer intersected a live landscape node's frame against a portrait
  `appFrame` cached at launch, so nodes past the portrait width were
  reported `visible: false` on a landscape-locked app (a counter at
  x=407 went false while siblings inside 402 stayed true). Visibility is
  now computed against the snapshot's own root frame, mirroring the
  Rust `is_visible_enough`, and the stale cached-frame path is removed.
  This is the sense-side companion to 6.0.0's act-side coordinate-space
  fix.
- **A stale lease no longer locks smix out of a device it owns.** Device
  ownership is decided through `smix_lease::assess`, which already
  distinguishes "someone is using it" from "someone used it and left",
  so a lease whose holder has exited no longer refuses a legitimate
  shutdown.
- **`smix sim shutdown` reads the boot row it claimed to.** A comment
  said the boot row recorded who may shut the device down; the code shut
  it down without ever reading the row. The rule now lives in the
  behaviour, not only in the prose.

### Gates

- `v6.1-c1-who-booted-this-emulator-e2e` — on a real device: smix boots,
  the ledger answers `booted by smix`, smix shuts it and the row
  disappears; the same emulator booted by hand is refused, `rc=1`, and
  stays up.
- `v6.1-c5-two-devices-one-is-not-yours-e2e` and
  `no-script-picks-a-device-by-accident` — a second person's device is
  not the one a smix script reaches for.
- `contract-scan` runs on a bare checkout — a gate that reads `.claude/`
  used to error where that tree is absent, which is any clone but the
  development machine.
- `v6.2-c1-platform-from-device-e2e` — a byte-identical flow (only the
  appId differs) runs on iOS and Android with no `--platform` and both
  foreground the app.
- `v6.2-c3-capability-parity-e2e` — `fill` and `find` reach the CLI on
  Android (`rc` 0, not `501`) exactly as through a flow.
- `v6.2-c4-env-interpolation-e2e` — `--env` and `${NAME}` on both sides:
  a supplied variable lands as its value, an undefined one fails with
  `undefined variable` and leaves the field untouched rather than typing
  the literal. Both sides are proved by mutation. (The single-machine
  interpolation path was already sound on `develop`; the symptom a
  consumer reported was against a 5.1.0 build that was never published.
  This gate keeps the path from regressing.)
- `v6.2-c5-tree-text-e2e` — the outline carries `text` on Android and
  does not sprout an empty `text=` where a node has none.
- `generated-artifacts-are-load-bearing` and `napi-dts-fresh` — every
  file that declares itself auto-generated is covered by a freshness
  gate. The napi loader's `index.d.ts` drifted between two ships
  (`swipeAtCoord` reached the Rust binding and not the `.d.ts`) with no
  gate watching it; now regenerate-and-diff catches that shape.
- `v6.2-c6c-landscape-visible-e2e` — three nodes on a landscape stage
  assert `visible: true`; the pre-fix portrait rectangle makes the gate
  red.
- `v6.2-c6d-attach-on-device-e2e` — the attach retry, watched on a
  simulator through a compile-time injection seam (`up_on_with`, not a
  runtime switch on the shipped path): an injected first-attempt timeout
  drives a real `simctl launch` foreground and a real second attach, and
  the runner it brings up then drives the fixture. This closes the
  [5.0.0] "not watched on a device" note.
- `v6.2-c7-two-platform-flow-e2e` — the exit gate: one byte-identical
  flow (only the appId differs, portable selectors only) runs end to end
  on both platforms, with no `--platform`, threading launch-to-front,
  platform inference, CLI `fill`/`find`, `--env` supply, and Android's
  text-bearing outline.
- `project-pointer-holds-no-facts` — the per-project device pointer holds
  only the alias string; a paired scan refuses any device fact (UDID,
  kind, opt-in, runner port, lease) in its writer, so the pointer cannot
  quietly become a second place facts live.

## [6.0.0] — 2026-08-16

A tap that reported success while nothing moved, and the half of an
escape hatch that never reached a surface.

A consumer drove a landscape screen and watched every `smix tap` answer
`landed inside: <the button>` while a pixel-by-pixel diff found the
screen byte-identical. Six rotation mappings, all reasonable, all
useless — because the error is applied *after* the caller's input.

The point is computed against `XCUIApplication.frame`, which is the
app's own landscape space, and the synthesised event was stamped
`.portrait` at six call sites. That stamp decides which space the
coordinates are read in, so `x = 437` of an 874-wide layout arrived read
against a 402-wide screen. The device had not rotated at all: the
framebuffer stays portrait with the app's landscape layout drawn rotated
inside it.

Which repair it took was not the one the analysis pointed at. Stamping
the event with the app's orientation — the obvious move — changes
nothing at all on a device. `XCSynthesizedEventRecord`'s interface
orientation does not participate in mapping coordinates. What lands is
rotating the point into the device's space and leaving the stamp alone.

### Breaking

- **`FailureCode` is `#[non_exhaustive]`, and gained `COORDINATE_SPACE_MISMATCH`.**
  A `match` over every arm no longer
  compiles without a catch-all; add one. `cargo semver-checks` called
  this a major and it is right — an exhaustive public enum cannot grow.

  The attribute is the reason this is the last time. Two codes arrived in
  two releases, and on an exhaustive enum each one costs a major, which
  is the wrong price for smix naming a failure more precisely. From here
  a new code is additive.

  The guard that made a new variant fail to compile moved inside
  `smix-error`, because from outside `non_exhaustive` would have made it
  accept anything — an escape hatch nobody checks the far side of.

- **`DriverError`'s discriminant moved from 9 to 10.**
  Only matters to code that transmutes or persists the numeric value; the wire form is
  the SCREAMING_SNAKE string and is unchanged.

### Fixed

- **Landscape taps reach the app.** Every synthesised touch — tap, tap
  by id, long press, swipe, system-popup buttons, and the RN daemon-proxy
  path — now converts its point from the app's space to the device's
  before delivery.

### Added

- **`swipe` takes coordinates, on every surface.** `smix swipe --from
  50%,80% --to 50%,20%`, `swipe_from` / `swipe_to` through MCP,
  `swipeAtCoord` in the TypeScript SDK and the napi binding. Section 9 #3
  of the charter authorises two coordinate escape hatches on the same
  grounds; `tap`'s reached the surfaces at 5.0.0 and `swipe`'s did not,
  while `docs/ai-guide/verb-parity.md` ticked both platforms for it. A
  reader following the guide wrote a flow that could not be written.
- **`GET /coordinate-space`** on the runner — read-only, synthesises
  nothing. Reports the app frame, the snapshot root frame, the device
  orientation, the orientation stamped on synthesised events, and
  whether a point will be read in the space it was computed for. The
  runner could describe the screen and touch the screen and had no way
  to say whether those were the same space.
- **`COORDINATE_SPACE_MISMATCH`**, a failure code of its own. Distinct
  from `TAP_MISSED` because the two send a reader somewhere different: a
  miss invites another attempt with a better point, and here there is no
  better point.

### Changed

- **`aimed inside`, not `landed inside`.** What the runner computes is
  geometry — every named element whose frame contains the point, as the
  snapshot describes it. It is evidence about the aim and never was
  evidence of arrival. Two guide passages claimed the stronger thing;
  one of them told the reader that an unchanged screen meant the fault
  was in their app.

### Gates

- `a-tap-proves-aim-not-arrival` — no surface may say a successful tap
  proves the touch arrived, and the surfaces must state what it does
  prove. Both halves, because banning phrasings is satisfied by silence.
- `an-authorised-hatch-reaches-every-surface` — every hatch §9 #3
  authorises is present on all four surfaces (CLI, MCP, TypeScript SDK,
  napi), and every coordinate API it does not authorise is absent from
  all of them. The second half is what keeps the gate from reading as a
  licence to add more.

## [5.0.1] — 2026-08-14

Help text, and a gate for the axis it was on.

A reader of 5.0.0 found `smix tap --help` describing `--ocr-locale` with
`--port`'s sentence while `--port` itself was blank. It was not a typo:
`--port`'s doc comment sat above the field *before* it, because a field
had been inserted between a comment and the thing it described, and clap
reads the comment as the new field's. Nineteen more flags were blank the
same way — the reader saw the ones on commands they happened to run.

### Fixed

- **Twenty flags and positional arguments now say what they do.**
  `--port` on twelve commands, `--json` on three, and `--text`,
  `--direction`, `--udid`, `<MODE>` and `<ARGS>`, each of which sat first
  in its variant where the `///` above it belongs to the command.

### Gates

- `every-flag-says-what-it-does` — every `#[arg]` field carries a
  description, `hide = true` exempted by name. It found five of the
  twenty by reading the source rather than the rendered help, which is
  where the last five were invisible.

## [5.0.0] — 2026-08-14

**The major is the arbiter's, not the plan's.** Everything here is
additive, and `cargo semver-checks` still says major: two public structs
gained public fields, so any code constructing them with an exhaustive
struct literal no longer compiles. Nothing was added to justify the
number — the number was read off the gate, and the two structs are named
under **Breaking** below. If you never wrote either literal, this is a
minor release wearing a bigger number.

The content is one consumer report, answered. A team driving a simulator
for a day sent four things they had hit; three were defects and one was a
capability that did not exist. What the first one cost them is the shape
worth reading: `/health` answered 200, `/tree` was dead, and
`smix runner up` read only the first and returned success — so the one
command that could have recovered the runner was the one refusing to.
They worked around it three times by hand. The runner had been saying
"reinstalled out from under the runner" the whole time; nothing asked it.

### Breaking

- **`smix_capsule::runner::UpOptions` has a new public field**,
  `force_recover`. Constructing it with `..Default::default()` is
  unaffected; an exhaustive literal needs the field. This one was
  avoidable — the file's own comment says a new field breaks every
  existing literal, and says it next to the function that exists because
  of exactly that.
- **`smix_mcp::SelectorParams` has three new public fields**: `point`,
  `fallback`, `locales`. Same rule, same fix. These are what let a
  coordinate, a chain and a recognition language be named over MCP at
  all, so there is no version of this release without them.

### Changed

- **`smix runner up` no longer reports success when the session behind
  `/health` is unusable.** It used to print `runner already up` and exit
  0 whenever the port answered. It now asks whether the session works,
  and when it does not it exits non-zero naming both facts and the
  command that recovers it. `--force` runs that recovery for you — the
  same in-place cycle `smix runner cycle` does, and it does **not** reach
  a runner recorded for another device or one the store has no record of.
- **A bring-up that runs out of time now tries once more**, with the app
  foregrounded first and the runner attaching to it rather than launching
  it. At most once, and the second attempt starts a fresh clock, so the
  worst case is twice `SMIX_RUNNER_UP_TIMEOUT_SECS` rather than once. It
  does not happen on a physical device (no `simctl launch` to foreground
  with), without `--bundle`, or when the attempt already was
  `--no-launch` — each says which it is rather than skipping quietly.
- **`smix_use` answers `already driving` only when the session works**,
  for the same reason, and names `smix runner cycle` when it does not.

### Added

- **`smix tap --then-screenshot <OUT>`** and **`smix_tap_then_screenshot`** —
  tap and frame in one call, for UI that does not wait. What this saves is
  measured rather than assumed: the wire is 423 ms (a tap 336, a frame
  from the runner 88) and the UI that prompted it lives 3000, so what it
  removes is the turn between two calls, not a round trip. On a simulator
  it also takes the frame from the process that tapped — 88 ms rather
  than the ~325 ms of going out to device tooling. It reports which route
  took the frame and how many milliseconds after the tap it landed. A tap
  that fails writes nothing.
- `smix_capsule::runner::probe_session` / `probe_session_for` /
  `SessionProbe` — "can this session be driven", as a question that can
  be asked, optionally about a named app.
- `smix_capsule::runner::decide_already_serving` /
  `decide_after_timeout` — the two judgements above as pure functions, so
  every row of them is reachable without a device.
- `smix_sdk::tap_then_capture_with` / `App::tap_then_capture` /
  `CapturedAfterTap`, and `smix_runner_client::HttpRunnerClient::screenshot`.
- `smix_driver::transport_to_failure` is now public — what a transport
  error means to a caller, in one place rather than two that drift.

### Gates

Three, each with a harness of its own and each rule removed once to watch
its case go red:

- `health-is-not-a-session-check` — every `health_ok` call site says
  whether it is making a decision, and the ones that are must also ask
  whether the session works. Silence is not a way to become exempt.
- `probes-name-the-app` — a probe either names the app it is asking
  about or carries a reason it cannot.
- `tap-then-capture-is-one-path` — the combined action has one
  implementation, both surfaces reach it, and the frame comes from the
  runner. None of that is visible to a behaviour test.

`mcp-cli-parity-scan` also asks a stricter question now: a declaration
ending in a flag is checked by the flag being listed in that command's
help, rather than by the command running with `--help` appended — which
a value-taking flag cannot survive.

## [4.2.0] — 2026-08-12

A rule changed on 6 August and four things went on saying the old one.
`llms.txt` — the first file an agent reads — still opened with
"simulator-only ... never a physical device" two majors after physical
devices became a first-class backend. A guard meant to keep an install
off a physical phone let one through. A device left running had nothing
on the machine able to say who turned it on. And the document that says
what is being worked on had a shape the contract could not describe, so
its absence read as carelessness rather than as waiting.

None of it was failing. Every gate was green, and each was right about
what it asked.

### Added

- `smix lease prune` asks whether the device is actually on. Three
  answers rather than two: on keeps the ledger, off clears it, and
  *this machine cannot tell* keeps it. An Android serial and a phone
  are not in `simctl`'s list at all, and unlisted must never collapse
  into off.
- `smix_lease::prune_verdict` / `PruneVerdict`, the judgement above as
  a pure function — fed the device's state rather than going for it.

### Changed

- `smix runner up` boots the simulator itself and records that it did.
  It came up either way, because `xcodebuild test -destination platform=
  iOS Simulator,id=…` boots it as a side effect — but nothing then said
  who turned it on, so `smix lease owner` exited 3 for a device that was
  plainly running. Anything reading that code, including release
  tooling, saw a working simulator as busy.
- `smix down` keeps the ledger when a close fails. It used to delete it
  regardless, which meant a failed shutdown left the device running and
  removed the only record of it. `smix lease reconcile` has always drawn
  this line; the two verbs now answer the same situation the same way.
- The plugin's adb guard judges **one command at a time**. It matched a
  pattern against the whole Bash call and then read a word out of the
  whole Bash call, and the two could land on different commands:

      adb -s emulator-5554 shell getprop sys.boot_completed
      adb -s <a physical serial> install -r app.apk

  was allowed, because the emulator pin on the first answered for the
  second. Five shapes of that were reproduced, including `rm -rf` on a
  phone. The mirror image refused honest work: a `curl -s <url>` beside
  a legitimate device command had its URL read as the device name.

  **A script that relied on one command's `-s` covering the next will
  now be refused.** Pin each command that touches a device. An exported
  `ANDROID_SERIAL` still carries, because in shell it genuinely does; a
  `VAR=value cmd` prefix does not, because it applies to that command
  alone.
- `llms.txt` describes what smix drives today. It said "simulator-only"
  and "never a physical device" from 3.x into 4.1.

### Fixed

- The ways a boot record could be lost or never written. Two of the six
  suspected sites turned out to be sound and are documented as such.
- Booting a device that is already up no longer un-says who turned it on.
  Both boot rows are the same kind of row and the ledger replaces rows by
  kind, so a second `smix sim boot` of a simulator smix had already
  booted rewrote "smix booted this" into "smix found it running". The
  next teardown then found a row not worth keeping a file for, and the
  file went while the device stayed up. `by_us` describes one transition
  — off to on — and a later call that finds the device already up learns
  nothing about it.

## [4.1.0] — 2026-08-12

4.0 made a physical Android device registrable, addressable and
drivable, and left no way to put an app on it. This closes that, and the
loop it was part of: the adb guard refuses a bare install and names smix
as the way through, while smix routed the install to simctl and said to
use adb. A consumer chasing a crash that only exists in a minified
release build on real camera hardware moved all eight copies of the
guard aside to get their build onto a phone.

### Added

- `smix sim install` reaches Android. A simulator still goes through
  `simctl`; an emulator or an Android phone goes through `adb`. A
  physical iPhone is refused rather than attempted — installing on one
  needs `devicectl` and a provisioning profile, which nothing here
  wires up, and §9 #1 ③ asks for that to be said rather than degraded
  into silence.
- `smix sim uninstall` reaches every kind of device smix can address.

### Changed

- Taking an app off a physical device is refused until that device has
  been opted in with `smix sim allow-destructive`. Registering one has
  always printed that this gate exists; until now nothing could reach
  it, because erase and keychain-reset are simctl and Apple. An Android
  phone is the first device it has ever had to refuse.
- The plugin's adb guard no longer judges text that is being written
  rather than run. Its patterns are shell syntax, and a heredoc body fed
  to anything but a shell cannot be the shell command those patterns
  describe — so keeping it could only ever refuse prose, which it did:
  a document mentioning a device command could not be saved, placeholder
  serials included. A body read by `bash` is still judged.
- The same guard allows read-only adb against a physical serial —
  `devices`, `get-state`, `logcat`, `shell getprop`. Its header has
  always said read-only was untouched and its behaviour did not agree,
  so a `getprop` came back as a policy refusal that reads like a typo.
  `shell` alone is not read-only: `shell pm uninstall`, `shell rm` and
  `shell settings put` still stop.

## [4.0.0] — 2026-08-11

Device records moved, and two published Rust signatures moved with them.
If you have an SDK integration written against 3.x, read
[Migrating to smix 4.0](docs/migrating-to-4.md) — it is short, and most
of it is one command you run once.

If you drive smix by hand or from YAML flows, there is nothing to undo:
`smix sim migrate` and `smix lease migrate` copy your existing records
into place, and the old locations keep being read until you do.

### A device belongs to the machine, not to the checkout you are standing in

A simulator is an operating-system object. Its UDID, its runtime version,
whether it is booted, who booted it and which port a runner holds on it do not
change when you `cd`. They were stored in whichever `.smix/` sat above the
working directory, so a machine with four checkouts held four answers about the
same simulators — and a runner could hold a port while the tree asking about it
saw an empty ledger and could neither confirm it an orphan nor stop it.

Device records now resolve under `$XDG_DATA_HOME/smix/devices`, and leases under
`.../leases`, beside the runner tree that already lived there. A checkout's
registry still resolves as a read-only fallback and says so; a checkout's
ledgers are read and never obeyed.

**Added**

- `smix runner list` — every runner on this machine: its port, its device, and
  whether the ledgers know about it. Reads only; always exits 0.
- `smix lease owner <device>` — who booted it. Exit 0 recorded, 3 no record,
  1 the question could not be asked. Not a teardown permission: it answers
  "did smix boot this", which is not "did *you*".
- `smix lease migrate [--from DIR] [--dry-run]` and
  `smix sim migrate [--from DIR] [--dry-run]` — fold a checkout's books into
  this machine's. They add and never remove; a second run does nothing.
- `smix lease prune [--dry-run]` — drop ledgers that no longer describe
  anything. A record that can only be added to stops describing the machine.
- `smix sim unregister <alias>` — forget a name, not a device.
- `smix sim list --registered` — the devices smix has recorded, and whether
  each record is the machine's or one checkout's.

**Changed**

- `smix down` closes a ledger when its holder is gone or is this process, and
  names and leaves the rest. It used to close every ledger it found, which was
  sound while the ledgers were per checkout and is not now that the directory
  holds other people's sessions.
- A live holder that has stopped updating its heartbeat is refused rather than
  reclaimed. The heartbeat is written when the ledger is touched, so a holder
  that takes a device and then serves for hours is silent by design; treating
  that as wedged would let any `lease reconcile`, from any checkout, tear down
  a live session. `StaleReason::HeartbeatExpired` remains as a name and is no
  longer produced.
- A zombie process no longer passes the liveness check. `<defunct>` answers
  `ps -o lstart=` with the time the process it used to be started, so the
  (pid, start time) identity matched and a ledger reported a live runner on a
  device that had none.
- `smix lease reconcile` refuses on a device whose checkout ledger disagrees
  with the machine's, naming both paths and the one command that ends the
  state.
- A device record no longer falls back to `.smix/sims.json` in the working
  directory when the machine location cannot be resolved. There is no good
  place to put such a record silently.

**Breaking**

- `smix_lease::store::lease_dir` is gone. It built `.smix/leases` from a
  workspace root, which is the thing that no longer happens.
- `smix_sdk::leased::Leased::acquire` and `smix_sdk::App::hold_device_lease`
  each take one more argument: the ledger directory. The tree they were given
  used to mean two things — where the ledger is, and where a dead holder's
  build products would be settled — and only the second is still a tree.

**Library**

- `smix_lease::store` takes a `LeaseDir` rather than a workspace root. The
  argument used to be a root the functions appended `.smix/leases` to; changing
  what it meant left every call site compiling and still writing into trees, so
  it is a type the compiler can check. `CheckoutLedgers` is the read-only
  counterpart.
- `smix_lease::store::machine_root()` is the single resolver for where this
  machine keeps smix's data. Six functions worked it out for themselves and
  three reached through `dirs::home_dir()`, which does not consult
  `XDG_DATA_HOME` — so with that variable set, the runner tree and the
  press-timing table landed in different places.

## [3.0.0] — 2026-08-11

A round of consumer feedback found fourteen things. Under them were four
gaps, and two of those change what code you already have does. If you
have flows or an SDK integration written against 2.x, read
[Migrating to smix 3.0](docs/migrating-to-3.md) — it is short, and it is
only about the two.

### Changed

- **`fill` replaces the field it names.** It appended, so returning to a
  form and filling the same field again left both values concatenated.
  In a password field that is invisible — the dots look right — and it
  surfaces as a login rejecting a correct password.

  The rule is now stateable: **you can only replace a field you named.**
  `fill(id:…)` and `inputText: {id, text}` empty the field first; typing
  into whatever holds focus (the scalar `inputText:`, `pasteText`,
  `App::fill(&focused(), …)`) still appends, because there is no named
  field to empty — which is also what maestro's verbs of that shape do,
  so a ported flow still means what it meant.

  The guides have described this verb as replacing since it existed. The
  default flipped rather than gaining a flag, because a flag leaves the
  bug in place for everyone who does not know to set it. On the wire it
  is `clearFirst` on `POST /fill`, default true; a runner too old to
  know the field appends, which is what it did before.

- **`describe` no longer enumerates the software keyboard's keys, and
  `tree` collapses them.** A summary per letter plus `Next keyboard`,
  `Dictate`, shift and delete is around sixty elements that are the same
  sixty on every screen of every app, in output an AI pays for by the
  token. `smix tree --keyboard` includes them; the keyboard element
  itself always appears, because a keyboard covering the thing you
  wanted to tap is the explanation for a failure.

- **`smix capsule` refuses a device it cannot act on, and says which
  command can.** It ran `simctl boot` against an Android emulator and
  sat there until it timed out 120 seconds later, reporting the timeout
  rather than the mistake. Every `smix sim` verb now declares which
  device kinds it supports, in a match the compiler checks: a new verb
  does not build until it says.

- **`Driver::fill` and `HttpRunnerClient::fill` take `clear_first`.**
  `cargo semver-checks` calls this a major break, and it is; `App::fill`
  is unchanged and derives the flag from whether the selector names a
  field.

- **The state log compacts itself.** kevy compacts on a growth rule
  whose baseline is re-read at every open, and smix is a one-shot CLI:
  each process saw a log that had not grown since it started. A working
  install had reached 100,501,372 bytes holding three keys, replayed in
  full at the start of every command. `Store::open` compacts past 16
  MiB — that install became 35,923 bytes. Reported upstream; the replay
  banner itself is kevy's and cannot be silenced from here.

### Added

- **The Android runner ships with the install.** `runner up --platform
  android` used to end at an error naming
  `android-runner/app/build/outputs/apk/…` — a path relative to the
  caller's working directory, which is the project being driven, not
  smix. That directory exists only in a clone of this repository, so
  everyone who had merely installed smix was told there was no APK and
  concluded the product had no Android support. The capability was
  there the whole time, gated behind an artifact nothing shipped.

  What ships is the project, not the artifact — the same bargain the
  iOS side makes with Xcode. The APK is 51 MB; the sources that produce
  it are 94 KB, and a machine that drives an Android device already has
  the SDK that builds them. First run extracts to
  `~/.local/share/smix/android-runner/` and builds; later runs find it
  built. Staleness is decided by a content digest, with the same gate
  the Swift sources have.

- **`POST /clear-text` on the Android runner.** Emptying a field was
  fifty `/press-key DELETE` posts from the host — fifty sequential round
  trips over the adb forward, on every fill once fill began clearing
  first, and still wrong for a field longer than fifty characters. One
  request now: `ACTION_SET_TEXT` on the focused node, exact at any
  length, falling back to bounded deletes in a single shell exec when no
  focused editable node answers. The response names which path ran,
  because one is exact and the other is not.

- **`not-running` as an app-unavailable reason.** A terminated or
  reinstalled app was reported as `crashed-during-init`, which sent the
  reader to look for a crash report that does not exist. The runner also
  falls back to the bundle it was started with when a request names
  none, instead of answering `unknown` about an app it has known since
  `runner up --bundle`.

- **`smix sim list` lists Android devices**, with a `platform` field in
  `--json`. Android entries are not dressed in simctl's clothes: there
  is no runtime identifier on an emulator, and inventing one would make
  the listing agree with a schema by lying about the device.

- **`smix runner down --runner-port`.** `up` has taken it all along and
  `down` read `SMIX_RUNNER_PORT` instead, so a teardown written as the
  obvious mirror of the bring-up failed its argument parse and left the
  runner running.

- **`GET /windows` on the Android runner**, and an `unreadableWindows`
  count on the tree's root. A window whose root cannot be read was
  skipped in silence, and a window missing from the tree looks exactly
  like an app with no accessibility nodes — which is what a consumer
  concluded, after several rounds of driving by pixel. The two are
  different problems and now say which they are.

- **A third-party fixture app for the Android gates**
  (`test-fixtures/android-app`). Every Android e2e drove Settings, a
  system app, so anything specific to an ordinary app's window was
  invisible to all of them.

- **`smix runner up --device`**, which `runner down` has always taken.
  The adb guard and the guides suggested the flag form, and `runner up`
  answered "a similar argument exists: '--supervise'".

- **`smix tree --keyboard`**, and `docs/migrating-to-3.md`.

### Fixed
- **`/find`, `/fill` and `/clear` read `App-Bundle-Id`.** A request says
  which app it means with that header, and these three never looked at
  it — they used whichever app the runner booted with. A flow whose
  `appId` differed from `runner up --bundle` could not find anything,
  and `fill` was worse than a failed lookup: it typed into the wrong app
  and reported success. If you drive more than one app in a flow, this
  is the difference between it working and it silently working on
  something else.
- **The iOS runner starts from a fresh install.** `Package.swift`
  declared a 49 MB binary target the archive deliberately excludes, and
  SwiftPM resolves the whole package graph before building anything —
  so `smix runner up` failed on any machine that had not built smix from
  source before, with `local binary target 'SmixCoreFFI' … does not
  contain a binary artifact`. The tarball now carries the manifest the
  runner builds rather than the whole workspace's.
- **Destructive verbs meet the governance gate, and `exec` is one of
  them.** On a physical device, `sim erase`, `uninstall` and
  `keychain-reset` were refused by the transport layer — "this command
  runs through simctl, and that is a phone" — which reads as "this is
  impossible" and sends you looking for another route. The rule now
  speaks first and says how to lift it. `smix sim exec` runs an
  arbitrary command on the device and was not gated at all.


- **A filled value no longer reaches the transcript.** `smix fill` and
  the MCP tool echoed the text they typed, so a password read from a
  file into a shell variable — deliberately, to keep it out of the
  session record — was printed into that record anyway. Both now report
  the length and nothing else. Not a `--secret` flag: a default that is
  only safe when you remember to ask for safety is not a safe default.

- **`--runner-port` works on Android.** The forward mapped host port to
  the same device port while the runner listens on a compiled-in 28080,
  so any port but the default forwarded into silence and the health
  wait timed out with the runner running perfectly. Every Android caller
  in this repository passed 28080, which is why it never showed.

- **`runner down` closes the forward adb actually has**, read back from
  `adb forward --list` rather than assumed from the port passed in. Run
  from a directory with no workspace state it fell back to the default,
  announced a port closed, and left the real forward pointing at a dead
  runner.

- **The instrumentation APK follows its sources.** It was rebuilt only
  when absent, so a source sync left the artifact behind and the runner
  answered `not_implemented` for a route whose Kotlin sat one directory
  away. It carries a stamp of the digest it was built from; the working
  tree's APK is rebuilt when older than `app/src`; and `runner up` no
  longer reads "something answers /health" as "we are up", since the
  device port is fixed and an instrumentation from an older APK answers
  it perfectly.

- **The unregistered-device refusal states the actual rule.** It said
  "neither simctl nor adb calls it one of theirs" about a phone `adb
  devices` lists by name. The rule is registration, not visibility —
  being plugged in is not registration, which is the whole point.

- **`sim register` says where it wrote** on every device kind, not only
  simulators. **`sim resolve` accepts an adb serial**, which also fixes
  `lease reconcile` on an emulator.

- **The plugin's version warning says which side is behind and what
  follows.** The two directions are not alike: a newer smix costs
  nothing, a newer plugin can name a tool that does not exist. It also
  names the command to update the plugin, and that a restart is needed —
  reloading re-reads the version you have.

- **The skill-parity gate checks the direction it was missing.** It
  verified that skills name real commands; it could not see a skill that
  promised a capability and never taught it, which `drive` did for
  Android across two releases. Which terms count as capabilities is read
  out of the product — subcommands, flag values, tool names — not from a
  list somebody maintains.

## [2.3.0] — 2026-08-07

### Added

- **Physical devices — an iPhone or an Android phone, driven the same way a
  simulator is.** They were refused on principle until now, and the reason was
  never capability: it was that smix had no concept of owning a device, so
  every non-business action was an unowned one-shot call. On a simulator that
  costs a crash-report dialog; on somebody's phone it costs their phone. Three
  constraints hold, and each is a code path rather than a policy. A device must
  be **registered before it can be addressed**, so "whichever one happens to be
  plugged in" is never a target. Destructive actions are **refused per device**
  until allowed once, recorded rather than confirmed per command. And a
  capability a phone does not have is a **loud error, never a silent no-op** —
  of the 25 device operations smix offers on a simulator, six reach a phone
  through `devicectl`, three through the runner, two do not apply, and fourteen
  have no equivalent at all.

- **A ledger of what is open on each device** — `smix lease list|show|reconcile`.
  One record per device naming who holds it and what they opened: runner,
  recording, boot, supervisor, Android runner, port forward. Identity is the
  pair (pid, start time), because a pid alone is reissued to strangers and a
  command line is identical across concurrent runs. Teardown now closes what
  the ledger says was opened rather than what a process listing looks like,
  which used to fail in both directions — killing another project's runner, and
  missing the one whose command line read differently. **A kill that had no
  graceful path gets one at the next startup**, which is what the crash-report
  dialog was always about.

- **`smix record start|stop|status`** — recording lived in one process's memory,
  so "is this device recording, and where is the file" had no one to ask, and a
  killed holder left `simctl` writing an mp4 that would never gain a trailer.
  `status` reports the path, not just the fact: knowing a recording exists
  without knowing which file it is leaves you guessing in a directory listing.

- **A tunnel to a physical iOS device, written here rather than depended on** —
  `smix runner forward`. usbmux is Apple's own daemon, so what a third party
  would supply is the protocol, and that is exactly the part that can be
  written. `smix runner up <phone>` raises the forward before `xcodebuild`,
  because the runner listens on the *device's* loopback and a health probe
  against a pipe that is not there yet reads as "the runner did not start".

- **Screenshots reach every device smix can address.** The CLI dispatched to
  `simctl` unconditionally, so a registered Android device had no screenshot at
  all even though the adb path had been implemented the whole time. Android now
  goes through `adb`, and a physical iPhone through a new `GET /screenshot` on
  the runner — Apple exposes no screen capture for a phone through `simctl` or
  `devicectl`, but `XCUIScreen` runs inside the runner and works on both. A
  failure there answers 503 with a reason rather than 200 with an empty body: a
  zero-byte PNG on disk is a file every later step treats as a picture of the
  screen.

- **`smix sim register --kind`**, and `--kind emulator` now succeeds. It could
  not before: every non-physical kind had to pass a CoreSimulator UDID shape
  check that no adb serial can pass, so the flag had no input that worked. Each
  virtual kind is now checked against the catalogue its own platform keeps —
  simulators against `simctl`, emulators against `adb` — and only a phone is
  taken as given, because nothing here can enumerate the world's phones.

### Changed

- **`smix runner down` no longer ends a runner it has no record of.** It reports
  it and stops, naming the process and `--include-unrecorded` for when it should
  go. `runner up` had refused the same situation for a year — "not killing
  blindly" — while `down`, one keystroke away, did it silently. Both commands
  now give the same answer about the same fact.

- **A raw identifier has to be one the platform claims.** Passing a UDID used to
  skip the registry entirely, which was safe only because `simctl` refuses to
  recognise a phone. That stopped being true the day a `devicectl` path existed:
  a CoreDevice UUID wears the same 8-4-4-4-12 shape a simulator's does. A raw
  identifier now has to be a simulator `simctl` lists or an `emulator-<port>`
  serial; anything else is refused before the command runs, naming the
  registration that lifts it.

- **Apple identifiers are normalised and adb serials are not.** `devicectl`
  rejects the lower-case spelling of a UDID it accepts in upper case, so
  upper-casing rescues a typed-in identifier; `adb` matches serials byte for
  byte, so the same move broke them — a registered `emulator-5554` came back
  from `sim resolve` as `EMULATOR-5554`, which is not a device.

- **The installed runner sources re-extract when their content changes**, not
  when the version string does. Between two releases the version does not move,
  so a rebuilt tarball compared equal to whatever was already on disk and was
  never extracted: the device kept running the old runner while the repository
  showed the new, and what that looks like from outside is a new route
  returning 404.

### Fixed

- **`--supervise` did nothing, for every runner new enough to report a wire
  schema — which is all of them.** The success branch returned from the middle
  of the function and the sidecar block lives at the end, so the flag was
  accepted, nothing was spawned, and nothing said so. Found only because giving
  the supervisor a ledger row required there to be one.

- **`smix down` shut down devices it had not booted.** Its last pass treated
  "registered" as "ours to turn off", so a sweep took away a simulator someone
  else's dev server was using. The ledger's boot row records who booted it, and
  only that entitles a shutdown.

- **A recording could report success while writing nothing.** The `simctl`
  child's output was piped to a reader that went away once the recording had to
  outlive its starter, so it died of SIGPIPE on its next log line, leaving a
  zero-byte file behind a success message.

- **`runner up` then `run` works — the lease is adopted, not a wall.** A
  finished `runner up` leaves a runner serving and its own pid in the
  ledger; admission read "live service under a dead holder" as occupied
  and refused every later command, including the quickstart's own
  pairing, on every device kind. A dead holder whose surviving resources
  are all services now hands the lease over as-is; a live holder still
  refuses, and a live recording is never adopted past.

- **`runner down` stops the port forwarder it recorded.** The forwarder
  is deliberately its own process, and teardown dropped its ledger row
  without signalling it — one outlived a passing test run by five hours,
  still holding the port and still wired to the phone. Teardown now
  reads the ledger before forgetting it, and release keeps rows it
  inherited rather than erasing the ledger's memory of processes that
  are still serving.

- **`smix sim boot` returned before the device could be drawn.** Screenshots
  then failed with "Timeout waiting for screen surfaces", and `recordVideo`
  reported success while producing nothing — the boot is now waited out.

## [2.2.0] — 2026-07-26

### Changed

- **The embedded store moves to kevy 4.** A data directory written by an
  earlier smix opens with no migration step. The AOF's record format is
  new in kevy 4, and it upgrades lazily rather than at open: an existing
  log keeps appending in the old format until something rewrites it, and
  **that rewrite is one-way** — afterwards the directory can no longer be
  opened by a smix built against kevy 3.

  So there is a window in which downgrading is still just installing the
  older smix, and it closes the first time the log compacts. Two things
  worth knowing about that window, both honest limitations rather than
  advice: smix does not expose kevy's rewrite policy, so you cannot hold
  the window open; and smix cannot currently tell you which side of it
  you are on, because the engine does not expose the format to embedders.
  If keeping a downgrade path matters more than the upgrade, copy the
  directory before running 2.2.0. Nothing in the smix API changes,
  and the store holds the same things it always did — the device
  registry, runner handles, and the server's stream sessions. What the
  upgrade buys is per-record checksums on the durable log, and an engine
  whose global state now lives in the instance, which is the shape smix
  already relies on when it runs several runners at once.
- **Store failures carry kevy's own error rather than a flattened
  string.** `StoreError::Open` and `StoreError::Op` hold a
  `kevy_embedded::KevyError`, so a caller can still tell an I/O failure
  from a missing key from an engine refusal. This is a breaking change
  for anyone matching on those fields; nothing in the CLI or the SDKs
  exposes them.

### Fixed

- **`smix-mcp` answers `--version`.** Asked for its version, the MCP
  server treated an empty stdin as a request and printed a JSON-RPC parse
  error on stdout — and the plugin's readiness hook filtered that output
  down to digits, produced "2.032700" out of the `-32700` error code, and
  told every session there was a version mismatch with the plugin. The
  server now answers before anything touches stdio, and the hook matches a
  version shape instead of inventing one from whatever it was handed. When
  a binary is present but says nothing recognisable, it says exactly that.

## [2.1.0] — 2026-07-26

smix installs without a Rust toolchain, tells you what to run next, and
ships as a Claude Code plugin. Two loops are closed and tested apart: what
smix does on its own, and what it does inside a session.

### Added

- **The CLI and MCP server as prebuilt binaries** — `npm install -g @goliapkg/smix-cli`. Getting smix meant `cargo install smix-cli --locked`: a Rust toolchain and a 27-crate compile, asked of someone whose app is Swift or Kotlin. Platform resolution refuses what it has no binary for, naming the platform and the source build that still works, because a binary for the wrong architecture installs cleanly and fails later at exec with a loader error that mentions neither smix nor the mismatch.
- **`smix init`** — registers a simulator under an alias, creating the `.smix` registry that alias-form device refs resolve against. With `--app`, it also boots the device and installs the app, reading the bundle id out of the bundle so the command it prints next is runnable as it stands. It does not choose between devices, and never repoints an alias that already exists: an alias is what every later command resolves through.
- **`smix doctor` says what to run next**, and `--json` gives the same verdict as `{ready, checks[], next}`. Checks stop at the first blocked one — telling someone with no Xcode command-line tools to run `smix init` sends them to a command that cannot succeed. It also checks whether the capture server is running, and suggests the `capsule up` invocation that works on the machine it is speaking about.
- **A Claude Code plugin** — `/plugin marketplace add goliajp/smix`. It carries the MCP server, three skills (driving, turning a session into a flow, reading a failure), the device guards, a readiness hook that speaks when smix is not installed, and a monitor that reports a runner going away instead of leaving it to surface as a missing element. It adds initiative, not capability: every verb a skill teaches can be typed in a terminal, and a test compares each one against what the CLI and server actually offer.
- **The MCP session picks its own device** — `smix_devices`, `smix_use`, `smix_release`. `SMIX_UDID` is now a default rather than a requirement; the device is chosen inside the conversation instead of in the client's config file before it started. `smix_use` opens the driving session as well as starting the runner, since a runner on a port is not yet something the other tools can use.
- **`smix authoring record --app-id`** — a recording names the app it was taken against, so it can be run back without an edit.
- **`smix_ai_tier::ask_with_attachments`** — the AI primitive takes attachments rather than assuming whoever answers can open local files.

### Fixed

- **A tap that opened a screen reported itself a miss.** The hit chain was snapshotted after the touch, so a tap that navigated had the destination under its own coordinate by the time the snapshot returned — the successful taps were exactly the ones it called misses. It is now read immediately before the touch. What that cannot see: a screen moving during the snapshot can leave the target under the point when it is read and gone when the touch lands, which reads as confirmed. That errs towards accepting a working tap rather than failing one.
- **A failure message used two definitions of visible at once.** The assertion judged geometry; the "visible elements" list under it judged Apple's flag. A zero-bounds node satisfied one and not the other, and appeared in the same message as both absent and present.
- **`smix doctor` claimed smix supports "iOS Simulator only"**, which stopped being true when the Android emulator lane landed. The first command a new user runs was telling them their platform was unsupported.
- **`authoring record` asserted on the application element itself** — a step that tests nothing and cannot pass, so every recording carried one that failed.
- **`smix runner down` stops the runner on the port it was given, and no others.** After dealing with its own recorded handle it swept anything matching `xcodebuild.*SmixRunner`, which is every runner on the machine — so a teardown pinned to one port stopped a resident runner on another, belonging to a different workspace. `--parallel` and `--nodes` both put several runners up at once, so the sweep contradicted capabilities smix already shipped. The port is now resolved to the session that holds it: the process listening on it is the runner app inside the simulator, whose executable path names the device, and the `xcodebuild` session driving that device is the one that gets the SIGINT. A wedged session that answers on no port is no longer swept — it has to be stopped by hand, which is the price of not being able to tell whose it was.

## [2.0.0] — 2026-07-25

The first major release: the v1 accretions are consolidated into one deliberate surface. Breaking; `smix migrate` rewrites v1 flow yaml, and the runner keeps answering wire schema 1 so v1.x clients still negotiate.

### Breaking

- **Sessions are mandatory** — the iOS driving surface always operates inside a runner session; the loose per-request app binding is gone.
- **Wire schema negotiation** — the runner reports the schema versions it speaks on `/health` (`[1, 2]`); clients negotiate the newest shared version instead of assuming one.
- **`SMIX_*` escape-hatch env vars removed** — behaviour that was env-toggled is now the default or a config field; unknown `SMIX_*` vars warn by name.
- **Selector model merged** — the `Modifier`/`Modifiers` split and the dual `open_url` forms collapse into the single selector model the resolver actually implements.
- **`smix-recorder-ir` renamed to `smix-authoring-ir`** — the crate name now matches what it holds.
- **smix-server needs no database and no valkey.** Stream sessions and the capturing set both live in an embedded store. `DATABASE_URL` and `REDIS_URL` are no longer read; setting either prints a line saying so, rather than being ignored, so nobody keeps a postgres or a valkey running for a server that stopped connecting to them. `SMIX_SERVER_STORE_ROOT` says where the store lives (default `.smix/server`).
- **Unknown keys in a selector mapping are refused.** They used to be read past in silence, which is how every spatial modifier could be dropped without a single flow failing. A flow carrying a key smix does not implement — `enabled:` was one, documented and never wired — now fails to parse, naming the key and listing the legal set.
- **YAML verb table frozen at v2** — verb renames land through `smix migrate`; identity rows that shadowed maestro aliases (`doubleTapOn`, `longPressOn`) are gone, so canonical maestro spellings survive the codemod.
- **The Android launch activity is resolved, not assumed** — every Android launch used to run `am start -n <pkg>/.MainActivity`, which is what an app generated from a template is called and what almost nothing else is; an AOSP app, or one whose entry point had been renamed, did not start and the response said nothing about why. The package manager is asked instead — `cmd package resolve-activity` on the host side, `getLaunchIntentForPackage` in the runner — and `.MainActivity` remains only as the answer when both come back empty, so no device that worked before is worse off. `apps.yaml`'s `activity:` key is honoured as an override; it had been parsed, defaulted and dropped since it was added. **Callers who implement `DeviceControl` themselves must change a signature**: `launch_with_args` takes an `activity: Option<&str>`, and `LaunchAppOptions` and `Flow` each carry a `launch_activity` field, so struct literals need the extra member. Flow yaml is unaffected.
- **Animations are quietened by default** — a run now asks the device to stop animating before it foregrounds the app, and reads the setting back to check it took. How far it gets differs by platform and the wording does not paper over it: Android's `window`, `transition` and `animator` scales go to zero, which is off; **iOS is not quietened at all**, because nothing on the host can do it: `simctl ui` offers appearance, increase_contrast and content_size and no motion option; `simctl spawn … defaults write` answers `Could not write domain` for every domain tried; and XCUITest runs in a separate process, so `UIView.setAnimationsEnabled(false)` cannot reach the app. This shipped claiming Reduce Motion and was corrected the first time it met a device. `--animations` runs with the device's own settings instead. **If you record `assertScreenshot` baselines, they were captured with animations running and may no longer match**; pass `--animations` on those flows or re-record. `waitForAnimationToEnd` still earns its place on iOS, where Reduce Motion is not the same as zero duration; on Android it now has little left to wait for.
- **A tap that lands outside its target says so** — `tapOn` resolves a selector to an element, takes its centre and synthesises a touch there. It used to report success once the touch was synthesised, which meant "a touch happened at that coordinate" and was read as "the element was tapped"; a consumer watched it succeed ten times against a button whose counter never moved. The runner now reports every named element containing the tapped point, and a tap that landed in none of them fails with `TAP_MISSED`, naming what was aimed at and what was there. The usual cause is the screen moving between the tree fetch and the tap — wait for it to settle first. `SMIX_TAP_HIT_MISMATCH=warn` downgrades it for a whole run while a suite migrates. **This does not detect an element covered by something else**: a scrim contains the tapped point exactly as the button does, and the a11y snapshot carries no z-order. `FailureCode` gains `TAP_MISSED` across all four SDKs.
- **A long press now holds for the duration it was given** — `longPressOn: { duration: N }` synthesises the touch instead of calling `XCUIElement.press(forDuration:)`, which was measured taking a constant ~2.6s on iOS 26.5 for every hold from 500ms to 6000ms. Flows that relied on the old fixed hold need their real duration written down.

### Added
- **Single-shot verbs can name a device** — `smix tap`, `find`, `wait-for`, `fill`, `press-key`, `scroll`, `hide-keyboard`, `tree`, `describe`, `system-popups`, `system-popup-action`, `run-script` and the four `smix authoring` actions take `--device`. They read `--port` and `SMIX_RUNNER_PORT` and stopped there, so the `.smix` registry's per-sim `runnerPort` was unreachable from them: in a workspace with a sim registered on 22088, `smix run` dialled 22088 and `smix tap` dialled 22087. `--device` here finds the port and nothing else — a runner is a process on a port, so the port already says which device the call reaches.
- **Relational operators in assertTrue expressions** — `<`, `<=`, `>` and `>=`. The expression grammar's tightest comparison was `==`, so the documented `assertTrue: ${output.userCount > 0}` failed to lex. Two numbers compare by value and two strings in order; comparing a string to a number is refused rather than converted, because an assertion that answers on a reading you did not intend is worse than one that stops. Note that `output.*` always holds strings — `extractWithAI` and `runFlow: {as: name}` are its only writers.
- **An explicit regex form for text selectors** — `text: { regex: "^Help$" }`. `Pattern` has serialized to `{regex, flags}` all along, but the yaml side read `text:` as a string, so a mapping fell through and only a `|` in a plain string ever produced a pattern. `^Help$` was matched as eight literal characters and found nothing. Metacharacter detection was deliberately not widened: `Delete?` and `3.5` are ordinary labels.

- `smix sim register <alias> --udid <UDID>` — creates and populates the `.smix/sims.json` device registry (previously there was no bootstrap for alias-form device refs).
- `smix system-popup-action <popup-id> <button-id>` — CLI verb for the runner's `/system-popup-action` route.
- Bare `- eraseText` parses with maestro's default of 50 characters.
- `POST /tap` returns the matched element's label, frame, app frame, and per-stage timings at the top level (`TapResult`); the previous nested emission deserialized to empty and the resolve mode's frame never actually arrived.
- FFI driving surface (`SmixDriver` / `SmixSession` / `CancelToken`) — the Swift and Kotlin SDKs drive through one Rust wire client instead of three per-language HTTP clients.
- `llms.txt` / `llms-full.txt`, generated from the verb table and guides, gated for freshness at ship.
- `inputText: { id, text }` types into a named field, not just the focused one — the SDK's targeted fill existed all along; only the yaml wiring was missing.
- `openLink: { link: <url> }` mapping form. maestro's `browser:` / `autoVerify:` options are refused loudly rather than accepted and ignored.
- `webViewEval` is accepted alongside `webviewEval` / `webview_eval`.
- Bare `- killApp` and bare `- clearState` act on the current app, resolved from the last launched bundle.
- `App::connect_to_runner_lazy` — build a client without a startup health probe, for hosts whose lifecycle starts before the runner's.
- `smix runner up --platform android` / `smix runner down --platform android` bring the Kotlin runner up and down: install the instrumentation APK, forward the port, start the server, wait for `/health`. Every adb call names its device with `-s`.

#### Folded capabilities (v2.8–v2.12)

The v2.1-additive and Beyond-v2 roadmap work was folded into 2.0.0 rather than shipped incrementally. The user-visible surface it adds:

- **TypeScript SDK drives real simulators** (v2.9) — the RN/Node SDK reached devices through a napi-rs addon (`@goliapkg/smix-node`, the napi peer of the Swift/Kotlin UniFFI surface). `Smix.launchApp` and the `App` driving methods (`tap`, `fill`, `swipe`, `snapshotTree`, `systemPopups`, …) drive a live sim instead of throwing; selectors resolve host-side through the same stone resolver the other SDKs use. (The napi binary's npm publishing is gated on the ship authorization — see Known limitations.)
- **Cross-platform recorder** (v2.10) — a recording captured on iOS (`RecordingApp`), Android (`UiAutomation` accessibility events) or the web (Playwright DOM injection) emits the same `smix-authoring-ir::IRAction` stream, and `smix authoring generate` turns it into a byte-identical maestro or rust flow across all three legs. `smix authoring tap-record --platform android` records a live Android session and generates a flow from it.
- **LLM-in-the-loop authoring** (v2.11) — `smix authoring propose <flow> --bundle <dir> -o <out>` reads a failed run's on-disk bundle, asks the local `claude` CLI to propose structured edits over the real `Step`/`Selector` vocabulary, applies them, and writes an amended flow. Single-provider, local CLI only; fenced (deletable, opt-in, non-deterministic) exactly like the AI-assertion tier.
- **Distributed run federation** (v2.12) — `smix run <flows…> --nodes <roster.yaml>` shards flows across the devices of N machines, runs each shard remotely over ssh, and merges the per-node reports into one JSON document with a worst-of-nodes exit code (an ssh transport failure surfaces as `255`). Nodes run simulators/emulators only — the simulator-only invariant holds across machines.
- **Faster and wider** (v2.8) — in-process runner soft-cycle (~4× faster reset than relaunch), resident IOSurface screenshot capture (~45× faster diff-loop frame grab), `smix run --parallel N` across multiple sims on one machine, an Android parity pass (`ScreenshotPacer` / `AppAliveCache`), a baseline-relative `smix bench` regression layer, and a real-sim stress corpus.

### Fixed
- **The tap routes resolve id and label** — `/tap`, `/double-tap` and `/long-press` decoded only `selector.text`, so `dispatch: daemonProxy` — the escape hatch for React Native views whose `Pressable` swallows the ordinary gesture path — could only ever address an element by its visible label, which is exactly what an RN `testID` is not. The actions guide has documented that pairing with an `id` since it was written and it had never worked. `text`'s meaning is byte-for-byte what it was; `id` and `label` are new and exact.
- **smix authoring suggest searches every readable string** — a bare-string search looked at `text`, `value` and `title` only, and a real iOS tree carries labels: a captured Settings tree has 33 non-empty labels and no text or title at all, so the example printed in the command's own help returned nothing on the platform smix exists for.
- **Underscored arrow key names** — `pressKey: ARROW_UP` and its three siblings. The guides write key names in SCREAMING_SNAKE, which worked for `VOLUME_UP` and not for the arrows, so a reader following the convention landed on "unknown key".

- **Spatial selector modifiers reach the resolver.** `near`, `below`, `above`, `leftOf`, `rightOf`, `inside`, `ancestor`, `nth`, `first` and `last` are implemented in `Modifiers`, honoured by the resolver and documented in the selector guide, and the yaml parser read none of them — every arm built `Modifiers::default()`, and `tapOn` honoured one, spelled `index:`. A dropped modifier cannot fail, it only widens the match, so a flow written to disambiguate silently resolved against everything. `nth:` (the documented spelling) and `index:` both work. Verified on a real simulator: an assertion anchored the true way passes and the same assertion anchored the wrong way fails, which is the only evidence that separates "the modifier arrives" from "the modifier is used".
- **CLI state moved out of hand-written JSON.** The device registry, runner handles, capsule records and the three diagnostic buffers were six JSON files written with unchecked `std::fs::write`; they are one embedded store under `.smix/kv/` now. An existing `.smix/sims.json` (or runner/capsule state) is imported on first use and **left on disk** — downgrading to a pre-2.1 smix still finds it. `smix diagnostic store` prints the whole store as JSON, so state stays as readable as `cat` made it.
- **Concurrent `smix sim register` no longer loses an alias.** Registering read the whole registry, inserted a row and wrote the whole file back; two processes doing that at once kept only the later write. Each record is written on its own now, behind an exclusive lock on the store.
- **The iOS and Android runners stop overwriting each other.** Both wrote `.smix/runner/state.json` through their own copy of the path helper, so bringing one up replaced the other's record and `smix runner down` tore down whichever had won. `down` with no arguments still means iOS and `--device` still means Android.
- **Damaged state is reported instead of read as absent.** Every load used `.ok()?` or `let Ok(x) = .. else { return }`, so a corrupt file was indistinguishable from no file — and the next write erased the evidence. Corruption now names the key it could not read.
- **Diagnostic writes that fail say so.** The subprocess ring, the resetAppData counters and the flow attempts discarded their write results. They still never fail the `xcrun simctl` call they accompany — that was always the right call — but a failure is now visible instead of silent.
- **A flow's own `appId:` opens its session.** `run_flow` opened the session and foregrounded before parsing the yaml, using only the CLI bundle — which defaulted to the placeholder `com.example.app`. The README's own quickstart form, `smix run flow.yaml --device X` with no `--bundle-id`, could not drive a real app at all.
- **Running a flow against an app that is not installed** reports `APP_NOT_RUNNING` naming the bundle and what to type next, instead of a raw `xcrun simctl get_app_container … NSPOSIXErrorDomain code=2`.
- **The MCP server survives a runner that is not up yet.** MCP clients launch their server at client startup, almost always before `smix runner up`; it died there, leaving a dead server for the whole session. It now connects on first use and reports the runner state then.
- **A runner that was never reached is reported unreachable, not dead.** `last_seen_ms = 0` produced "runner died mid-session", sending first-time users after a crash that never happened.
- **`back` waits for the navigation bar to change** instead of sleeping a fixed 500 ms and reporting success unconditionally; an end-to-end flow measured 1811 ms → 1433 ms.
- **The Android runner reads the whole request body.** Unread POST bytes prefixed the next request line on a keep-alive connection, producing `HTTP verb {}GET unhandled`.

### Known limitations

- **The napi backend of the TypeScript SDK is not yet on npm.** The TS driving code is wired, but `@goliapkg/smix-node` (and its per-triple prebuilt addons) is not published, so a plain `npm install @goliapkg/smix` cannot drive a device until that publishing step lands. Swift, Kotlin and the CLI are unaffected.
- **Recording captures user interaction on Android and the web, but not on iOS.** The iOS leg records the SDK's own driving calls (`RecordingApp`), not passive user taps — an accessibility notification carries no action intent, so reconstructing tap/fill/clear from it is a separate research problem.
- **Web recordings generate native flows; they do not replay in the browser.** The web leg is a capture source (Playwright bridge), not an execution target.
- **`App.screenshot()` / `App.openUrl()` / `App.launchFresh()` are not exposed as SDK methods** on any SDK (they need runner-wire or host-side routes that are a separate checkpoint). Screenshots are available as the `takeScreenshot` flow verb; `smix sim openurl` covers URL opening from the CLI.

## [1.0.27] — 2026-07-13

Two issues that flows had been working around at the yaml level move to the runner level where they belong: per-key user-defaults deletion (deep-link replay-state neutralization) and live on-screen visibility confirmation.

### `clearUserDefaults` verb: per-key NSUserDefaults deletion

expo-dev-launcher persists the most recent custom-scheme deep link and re-delivers it after EVERY JS bundle load — always AFTER any URL the flow sends post-relaunch, so flow-side neutralizer URLs and post-relaunch closes all lose the ordering race. App-side replay gates misclassified legit deep links and failed whole batches.

New verb — surgical, host-side, ordering-race-free (runs between terminate and relaunch, no app cooperation needed):

```yaml
- stopApp
- clearUserDefaults:
    keys:
      - "expo.devlauncher.pendingDeepLink"   # whatever key your investigation names
    bundleId: com.acme.app                   # optional; default = flow appId
- launchApp
```

- iOS: `simctl spawn <udid> defaults delete <bundle> <key>` — goes through the sim's cfprefsd so the deletion is coherent with the app's next launch (host-side plist editing would race the cache).
- Contract: "ensure keys absent" — already-absent keys (or a missing domain) are success, not errors.
- Android: explicit unsupported error (SharedPreferences has no host-side per-key path; `clearAppData` remains the whole-store option).
- The generic decision split per the three-layer model: smix owns the deletion capability; WHICH keys encode replay state is app knowledge that stays with the test author.

### Tree-tier visibility agrees with tapOn (live on-screen confirmation)

Under iOS 26.5 + RN 0.86 Fabric, XCUITest SNAPSHOTS report below-the-fold elements with drifted in-viewport frames and `visible=true`. The resolver's frame∩viewport filter passes them, so `extendedWaitUntil` false-greened, `scrollUntilVisible` returned without scrolling, and `tapOn` then honestly failed on the same selector — three verbs, two answers.

Fix — live on-screen confirmation at the driver layer:

- When a tree probe matches a node, the driver issues ONE live `/find` with the new `requireOnScreen: true` flag, using the matched node's `identifier` (else `label`) as the probe handle. The live XCUI query re-resolves current layout — live frames don't drift.
- Confirmed ⇒ hit (one extra live query per successful wait, ~ms). Refuted ⇒ treated as not-yet-visible: `wait_for` keeps polling, `scrollUntilVisible` keeps swiping (exactly the state another swipe fixes), `find`/`runFlow.when`/`wait_for_not_visible` return false.
- On `wait_for` timeout after refuted confirms, the failure hint states it explicitly: "the a11y tree matched this selector but the LIVE on-screen check refuted it every time … use scrollUntilVisible … or an ocrText tier."
- **Frame∩viewport, deliberately NOT `isHittable`** — hittability is false for elements under floating overlays (QA bubbles), which are genuinely visible and assertable; hittability-strict checks would false-negative overlay-tolerant assertions.
- Matched nodes with neither identifier nor label can't be live-confirmed → tree verdict stands (pre-v1.0.27 semantics; OCR tiers remain the play for handle-less degraded trees). Live-probe transport errors also let the tree verdict stand — a flaky probe must not turn a real hit into a miss.
- The live `/find` fast path (simple literal Text selectors) now also requires on-screen, so `find`-based paths agree without a second round-trip.
- Android untouched: its tree is live per-node-refreshed (`AccessibilityNodeInfo.refresh()`), so snapshot drift doesn't arise.

### Regex Text selectors no longer burn the live-route budget

Found while wiring the live-confirmation work: `can_use_find_route` admitted regex-pattern Text selectors, but `Pattern::Regex` serializes as an object the runner's `/find` decode rejects with 400 — a regex Text selector dispatched there would burn the full transport-retry budget (~8 s) and fail with a DriverError instead of evaluating. Regex patterns now host-resolve like other complex shapes.

### Supervisor health-unreachable auto-cycle

A dogfood batch observed a runner death with no `** TEST INTERRUPTED **` banner (warm derived-data reuse after a downgrade sync) — the log-marker supervisor sat through it. `smix runner supervise` now also probes `GET /health` every ~10 s; 3 consecutive failures (~30 s unreachable) triggers a cycle through the same cooldown + storm accounting as the log markers, emitting `{"event":"RunnerCycled","reasonMatched":"health-unreachable x3", …}`.

### Wire compatibility

- `POST /find` gains optional `requireOnScreen` (absent = pre-v1.0.27 exists-only). `FindHandler` typealias gains the param (runner + CLI ship together via the version-mismatch gate).
- `Step::ClearUserDefaults` new enum variant; `clearUserDefaults` new verb key — additive.
- `DeviceControl::user_defaults_delete` new trait method with an explicit-error default impl (Android inherits the honest unsupported error).
- CLI flags unchanged.

### Ship gate

- 81 parser (+4 clearUserDefaults) + 90 runtime_mock (+2 dispatch) tests green; 360 swift-bridge tests green (now a NON-BYPASSABLE ship.sh gate — a stale test asserting the pre-v1.0.11 aliveCache contract had sat failing unnoticed for 15+ releases because this suite was never in the gate; corrected).
- Full workspace + Swift build green.
- Real-sim smoke: wait/scroll/tap agreement + clearUserDefaults key deletion on Preferences.

## [1.0.26] — 2026-07-13

**Systematic polish sweep** — audit of the v1.0.14 → v1.0.25 rapid-patch arc for consumer-specific design leakage, iOS/Android parity drift, and documented-but-unimplemented yaml shapes. No single-consumer feedback drove this cycle; the charter was "sweep everything before moving on."

### De-consumer-ization (generic semantics replace hardcoded values)

- **Interactive-probe default ignore list** (Swift runner): the bundled default contained a specific consumer's bundle id. Replaced with the generic semantic — the target app's OWN bundle id is now always merged into the ignore set dynamically at probe time (the application root node carries `identifier == bundleId` on every app; counting it toward `minIdentifierCount` was a semantic bug, not a per-consumer config concern). Default ignore is now `["SplashScreenLogo"]` (generic Expo splash artifact) only. Consumers with explicit `.smix/config.yaml interactiveProbe.ignore` are unaffected; consumers relying on defaults get strictly more correct behavior.
- **`.ips` snapshot heuristic** (`smix run` crash attribution): the no-bundle-id fallback matched a hardcoded consumer app name. Now matches ALL `.ips` files when the bundle is unknown — the before/after diff around the flow run already time-bounds relevance; name-filtering was both consumer-specific and lossy.

### iOS/Android parity

- **Android runner `/tree` snapshot-freshness headers** — `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` now emitted by the Kotlin runner (v1.0.23 shipped them iOS-only).
- **Android runner version string** — `SmixRunner.VERSION` had frozen at an old build id (`v6.0-c3b`) for multiple releases while the workspace advanced; `/health` lied about the running version. Now tracks the workspace version, and `scripts/release/ship.sh` gates on BOTH the Kotlin `VERSION` and the gradle `mavenCentralVersion` matching the ship version — the drift class is mechanically closed.
- **`elementTypeRaw` documented as iOS-only** (`wire-format.md`): Android has no `XCUIElement.ElementType` numeric; payloads omit the field and deserialize to the default `1`. Android's equivalent triage signal is the full a11y class name in `rawType` plus the runner-emitted `role`.

### Documented-but-unimplemented yaml shapes (docs promised, parser rejected)

- **`tapOn: { dispatch: xcui | daemonProxy }`** — docs described a `mode: pathA|pathB` tap override that never parsed. Implemented as `dispatch:` with the real mechanism names: `xcui` = XCUIElement-anchored dispatch (required for SwiftUI `.sheet`/`.alert`/`.confirmationDialog`/`.fullScreenCover` dismiss bindings; requires `id:`; cross-platform via the runner's id-anchored tap), `daemonProxy` = XCTRunnerDaemonSession synthesize (stubborn RN Pressables; iOS-only). This is the GENERIC surface for what the `v2-*` fixture-namespace auto-routing heuristics did for smix's own selftest — those heuristics remain (scoped to smix's own id namespace, zero third-party blast radius) but are frozen; new yaml uses `dispatch:`.
- **`waitForAnimationToEnd: { timeout: N }`** — maestro-canonical map form now parses (docs showed it; parser only accepted bare/integer forms).
- **`anchorRelative:`** — accepted as alias of `anchored:` in `tapOn` and fallback-chain elements (3 docs pages promised the alias since v5.20).
- **`runFlowConditional:`** — was documented as a verb name in 2 places but never parsed (it's the internal enum variant name). Docs corrected to the canonical `runFlow: { when: …, file|commands: … }` shape.

### Docs alignment sweep

- `02-yaml-reference.md` — `waitForAnimationToEnd` described as "blocks until UI quiescent" (wrong since ever; it's a fixed sleep). Rewritten. Added `runFlow.when.notVisible`, OCR-in-verbs section, `SMIX_AUTO_OCR_FALLBACK` + `SMIX_TAP_OCR_POLL_MS` env docs.
- `04-actions.md` — OCR fallback tap + poll semantics, OCR-aware scrollUntilVisible, real `dispatch:` surface replacing the fictional pathA/pathB.
- `05-cli.md` — `--dry-run`/`--check`, `--retry`, `--debug-output` flags + env var table.
- `06-fixtures.md` — the `- fixture:` verb host-app contract (the `qa-bubble-toggle` toggle id, chip testIDs, completion-signal registry shape) was only documented in non-public notes; now in the public guide with the registry JSON verified against `smix-fixture`'s actual wire shape.
- `verb-parity.md` — version-at-freeze refreshed, `extendedWaitUntil` poll cadence corrected (250 ms, not 100 ms), OCR-in-fallback + snapshot-header rows updated.
- `wire-format.md` — `elementTypeRaw`, `role`, snapshot-freshness headers documented.
- `08-cookbook.md` — conditional-flow recipe corrected to the real verb shape + `notVisible` idempotency example.

### Test-infra fix

- v1.0.23's `SMIX_AUTO_OCR_FALLBACK` parser tests mutated process env under a Mutex — which serialized the env-touching tests against each other but still raced every OTHER parallel test parsing a bare-string selector (observed as flaky failures on two fixture tests). Replaced with a thread-local override seam (`set_auto_ocr_fallback_override`, `#[doc(hidden)]`); tests no longer touch process env at all. 3× consecutive full-bucket runs green.

### Ship gate

- 77 parser tests (+5 new: dispatch ×3 shapes, anchorRelative alias, waitForAnimationToEnd map form ×2) + 88 runtime_mock tests (+3 new dispatch-routing tests) green.
- Full workspace green; Swift build + Android Kotlin compile green.
- Real-sim smoke on Preferences (iOS) for the probe-ignore change.

## [1.0.25] — 2026-07-13

Two follow-up fixes to the v1.0.24 D1/D2 OCR-visibility work, from validating a conditional-gate ceremony (`runFlow when.notVisible: qa-bubble file: qa-gate-passcode.yaml`) on a real batch:

### `SMIX_AUTO_OCR_FALLBACK` splits regex-OR strings per alternative on OCR tier

Pre-v1.0.25, bare-string `visible: 'A|B'` under the v1.0.23 auto-lift became:
```
fallback: [text: /A|B/i, ocrText: 'A|B']
```
Text tier's regex worked; OCR tier's literal `"A|B"` was never on screen — Apple Vision doesn't interpret pipes. Observed in the field: a launch flow used `visible: 'Log in to MyApp|Device'` (legacy regex OR), the auto-lift silently misfired OCR, and the launch chain timed out at 45 s.

Fix: split on top-level `|` and emit one `OcrText` tier per alternative AFTER the single regex text tier:
```
'A|B'       → fallback: [Text('/A|B/i'), OcrText('A'), OcrText('B')]
'A|B|C'     → fallback: [Text('/A|B|C/i'), OcrText('A'), OcrText('B'), OcrText('C')]
'Sign In'   → fallback: [Text('Sign In'), OcrText('Sign In')]      # unchanged, no pipe
```

Tree tier still covers "either A or B" in one probe (cheap); OCR now has real strings per alternative. Character classes (`[A|B]`) and escaped pipes (`\|`) are respected — split only on top-level pipes. Empty alternatives (`|A|` → just `A`) filtered.

4 new parser tests locked (`parse_visible_bare_string_regex_or_splits_ocr_per_alternative`, `..._no_pipe_unchanged`, `..._three_alternatives`, `..._empty_alternatives_filtered`).

### Skipped diagnostic emitted to stderr per step

v1.0.24 improved the `RunStepReport::Skipped { reason }` string to include the selector + evaluation outcome, but the reason only surfaced in `--debug-output/step-N.json`. Under `stdio: inherit` consumers (a harness invoking `spawnSync(SMIX_BIN, ..., { stdio: 'inherit' })`) the diagnostic was invisible.

Fix: emit `STEP N: <verb-summary> → SKIPPED: <reason>` to stderr after each Skipped step. Non-Skipped outcomes stay quiet — no noise for the happy path.

For a batch of flows each short-circuiting on `runFlow when.notVisible: qa-bubble`, consumers now see:
```
STEP 3: runFlow qa-gate-passcode.yaml (conditional) → SKIPPED: runFlow when.notVisible visible=true ({ id="qa-bubble" }); skipped subflow qa-gate-passcode.yaml
```

### Wire compatibility

- The auto-lift shape change only affects yaml parsed under `SMIX_AUTO_OCR_FALLBACK=1`. Pre-v1.0.25 (and OCR-off) parses unchanged.
- Stderr lines are emitted only when a Skipped outcome occurs. Silent for Ok / ExpandedSubflow / errored outcomes.
- CLI / Rust wire types / other subsystems byte-identical to v1.0.24.

### Ship gate

- 70 total parser tests green (+4 new tests, Mutex-serialized env-touching helper reused).
- 119 workspace test-result-ok buckets green (unchanged bucket count).
- CLI smoke on `visible: 'Log in|Device'` under `SMIX_AUTO_OCR_FALLBACK=1` — dry-run parses to 3-tier fallback as spec.

## [1.0.24] — 2026-07-12

**`runFlow.when.visible` + `inputText` silently no-op — fixed.** Root cause: the tree-only visibility check in `runFlow.when.visible` silently drops `Selector::OcrText` at the tree resolver, so a `fallback: [text, ocrText]` gate under iOS 26.5 + RN 0.86 Fabric a11y drop returns false and the whole conditional body is skipped without any signal. `inputText` never fires because the conditional never enters. 3 fixes land:

### `runFlow.when.visible` fires OCR when selector contains OcrText

Runtime dispatch for `Step::RunFlowInline` and `Step::RunFlowConditional` now goes through `check_selector_visible` (introduced in v1.0.23 as the shared "probe once via tree + OCR" primitive) instead of tree-only `App::find`. When the gate selector contains any `OcrText` sub-selector, OCR fires; when it doesn't, tree resolver runs unchanged. Fast path preserved for OCR-free gates.

Concrete scenario from the field:
```yaml
- runFlow:
    when:
      visible:
        fallback:
          - text: 'For internal testers only'
          - ocrText: 'For internal testers only'
    commands:
      - tapOn: { id: 'qa-passcode' }
      - inputText: '0429'
```
Pre-v1.0.24: `text:` misses under Fabric drop → OCR silently skipped → gate returns false → whole body skipped → `inputText` never fires → passcode field stays empty → 45 s wait inside the (never-entered) conditional appears as ~0 s STEP wall.

Post-v1.0.24: OCR fires on the `ocrText` layer → gate returns true → body runs → `tapOn` + `inputText` deliver as expected.

### `runFlow.when.notVisible` inverse gate

Idempotency pattern: "only enter the conditional if the target state hasn't been reached." Enables:
```yaml
- runFlow:
    when:
      notVisible:              # ← NEW — enter gate only if not already past it
        id: 'qa-bubble'
    file: enter-qa-mode.yaml
```
- Runtime: fires the conditional when the selector is NOT visible.
- Same OCR-aware `check_selector_visible` under the hood — no wire divergence from the `visible` gate.
- Mutually exclusive with `when.visible` at parse time (both Some → parse error with clear message).
- Same `#[serde(default, skip_serializing_if = "Option::is_none")]` treatment — additive on the enum.

### Better `runFlow` Skipped diagnostic

Pre-v1.0.24 message: `runFlow when.visible=false; skipped inline body (5 steps)`. Told consumers nothing about WHAT was checked. Now the reason includes the selector's `describe_selector` form:
```
runFlow when.visible=false ({ fallback=[{ text="For internal testers only" }, { ocr_text="For internal testers only" }] }); skipped inline body (5 steps)
```
For `notVisible`:
```
runFlow when.notVisible visible=true ({ id="qa-bubble" }); skipped subflow enter-qa-mode.yaml
```
Consumers get selector shape + evaluation outcome in one line — sufficient to grep the runner log + know if the gate misfired.

### Wire compatibility

- `Step::RunFlowConditional.when_not_visible` + `Step::RunFlowInline.when_not_visible` new fields, both `Option<Selector>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Pre-v1.0.24 serialized flows deserialize with `when_not_visible: None` = unchanged behaviour.
- `runFlow.when.notVisible` parser is additive on the accept-set. Any yaml that parsed before still parses.
- Runtime `evaluate_run_flow_gate` shared method centralizes the gate evaluation for both variants and both gate types.
- No wire / HTTP surface changes.
- CLI / other subsystems byte-identical to v1.0.23.

### Ship gate

- 3 new parser tests: `parse_run_flow_conditional_when_not_visible`, `parse_run_flow_inline_when_not_visible`, `parse_run_flow_when_visible_and_not_visible_together_rejects`. 66 total parser tests green (+3 new).
- Full workspace `cargo check` + `cargo test` green (119 test-result-ok buckets, unchanged bucket count).
- Real-sim empirical validation pending on the next dogfood batch — the failing case (a qa-gate ceremony wrapped in `runFlow.when.visible` with `fallback: [text, ocrText]`) is ready to test.

## [1.0.23] — 2026-07-12

**4 new fixes extending the v1.0.22 OCR-runtime work** to `tapOn` / `scrollUntilVisible`, plus a snapshot-freshness diagnostic and an ergonomic bare-string auto-OCR opt-in. The v1.0.22 OCR-fallback path was validated on real workload (a bootstrap batch that passed 3/3 on v1.0.22 vs 0/3 on v1.0.21 with the same yaml), and the auto-captured `.smix/timeouts/*.png` cut triage-per-failure from 4-5 turns down to 1.

### `tapOn: fallback` implicit poll window when OcrText present

Pre-v1.0.23 `run_tap` fallback was one-shot: try every sub-selector once, exit. On iOS 26.5 + RN 0.86 Fabric the tap moment often races the app's post-transition mount — OCR misses because Vision snapshots BEFORE the target text is visible, not because it isn't there.

Fix: when the fallback contains any `OcrText` sub-selector, the whole chain is now polled within `SMIX_TAP_OCR_POLL_MS` (default 3000 ms) at 250 ms cadence. First hit wins. Fast path (no OCR anywhere) unchanged: single pass, no poll — pre-v1.0.23 semantics preserved for tree-only selectors.

The failure hint now names the poll budget for consumer visibility: `v1.0.23 poll budget (SMIX_TAP_OCR_POLL_MS, default 3000 ms) activates automatically when Fallback contains OCR — bump it if your post-transition mount is slower than 3 s.`

### `scrollUntilVisible` fires OCR between scroll strokes

Pre-v1.0.23 `Step::ScrollUntilVisible` delegated to `driver.scroll`, whose tree-only resolver couldn't see off-screen targets in degraded a11y trees (RN 0.86 Fabric LazyColumn/LazyRow drops off-screen items on iOS 26.5).

Fix: new adapter path `scroll_until_visible_with_ocr` — activates when the selector contains any `OcrText`. Per iteration: probe via `App::find` (tree) AND `App::find_by_text_ocr` (OCR); first hit stops the scroll. `scroll_screen(direction)` swipes between iterations. 30-swipe budget + 20 s wall (matches driver's `driver.scroll`). Fast path (no OCR anywhere) unchanged.

Shared helper `check_selector_visible` used by both the tapOn poll and the scroll poll — single implementation of "probe this selector once via tree + OCR".

### `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` response headers on /tree

Consumers debugging `--all` batch snapshot drift want a signal that the runner is (or isn't) doing fresh work. Fix — two additive headers, wire body byte-identical:

- `X-Tree-Snapshot-Refresh-Count` — cumulative /tree successful serves since runner boot (monotonic UInt64). Consumers subtracting between calls detect stalls.
- `X-Tree-Snapshot-Wall-Ms` — how long THIS `snapshotHandler` invocation took end-to-end. Trending upward across a batch = XCUITest bogging down under sustained JS reload pressure.

An alternative proposal — adding `snapshotAgeMs` / `snapshotRefreshCount` to the JSON body under a wrapping `root` field — was rejected: a wire body wrap would break every existing consumer that parses the top-level as an A11yNode. Headers are additive — pre-v1.0.23 consumers see zero change; new consumers add a single header read.

### `SMIX_AUTO_OCR_FALLBACK=1` bare-string auto-lift

In a real consumer corpus, every flow spelled out the 3-line `visible: fallback: [text, ocrText]` form. Fix — env-opt-in `SMIX_AUTO_OCR_FALLBACK=1` lifts bare-string `visible: 'X'` to `visible: fallback: [text: X, ocrText: X]` at parse time. Reduces yaml boilerplate ~40% for degraded-tree callers; portable back down to versions with less-degraded trees (bare form still parses without the env).

Accepted truthy values: `1`, `true`, `TRUE`, `yes`. Anything else (including unset) leaves bare strings as `Selector::Text` — pre-v1.0.23 semantics preserved.

Reading the env at PARSE time (not RUN time) keeps the emitted Selector shape stable across a flow — you can't have "sometimes this yaml parses to Text, sometimes to Fallback" depending on runtime state, which would violate the parser's determinism contract.

4 new parser tests locked (`parse_visible_bare_string_default_stays_text`, `..._with_env_lifts_to_fallback`, `..._with_env_true`, `..._with_env_zero_stays_text`) + Mutex-serialized to survive Cargo's default parallel test execution.

### Wire compatibility

- `X-Tree-Snapshot-Refresh-Count` + `X-Tree-Snapshot-Wall-Ms` headers additive — pre-v1.0.23 consumers see zero change.
- `SMIX_AUTO_OCR_FALLBACK` env-off ⇒ bare-string `visible: 'X'` still parses to `Selector::Text`.
- tapOn / scrollUntilVisible without any OCR in the selector: fast path preserved — no polling, no perf change.
- CLI / other subsystems byte-identical to v1.0.22.

### Ship gate

- 119 test-result-ok buckets across the workspace green (63 parser + 56 elsewhere; +4 new parser tests for the auto-lift).
- Full workspace `cargo check` green + Swift build green.
- Real-sim empirical validation pending on the next dogfood batch — failing cases for both tapOn OCR (a force-update Skip flake) and scrollUntilVisible OCR (an off-screen deeplink panel) are ready.

## [1.0.22] — 2026-07-12

**iOS 26.5 + RN 0.86 Fabric tree-degradation triage upgrade.** On Xcode 26.6 + iOS 26.5 sim + RN 0.86 New Arch (Fabric), `GET /tree` returns every child under the app root with empty `identifier` and empty `label` — nodes visibly showing (e.g.) a login button that carries JSX `testID` + `accessibilityLabel` + `accessibilityRole="button"` + `accessible={true}`. Bootstrap flows time out on the first `extendedWaitUntil` regardless of resetAppData / clearAppData choice, and the `fallback: [ocrText]` last resort silently never fires. Three fixes land:

### `extendedWaitUntil.visible.fallback: [ocrText: ...]` now actually calls OCR

Parser accepted `ocrText` in fallback since v1.0.20, but the runtime dispatched every selector through `App::wait_for` which uses the tree resolver — the tree resolver skips `Selector::OcrText` (correct behavior in isolation; OCR is meant to be dispatched at the adapter layer). Consumers who spelled `fallback: [id, text, ocrText]` got 45 s of pure `/tree` polls and never a single Vision call.

New adapter method `wait_for_visible_with_ocr` splits the fallback per poll iteration:
- Tree-resolvable sub-selectors (Id / Text / Label / Role / LocalizedText / Anchor / AnchorRelative / Focused / Point) fire via `App::find`.
- `OcrText` sub-selectors fire via `App::find_by_text_ocr`.
- First hit wins. OCR members run LAST in each iteration so tree hits pre-empt OCR cost.
- Standalone `Selector::OcrText` at top level: polls `find_by_text_ocr` on the same 250 ms cadence as the driver.
- Fast path: selectors without any `OcrText` anywhere still delegate to `App::wait_for` unchanged.

Timeout emits a per-layer trace: `L1 id=btn-…: MISS; L2 text=Log in: MISS; L3 ocrText=Log in: MISS`.

### Screenshot + tree JSON always captured on `extendedWaitUntil` timeout

Pre-v1.0.22 required `--debug-output <dir>` to get a fail PNG + tree snapshot. Consumers debugging a tree-degradation regression in CI didn't have that wired; every timeout left them blind. Now every `extendedWaitUntil` timeout auto-captures both.

Sink resolution:
1. `--debug-output <dir>` if set (same convention as per-step debug).
2. Else `<CWD>/.smix/timeouts/` (repo-scoped triage; already in typical gitignores).
3. Else `~/.local/share/smix/timeouts/`.

File names: `timeout-extendedWaitUntil-<epoch-ms>.png` + `.tree.json`. The written paths are appended to the failure's existing hint (`v1.0.22 timeout capture: screenshot=<path> tree=<path>`) so AI-readable output surfaces them.

Best-effort: any screenshot / tree / I/O error is logged to stderr and does not affect the failure verdict.

### `A11yNode.elementTypeRaw` numeric on wire (partial fix for RN Fabric tree gap)

The root-cause diagnosis: iOS 26.5 XCUITest returns empty `identifier` and empty `label` for RN 0.86 Fabric-mounted views despite the JSX setting `testID` and `accessibilityLabel`. That's an app-side (RN → UIAccessibility bridge) issue, not a smix serializer bug. But smix consumers had no way to see that from the wire: `rawType` was the only exposed type info, and the numeric `XCUIElement.ElementType.rawValue` was lost.

Now `A11yNode.elementTypeRaw: u64` ships on every wire payload. Consumer client-side triage:
- `elementTypeRaw != 1 && identifier == "" && label == ""` ⇒ iOS types this as a real element (`.button`, `.textField`, `.staticText`, ...) but the a11y bridge dropped its name — app-side fix needed (RN 0.86 Fabric accessibility bridge on iOS 26.5).
- `elementTypeRaw == 1` (`.other`) ⇒ plain wrapper view, expected to be nameless.

Consumers can now distinguish "smix bug" from "RN bridge dropped the name" in one field lookup.

Additive on the A11yNode wire; `#[serde(default = "default_element_type_raw")]` returns 1 (`.other`) for pre-v1.0.22 payloads.

### Wire compatibility

- `DiagnosticDumpResponse` unchanged.
- `A11yNode.elementTypeRaw: u64` new (default 1) — pre-v1.0.22 consumers ignoring it see zero behaviour change.
- `extendedWaitUntil` semantics preserved for selectors without OCR anywhere.
- Timeout capture is additive — hint on the failure gets extra lines; the failure code / message / structure otherwise unchanged.

### Ship gate

- 119 test-result-ok buckets across the workspace (all pre-existing + new); no regressions.
- Full workspace `cargo check` green.
- Real-sim empirical validation pending on the next dogfood batch: the failing case — `fallback: [id, text, ocrText]` yaml + a screen where the a11y tree is degraded — is ready. If OCR fires and the tree-JSON capture surfaces at timeout, the first two fixes are proved. The numeric type field informs the app-side fix or the choice to fall through to OCR.

## [1.0.21] — 2026-07-12

**iOS 26.5 UIAlertController button role mapping fixed.** Dogfooding reported `tapOn: { role: button, name: 'Reload' }` (newly-parsing in v1.0.20) regressed 3/3 flows on iOS 26.5 sim — the wire and parser are correct, but iOS 26.5 XCUITest now exposes `UIAlertController` action buttons with `elementType == .other` (rawValue 1) instead of `.button` (rawValue 9). Same failure mode expected for SwiftUI `.confirmationDialog`, `.actionSheet`, keyboard `return`/`done` bar buttons on iOS 26+.

### Swift-side action-container button promotion

Fixed at the perception layer (`swift-bridge/Sources/SmixRunnerCore/TreeRoute.swift`, `nodeToDict`). When emitting a tree snapshot, if a node is inside an `.alert` / `.dialog` / `.sheet` ancestor at any depth AND has a non-empty label AND its own elementType is `.other` (1) or `.staticText` (48), the wire `rawType` is promoted to `"button"`. This preserves `role: button` semantics across iOS versions without requiring per-consumer yaml patches.

- Promotion is **ancestor-scoped**, not global — a `.other` node outside an action container stays `"other"`.
- Promotion requires a **non-empty label** — decorative background views under an alert are not swept up.
- Promotion **never demotes** — a real `.button` (rawValue 9) inside an action container stays `"button"`.
- Nested containers (a sheet inside an alert) don't loop-double-promote; we track a single boolean.

### Wire compatibility

- `rawType` field on the wire is unchanged in shape (still `String`).
- Existing yaml `role: alert` / `role: dialog` / `role: sheet` targeting the container itself is unaffected — we only touch descendant elementTypes, not the container's own.
- Pre-v1.0.21 consumers that were matching alert-buttons via `text:` or `id:` still see the same match — text and id fields aren't touched.
- CLI / adapter parser is byte-identical to v1.0.20.

### Ship gate

- 7 new Swift unit tests in `TreeRouteTests` (`test_serialize_alertOtherChildWithLabel_promotedToButton`, `..._alertStaticTextChildWithLabel_...`, `..._dialogNestedButton_...`, `..._alertOtherChildNoLabel_notPromoted`, `..._otherOutsideActionContainer_notPromoted`, `..._realButtonUnderAlert_stillButton`, `..._sheetOtherChild_...`) — 26 TreeRoute tests total, all green.
- Real-sim empirical verification pending on the next dogfood batch (the failing 3/3 case — an alert-button `role: button, name: 'Reload'` yaml — is ready and will confirm v1.0.21 resolves it).

## [1.0.20] — 2026-07-12

**3 docs/impl gaps closed.** The v1.0.19 observability wins (`lastInteractiveNamedIds` at top-level + `AppUnavailableReason` disambiguation) held up on real workload — bootstrap batch flow completion reached 2/3 passing, with the remaining failure isolated to a downstream app-side native race (`expo::setProperty` / `ConstantDefinition.buildDescriptor`), not smix.

### `extendedWaitUntil.visible` accepts every selector key `tapOn` does

`docs/ai-guide/03-selectors.md` promised `ocrText:` as a first-class selector everywhere. Reality: `visible_to_selector` in `crates/smix-adapter-maestro/src/parser.rs` only accepted `text` and `id`. Fixed — now accepts every base selector form: `text`, `id`, `label`, `role` (+ optional `name`), `ocrText`, `localized_text`, `fallback`.

All 8 verbs that route through `visible_to_selector` benefit at once: `extendedWaitUntil.visible/.notVisible`, `assertVisible`, `assertNotVisible`, `scrollUntilVisible`, `copyTextFrom`, `runFlow.when.visible`, `tapOn.anchored.anchor`.

### `tapOn: {role, name}` + `tapOn: {label}` parse

`Selector::Role` wire type exists (since v5.x); the yaml parser just wasn't wiring it. Fixed — `parse_tap_on` now accepts:

```yaml
- tapOn:
    role: button        # camelCase (wire) or lowercase (docs-friendly) — both work
    name: 'Submit'      # optional Pattern (literal or |-alternation regex)

- tapOn:
    label: 'Home tab'   # accessibilityLabel strict equal (Selector::Label)
```

Role parser tolerates docs-friendly aliases: `role: textfield` → `Role::TextField`; `role: checkbox` → `Role::CheckBox`; `role: heading` → `Role::StaticText` (nearest wire equivalent since iOS/SwiftUI has no `.header` XCUIElement type). Unknown roles emit an actionable error listing every accepted variant.

### `smix run --dry-run` alias for `--check`

`--check` already existed with the exact "parse-only, no runner, no simulator" semantics consumers asked for, but `--dry-run` is the idiomatic name in most CLI tools. Added `--dry-run` as a clap alias for `--check`; output prefix changed to neutral `smix run: parse OK/FAIL <path> (N steps)` so it reads correctly under either name. Also appends a summary line: `smix run: parse OK — N flow(s), M total step(s)`.

### Wire compatibility

- `smix_selector::Role` re-exported at crate root (was `pub use smix_screen::Role` internally) — adapter crates can now `use smix_selector::Role` without pulling `smix-screen` directly.
- All parser changes are additive on the accept-set — no yaml that parsed before still fails.
- Docs updated in `docs/ai-guide/03-selectors.md §4 Role` to enumerate every supported role and note the "role works anywhere a selector map does" broader guarantee.

### Ship gate

- 59 parser tests (5 new: `parse_extended_wait_until_visible_ocr_text`, `..._role_name`, `..._label`, `parse_tap_on_role_name`, `..._role_lowercase_alias`, `..._role_unknown_errors_actionably`, `..._label`) + 25 CLI runner tests + all pre-existing green across touched crates.
- CLI dry-run smoke on 3-step yaml with `tapOn: {role, name}` + `extendedWaitUntil.visible: {ocrText}` + `tapOn: {label}` — parses clean, reports "3 steps, 1 flow".
- Unknown role smoke — emits full accepted-roles list, exit 2.

## [1.0.19] — 2026-07-12

**Post-mortem triage QoL.** Real-workload validation of v1.0.18 confirmed:
- Both v1.0.18 wins (D1 per-session `interactiveNamedIds` + D2 `waitForAnimationToEnd: N`) landed cleanly.
- **`.ips` growth 36→36 across 5 consecutive batches** — native cold-boot crash chain closed decisively (v1.0.14 → v1.0.18).
- Flow depth advanced 6–8 steps in every case; remaining stalls all on target-screen `waitFor { text: … }` (downstream RN Fabric a11y-label propagation, not smix).

### Top-level `lastInteractiveNamedIds` on `/diagnostic/dump`

Per-session `interactiveNamedIds` (v1.0.18) goes with the session when `close-all` teardown fires. Post-batch triage often runs AFTER teardown, so the sample vanishes right when consumers want it.

Wire additions (all `#[serde(default)]`, backward-compat):
- `DiagnosticDumpResponse.last_interactive_named_ids: Vec<String>` — most-recent non-empty sample across all `launchApp` completions since runner boot. Survives session close.
- Swift `SessionRoute.DiagnosticSnapshot.lastInteractiveNamedIds: [String]`; runner-side `LastInteractiveIdsBox` holder updated on every non-empty launch outcome.
- `smix diagnostic dump` (text mode) prints one line: `lastInteractiveNamedIds (N): id1, id2, ...` or `[]  # no launch has completed with a non-empty sample yet`.
- `smix diagnostic dump --json` emits the same field on the top-level `runner` object.

Per-session `sessions[n].interactiveNamedIds` from v1.0.18 remains — this new top-level field is the "last-values-standing" post-teardown observation surface, not a replacement.

### Wire compatibility

- `DiagnosticDumpResponse.last_interactive_named_ids` is `#[serde(default)]` + on a `#[non_exhaustive]` struct — pre-v1.0.19 consumers ignoring it see zero behaviour change.
- No new HTTP routes. No CLI flag changes. No yaml schema changes.

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline v1.0.18 behaviour unchanged; every previous assertion still holds.
- **Verified**: after 1 launch of Preferences, `curl -s -X POST /diagnostic/dump | jq '.lastInteractiveNamedIds'` returns the same 8-name sample as `sessions[0].interactiveNamedIds` and as the launch-app response. After closing that session via `/session/close-all`, `sessions` becomes empty but `lastInteractiveNamedIds` still holds the 8-name sample.

## [1.0.18] — 2026-07-12

**Two QoL additions**, landed alongside real-workload validation of v1.0.17:
- v1.0.17 crash fix confirmed working (0 test_runForever failures, 0 "Failed to get matching snapshot" entries)
- `launchAppReachedInteractive: 6/6` — every launch reached probeable tree
- `.ips` growth 36→36 — native crash triple stays fully closed
- Remaining flow failures **not a smix bug** — RN Fabric a11y-exposure lag during animation (post-tapOn transitions); downstream timeouts + testIDs + `waitForAnimationToEnd` are the knobs

### Per-session `interactiveNamedIds` in `session/list` + `/diagnostic/dump`

Previously only surfaced on `/session/launch-app` response body. The counter alone doesn't tell "probe fired on dev-bubble" from "probe fired on splash-screen artifacts."

Wire additions (all `#[serde(default)]`, backward-compat):
- Swift `SessionRoute.SessionSummary.interactiveNamedIds: [String]` (default empty).
- Swift `SessionEntry.lastInteractiveNamedIds: [String]` on the session table — updated on every `launchApp` completion.
- `session/list` + `/diagnostic/dump` JSON both now include `sessions[n].interactiveNamedIds`.

### `waitForAnimationToEnd` numeric override + doc

Consumers weren't sure if `waitForAnimationToEnd` was a no-op under `SmixQuiescenceSwizzle.m`. Reality: it never went through XCTest idle-wait in the first place — it's always been a fixed 400 ms `tokio::time::sleep`. Undocumented.

Fix:
- yaml accepts `- waitForAnimationToEnd: 500` (integer = ms sleep). Bare form still parses to 400 ms default (maestro-compat).
- `Step::WaitForAnimationToEnd { duration_ms: u64 }` — struct variant.
- Runtime dispatch sleeps the requested milliseconds.
- Docstring on the variant explicitly names that it's a fixed sleep, NOT XCTest quiescence.

2 new parser tests locked (`bare_default_400ms`, `numeric_override`).

### Wire compatibility

- `SessionSummary.interactiveNamedIds` is `#[serde(default)]` — pre-v1.0.18 consumers ignoring the field see zero behaviour change.
- `Step::WaitForAnimationToEnd` variant became `{ duration_ms }` — consumers of the yaml wire (yaml → Step conversion, not `Step` construction in user code) unaffected. Test fixtures using struct literal `Step::WaitForAnimationToEnd` updated.
- No runner-side HTTP surface changes.

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline: `POST /session/launch-app` still returns `reachedInteractive:true` + 8 sample ax-ids as v1.0.17.
- **Verified**: after launch, `session/list` and `/diagnostic/dump` both surface `sessions[0].interactiveNamedIds: ["Settings","AdditionalDimmingOverlay","com.apple.settings.primaryAppleAccount",…8]`. Same 8-name sample as the launch-app response.

682 workspace cargo tests (+2 new parser tests for D2) + all pre-existing green. No wire regressions.

## [1.0.17] — 2026-07-12

**Hotfix: v1.0.16 introduced a hard-crash mode in the interactive polling loop.** Root cause: `descendants(matching:).element(boundBy: i)` is XCTest-lazy — the element resolves at access time against the CURRENT tree. When the tree shrunk mid-iteration (a `stopApp + openLink dev-launcher` between test phases), XCTest raised an unrecoverable assertion "No matches found for Element at index N" that killed `test_runForever` and the runner process, taking subsequent flows down with it.

**Before this crash surfaced, the v1.0.16 snapshot-refresh DID help** — `force-update.yaml` reached STEP 47/47 vs the previous max of 34, and `.ips` growth stayed at 36 → 36 (native crash triple stays fully closed).

### Walk frozen `XCUIElementSnapshot` instead of live-query enumeration

Replaces:
```swift
_ = try? entry.app.snapshot()
let query = entry.app.descendants(matching: .any)
for i in 0..<query.count {
  let el = query.element(boundBy: i)   // lazy resolution at access → hard-fail on shrink
  ...
}
```

with:
```swift
guard let snap = try? entry.app.snapshot() else { return [] }
collectInteractiveIds(snap.dictionaryRepresentation, ignore, ids, ...)
```

- `snap.dictionaryRepresentation` returns a frozen in-memory tree that we walk recursively, collecting non-empty `accessibilityIdentifier` values not in the ignore list. Same pattern the runner already uses for modal popup collection (see `collectPopupNodes`) and keyboard focus detection (see `FocusedIdentifier.find`).
- `snapshot()` itself still forces XCUITest to re-scrape the a11y hierarchy from scratch (v1.0.16 fix for the Fabric mount-item-drain race). The walk over the returned snapshot is safe against any subsequent tree mutation.
- Pathological-tree stall guard: walk stops at 200 enumerated nodes (guards against runaway lists).

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline: `POST /session/launch-app waitForInteractiveMs:15000` → `HTTP 200, reachedInteractive:true, interactiveNamedIds:["Settings","AdditionalDimmingOverlay",…8]`. Snapshot-walk yields the same result as v1.0.15/v1.0.16 on the working Preferences case.
- **Stress test — 3 rapid terminate + launch cycles** to trigger the tree-shrink race pattern observed in the field. Every cycle returned `reachedInteractive:true` and runner stayed reachable after all cycles. `/health` still returning 200. v1.0.16 in the same scenario would have crashed after 1-2 cycles.

### Wire compatibility

- No wire changes. All v1.0.15 wire shape unchanged.
- Runner-side behavior change is invisible to consumers unless polling was hitting the tree-shrink race, in which case runner-death → runner-alive is the observation flip.

680 workspace tests + all pre-existing tests green.

## [1.0.16] — 2026-07-12

**Hotfix: v1.0.15's interactive polling had a stale-snapshot bug on RN Fabric + iOS 26.5 sim.** The exact race: RN 0.86 Fabric New Arch populates the a11y tree via `RCTMountItemProtocol` as mount items drain, NOT during layout. XCUITest's snapshot cache holds the sparse pre-drain tree, and `descendants(matching:)` returned the same cached snapshot every poll iteration.

### Swift snapshot-refresh in interactive polling

- `launchApp` handler now calls `_ = try? entry.app.snapshot()` on every polling iteration before reading `descendants(matching:)`. Forces XCUITest to re-scrape the a11y hierarchy from scratch, catching mount-item-drain updates.
- No `waitForQuiescenceIncludingAnimations` call — smix's existing `SmixQuiescenceSwizzle.m` already no-ops that private XCTest daemon idle-wait for performance. Snapshot alone forces the invalidation.
- `.smix/config.yaml interactiveProbe:` schema unchanged. Config-driven ignore-list and minIdentifierCount still work as v1.0.15 shipped.

### yaml `launchApp: { waitForInteractiveMs }` marker

- Parser accepts the new field on the map form of `launchApp:`.
- `Step::LaunchApp.wait_for_interactive_ms: Option<u64>` — additive; `#[serde(default)]`.
- Runtime: emits a warning (non-fatal) explaining the SDK launch pathway (`simctl launch --args`) is host-side and can't route to `/session/launch-app`. Consumers who want interactive gating use the `clearAppData` yaml verb instead — its SDK path defaults `wait_for_interactive_ms: Some(30_000)` since v1.0.15. Full first-class routing lands in a follow-up release that unifies the two launch pathways.

### Ship gate (real-sim, iOS 26.5 Preferences)

- Baseline reproducibility check — the v1.0.16 snapshot-refresh doesn't regress the working Preferences case that v1.0.15 shipped on:

```
POST /session/launch-app  {sessionId, waitForForegroundMs:15000, waitForInteractiveMs:15000}
→ HTTP 200
→ reachedInteractive:true
→ interactiveNamedIds:["Settings","AdditionalDimmingOverlay",
                       "com.apple.settings.primaryAppleAccount", …8]
```

Real-world validation (a consumer bootstrap batch on RN Fabric + iOS 26.5) happens downstream — launch-fresh flows migrate to `clearAppData` (which gets the interactive probe with v1.0.15's default and now v1.0.16's snapshot-refresh) and rerun.

### Wire compatibility

- No wire changes (v1.0.15 wire shape unchanged).
- Runner-side behavior change is invisible to consumers unless the polling loop was hitting the stale-snapshot case; when it was, the observation flip is (a) v1.0.15 always saw `reachedInteractive:false` on Fabric or (b) v1.0.16 sees `reachedInteractive:true` once the tree actually populates.

680 workspace tests + all pre-existing tests green.

## [1.0.15] — 2026-07-11

**Interactive-probe polling + app-unavailable reason disambiguation + retry attribution — the v1.0.14 deferred work.** Wire scaffolding from v1.0.14 now populated with the Swift + CLI implementation.

### Interactive-probe polling (Swift-side)

- Wire: `SessionAppLifecycleRequest.wait_for_interactive_ms: Option<u64>` (additive; `#[serde(default)]`).
- Wire response: `SessionAppLifecycleResponse.reached_interactive: bool` + `interactive_named_ids: Vec<String>` (up to 8 sample ax-ids captured at fire moment).
- Runner: after `.state == .runningForeground` is observed, the `launchApp` handler polls `entry.app.descendants(matching: .any)` at 500 ms cadence, counts descendants with non-empty `accessibilityIdentifier` NOT in the ignore-list, fires `reachedInteractive` on ≥ `minIdentifierCount`, or times out and increments `launchAppTimedOutBeforeInteractive`.
- Config file: `.smix/config.yaml interactiveProbe: { minIdentifierCount: 3, ignore: [SplashScreenLogo, com.example.app] }`. CLI reads via `serde_norway`, JSON-encodes, forwards to runner as `TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON`. Runner falls back to bundled defaults when absent.
- SDK: `App::clear_app_data_with_launch` defaults `wait_for_interactive_ms: Some(30_000)` — consumers using yaml `clearAppData` automatically see `launchAppReachedInteractive` counter delta with zero yaml migration.
- Counter fields `launch_app_reached_interactive` + `launch_app_timed_out_before_interactive` in `SessionLifecycleCounters` are now populated by the runner (were 0 in v1.0.14 wire-scaffold).

### `AppUnavailableReason` enum + hint field on `/tree` unavailable envelope

- Swift `TreeRoute.unavailable(reason:hint:)` variant emits enriched `{"ok":false,"error":"snapshot_unavailable","reason":"alive-but-tree-empty","hint":"…"}` body. Legacy `TreeRoute.unavailable()` still present for compat.
- Swift `AppUnavailableReason` enum: `crashedDuringInit` / `aliveButTreeEmpty` / `aliveButTreeStale` / `driverDisconnected` / `unknown`. Each carries a `defaultHint: String` steering downstream tooling.
- Runner-side detection in `SmixRunnerServer.swift` `/tree` handler:
  - Cache-suppressed short-circuit → `crashed-during-init` (observed XCTIssue about app not running).
  - Snapshot handler returned nil → consults `currentUnavailableReasonInferer` task-local closure. UITest target reads `XCUIApplication.state` for the current bundle: `.notRunning` → `crashed-during-init`; foreground/background running → `alive-but-tree-empty`; unknown → `.unknown` fallback.
  - Fallback (guarded closure threw entirely) → `driver-disconnected`.
- Wire in `smix-runner-client`: `RunnerTransportError::AppUnavailable` gains `category: Option<String>` + `hint: Option<String>` fields. `classify_error_body` discriminates v1.0.15 category values (`crashed-during-init` etc.) from legacy free-form `reason` strings; both populate cleanly for backward compat.
- Pre-v1.0.15 runners emitting legacy `{"ok":false,"error":"snapshot_unavailable"}` land in `category: None, hint: None` — the consumer's error message stays functional either way.

### `smix run --retry N` + per-flow attempt attribution

- CLI: new `--retry <N>` flag on `smix run` (default 1 = pre-v1.0.15 behaviour).
- Runtime: each flow wrapped in an attempt loop; retries only fire on non-zero exit; first success short-circuits.
- Per-attempt tracking captures `attempt_index`, `status` (`ok`/`timeout`/`error`), `error_class` (`TIMEOUT`/`DRIVER_ERROR`/`EXPECTATION_FAILURE`/`RUNNER_UNREACHABLE`), `wall_ms`, and any new `.ips` filename that appeared under `~/Library/Logs/DiagnosticReports/` during the attempt's window (attribution vs whole batch).
- Persistence: `~/.local/share/smix/flow-attempts.json` (last 32 flows) via new `smix-simctl::set_flow_attempts_persist_path` (parallels the v1.0.7 `subprocess_ring` and v1.0.14 `reset_app_data_counters` patterns).
- CLI dump overlay: `smix diagnostic dump` (non-JSON) renders a new `=== recent flows (retry attribution) ===` section per flow with per-attempt lines; `--json` payload gets `runner.recentFlows: Vec<FlowAttemptRecord>` (wire type land in v1.0.14).

### Wire compatibility

- All new request/response fields carry `#[serde(default)]`. Pre-v1.0.15 clients see zero behaviour change.
- `SessionAppLifecycleRequest.wait_for_interactive_ms: Option<u64>` — opt-in.
- `SessionAppLifecycleResponse.reached_interactive: bool` + `interactive_named_ids: Vec<String>` — additive.
- `TreeRoute.unavailable(reason:hint:)` — new variant; legacy `unavailable()` kept.
- `RunnerTransportError::AppUnavailable.category` + `.hint` — additive Option fields.
- `SessionLifecycleCounters.launch_app_reached_interactive` + `launch_app_timed_out_before_interactive` — already in v1.0.14 wire; v1.0.15 populates.
- `DiagnosticDumpResponse.recent_flows` — already in v1.0.14 wire; v1.0.15 populates via CLI overlay.

### Ship gate (real-sim, iOS 26.5)

```
$ smix --version                                     → smix 1.0.15
$ smix runner install --force                       → extracted 303 files at v1.0.15
$ /health.runnerVersion                              → "1.0.15"

$ curl -X POST /session/open …
$ curl -X POST /session/launch-app -d '{"sessionId":"…","waitForForegroundMs":15000,"waitForInteractiveMs":15000}'
→ HTTP 200
→ reachedInteractive: true
→ interactiveNamedIds: ["Settings", "AdditionalDimmingOverlay", "com.apple.settings.primaryAppleAccount", …8 sampled]

$ smix diagnostic dump | grep -A1 interactive
  interactive: reachedInteractive=1 timedOutBeforeInteractive=0  # timedOut>0 → process foreground but a11y tree unusable

$ /diagnostic/dump payload sessionCounters
  launchAppReachedInteractive: 1
  launchAppTimedOutBeforeInteractive: 0
```

680 workspace tests + all pre-existing tests green. `smix run --retry` mechanism not exercised in real-sim gate (needs a yaml with flaky expectations to fail-then-retry, out of scope for Preferences smoke); implementation locked by static tests.


## [1.0.14] — 2026-07-11

**resetAppData verb (URL-scheme JS-wipe) + external metro log tail (`--metro-log <path>`) + verb-selection guide.**

Version jump 1.0.11 → 1.0.14 (no interim v1.0.12 or v1.0.13 published).

### `resetAppData` verb (URL-scheme JS-wipe)

Fixes the "dev-fixture ceremony cost" problem: every prior `clearAppData` wiped the app's container INCLUDING expo-dev-client's persisted metro URL + Metro bundle cache + dev-tools state — replaying a 15-30 s dev-client cold-boot ceremony every launch.

New verb: `resetAppData` fires an app-owned URL scheme on the host (`simctl openurl <UDID> <url>`), optionally waits for a completion signal on the external metro log tail, then returns. No container tear. Consumer app decides scope (typically `mmkv.clearAll()` + `console.log('[dev] reset-complete token=<uuid>')`).

yaml shapes:

```yaml
# short form
- resetAppData: 'myapp://dev-mutate?action=reset'

# map form
- resetAppData:
    via: url-scheme            # only 'url-scheme' today; extensible
    url: 'myapp://dev-mutate?action=reset'
    waitFor:
      logLinePattern: '\[myapp-dev\] reset-complete token='
      # OR: sleepMs: 500 (best-effort fallback when --metro-log unset)
    timeoutMs: 5000
```

- `Step::ResetAppData { url, wait_for, timeout_ms }` in `smix-adapter-maestro`; parser accepts short-form + map-form.
- `smix_sdk::ResetAppDataWaitFor` enum (`LogLinePattern(String)` / `Sleep(u64)`) shared between adapter Step and SDK.
- Runtime dispatch fires `simctl openurl` via `App::open_url`, then either sleeps or awaits a `smix_metro_log::MetroLogTail::await_signal` match — the tail is provided by `smix run --metro-log <path>`.
- `smix-simctl::increment_reset_app_data_total()` + `increment_reset_app_data_timed_out()` counters, persisted to `~/.local/share/smix/reset-app-data-counters.json` so `smix diagnostic dump` (later, separate process) surfaces the counts.

Wire counter fields in `SessionLifecycleCounters`: `reset_app_data_total`, `reset_app_data_timed_out`. CLI-side populated (host-side dispatch, no runner HTTP round-trip for the reset itself).

### External metro log tail (`--metro-log <path>` on `smix diagnostic dump`)

Fixes the "log gate skipped — metro was already running externally" problem: consumers who spawn metro externally (`nohup bun dev > /tmp/metro.log`) couldn't see JS-side log signal when a flow stalled.

- New CLI flags on `smix diagnostic dump`:
  - `--metro-log <path>` — tail the last N lines from this file at dump time.
  - `--metro-log-tail-lines <N>` — default 200.
- New `tail_lines(path, n)` helper — seeks from EOF in 8 KB chunks, splits on `\n`, handles UTF-8 split across chunk boundaries, files smaller than one chunk, files with no trailing newline. 6 unit tests locked.
- New wire field `DiagnosticDumpResponse.metro_log_tail: Vec<String>` — CLI-side populated at dump time (not runner). Backward-compat additive.
- CLI display gains a `=== metro log tail (last N of file) ===` section when populated.
- Also lands `smix diagnostic dump` sections for `resetAppData` counters + `interactive` counters (v1.0.15 will populate the latter).

For runtime tail during `smix run` (used by v1.0.14's `resetAppData waitFor: { logLinePattern }` and pre-existing `expect.signal` verbs), the existing `smix-metro-log FileTailSubscriber` + `MetroLogTail` continue to serve — no new subscriber design required.

### Verb-selection guide

- A verb-selection guide — decision tree + comparison matrix for `clearAppData` vs `resetAppData` vs `clearState + clearKeychain`, plus a migration crib from pre-v1.0.14 yaml to the split baseline + fast-path pattern.

### Forward-compat wire scaffolding (populated in v1.0.15)

Wire types added in v1.0.14, Swift/impl side deferred to v1.0.15 so consumers get a coherent interactive-probe release rather than a half-populated one:

- `SessionLifecycleCounters.launch_app_reached_interactive` + `launch_app_timed_out_before_interactive` (interactive-probe counters; Swift-side polling not yet wired — always 0).
- `FlowAttemptRecord` + `FlowAttempt` types + `DiagnosticDumpResponse.recent_flows: Vec<FlowAttemptRecord>` (retry attribution; --retry N mechanism not yet wired — always empty).
- All `#[serde(default)]` — a v1.0.14 consumer ignoring the fields sees zero behaviour change; v1.0.15 populates the same fields without a wire migration.

### Wire compatibility

- New request/response fields carry `#[serde(default)]` everywhere.
- `Step::ResetAppData` is a new parser entry — pre-v1.0.14 yaml unaffected.
- `SessionLifecycleCounters` gains 4 fields (2 populated by `resetAppData`, 2 scaffolded for the interactive probe).
- `DiagnosticDumpResponse` gains 2 fields (`metroLogTail` populated CLI-side, `recentFlows` scaffolded).
- No route path changes; no HTTP method changes; no runner-side behaviour change (all v1.0.14 work is on the CLI + host side).

### Ship gate observations (real-sim, iOS 26.5)

```
$ smix --version                                                              # → smix 1.0.14
$ smix runner install --force                                                 # → extracted 303 files at v1.0.14
$ cat ~/.local/share/smix/runner/.smix-runner-version                         # → 1.0.14
$ smix runner up FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1 --bundle com.apple.Preferences
runner up: http://localhost:22087/health = 200 (runner v1.0.14)
$ curl -s http://127.0.0.1:22087/health | jq .runnerVersion                   # → "1.0.14"

# --metro-log tail render
$ echo -e "line-1\nline-2\nline-3\nline-4\nline-5" > /tmp/test-metro.log
$ smix diagnostic dump --metro-log /tmp/test-metro.log --metro-log-tail-lines 3
=== metro log tail (last 3 of file) ===
  line-3
  line-4
  line-5

# New counter sections render
$ smix diagnostic dump | head
  resetAppData: total=0 timedOut=0  # timedOut>0 → URL scheme fired but reset-complete log-line never arrived
  interactive: reachedInteractive=0 timedOutBeforeInteractive=0  # timedOut>0 → process foreground but a11y tree unusable
```

680 workspace cargo tests + 3 new clearAppData parser tests + 3 new resetAppData parser tests + 6 new `tail_lines` unit tests + 1 new reset-app-data-counters roundtrip test green.


## [1.0.11] — 2026-07-11

**launchApp launchArgs/launchEnv + wait-for-foreground + always-emit aliveCache + terminate-outcome counters.**

### The three v1.0.10 followup gaps closed

- **`aliveCache: null` in `/diagnostic/dump`.** Root cause: `SessionHandlers.diagnostic` closure read `SmixRunnerServer.currentAppAliveCache` (task-local); FlyingFox's per-request task spawn wasn't propagating the `withValue` scope around `server.run()`. Fix: `test_runForever()` now extracts `AppAliveCache` to a named local `localAppAliveCache` and the diagnostic handler closure captures the reference directly (not via task-local). The dump payload always emits `aliveCache` with a `wired: bool` sentinel + all-zero counters when unwired, so consumers can distinguish "runner has no cache" from "cache present, workload didn't fire".
- **Expo SDK 57 dev-launcher server picker blocking business flow.** `clearAppData` wipes the dev-client's persisted metro URL, next launch shows the picker, SDK 57 URL scheme no longer auto-navigates. Fix: `launchApp` HTTP endpoint (and yaml `clearAppData` step) accept optional `launchArgs: []` and `launchEnv: {}` fields; forwarded to `XCUIApplication.launchArguments` / `.launchEnvironment` before launch. Consumers steer the picker via `-EXInternalMetroPort` launchArg or `EX_DEV_CLIENT_METRO_URL` launchEnv (fixture57 accepts both).
- **`bug_type: 309 exec_terminated_before_ready` `.ips` writes during clearAppData.** Diagnosis: `XCUIApplication.launch()` returns after launch is dispatched, not when the app has signalled launchd ready. Caller's next step (or a batch retry firing another clearAppData) hits terminate mid-launch → XCUIApplication times out cooperative-terminate → falls back to hard kill → launchd catches `exec_terminated_before_ready` → `.ips`. Fix: `launchApp` endpoint accepts `waitForForegroundMs: Option<u64>`. When set, the runner polls `XCUIApplication.state` every 250 ms until `.runningForeground` or the deadline. `App::clear_app_data` defaults to 15 s. Response body carries `waitedMs` + `terminalState` (0 unknown / 1 notRunning / 2 runningBackgroundSuspended / 3 runningBackground / 4 runningForeground) so `/diagnostic/dump` can surface `launchAppReachedForeground` vs `launchAppTimedOutBeforeForeground` counters.

### CLI (Rust)

- **`SessionAppLifecycleRequest` gains `args`, `env`, `wait_for_foreground_ms`** (`#[serde(default)]`; pre-v1.0.11 callers see zero behaviour change). Non-`#[non_exhaustive]` on the request side (consumers construct) but IS on the response (consumers read).
- **`SessionAppLifecycleResponse` gains `waited_ms`, `terminal_state`, `terminated_cooperatively`** (additive).
- **`DiagnosticDumpResponse` gains `alive_cache: AliveCacheCounters`** (always emitted, `wired: bool` sentinel) **and `session_counters: SessionLifecycleCounters`** (cumulative — survive close). Client-side `smix diagnostic dump` (non-JSON) renders both under new sections.
- **`App::clear_app_data_with_launch(args, env)` SDK method** — `clear_app_data()` becomes a thin wrapper. yaml `clearAppData: { launchArgs, launchEnv }` (with `args` / `env` short aliases) parses to `Step::ClearAppData { launch_args, launch_env }` and threads through.

### Runner-side (Swift)

- `SessionRoute.AliveCacheCounters.wired: Bool` sentinel; `SessionRoute.DiagnosticSnapshot.aliveCache` is non-Optional and always emitted; `SessionLifecycleCounters` embedded alongside.
- `SessionRoute.AppLifecycleRequest` decoder accepts `args`, `env`, `waitForForegroundMs`; falls back to pre-v1.0.11 shape (bare `sessionId`) so older clients still work.
- `launchApp` handler applies `entry.app.launchArguments = req.args` + `.launchEnvironment = req.env` before `.launch()`, then polls `.state` for `.runningForeground` up to `req.waitForForegroundMs` (250 ms cadence). Reports `waitedMs`, `terminalState`, `terminatedCooperatively` (always false on launch) in outcome.
- `terminateApp` handler observes `app.state == .notRunning` after `.terminate()` returns; sets `terminatedCooperatively` accordingly. `terminateAppViaXCUIApplication` counter advances when cooperative; `terminateAppViaFallback` advances when XCUIApplication timed out and fell back. `> 0 fallback` is the smoking-gun for the `.ips` diagnosis.
- Cumulative `LifecycleCounters` local (class + NSLock, no actor overhead for the sync mutations) advanced on every `open`, `close`, `relaunchApp`, `terminateApp`, `launchApp`; snapshotted into every diagnostic response.

### Wire compatibility

- All new fields carry `#[serde(default)]`. Pre-v1.0.11 clients see zero change.
- New request fields default to empty — a pre-v1.0.11 SDK still sends bare `{sessionId}` and gets pre-v1.0.11 launch semantics.
- New response fields default to zero — a pre-v1.0.11 SDK ignoring them keeps working.

### Documentation

- A standing note explaining what `AppAliveCache` protects, what `unknown` descendants mean, and how to distinguish "app dead + retry-spam broken by cache" from "app alive but a11y sparse" (the case observed on Expo SDK 57 dev-launcher).

### Ship gate

Real-sim observations (iOS 26.5) at v1.0.11:

```
POST /session/launch-app  {sessionId, args: ["-AppleLanguages","(en)"], env: {SMIX_TEST_ENV:"hello"}, waitForForegroundMs: 15000}
→ HTTP 200 {"ok":true, "wallMs":2786, "waitedMs":0, "terminalState":4, "terminatedCooperatively":false}

POST /session/terminate-app  {sessionId}
→ HTTP 200 {"ok":true, "wallMs":1050, "waitedMs":0, "terminalState":1, "terminatedCooperatively":true}

POST /diagnostic/dump
→ aliveCache: {wired: true, markAliveTotal: 1, ...}
→ sessionCounters: {openedTotal: 1, terminateAppTotal: 1, terminateAppViaXCUIApplication: 1, terminateAppViaFallback: 0, launchAppTotal: 1, launchAppReachedForeground: 1, launchAppTimedOutBeforeForeground: 0, ...}
```

`terminatedCooperatively: true` + `terminateAppViaFallback: 0` — the cooperative pathway went through cleanly on Preferences. Consumer real-app validation (Expo 57 dev-launcher bypass) happens downstream after upgrading the CLI + regenerating runner sources via the v1.0.10 auto-sync path.

667 workspace cargo tests + 6 Swift SmixRunnerCore tests + 3 new clearAppData parser tests green. Corpus gate infrastructure landed in v1.0.10.


## [1.0.10] — 2026-07-11

**Systemic fix for the CLI-vs-runner distribution drift that made v1.0.4–v1.0.9 patches silently no-op on stale on-disk runner sources.**

### Root cause (confirmed with hard evidence)

`cargo install smix` used to ship only the Rust binary; the Swift `SmixRunner.xcodeproj` + `Sources/SmixRunnerCore/` + `SmixRunnerUITests/` sources were obtained separately at consumer install time, never version-synced afterward. A consumer's on-disk `~/.local/share/smix/runner/SmixRunnerUITests/SmixRunnerUITests.swift` was 2212 lines with zero references to `sessionHandlers` / `/session/open` / `SessionHandlers` while the current repo file was 2669 lines (v1.0.9) with them present. That's why 6 consecutive CLI patches (v1.0.4–v1.0.9) shipped session lifecycle + observability + crash-dialog fixes but that consumer's runner stayed frozen at a pre-v1.0.3 revision — `/session/open` 404 100% of the time, `clearAppData` unusable, a11y-cache re-probe log line never emitted.

Secondary root cause: `GET /health` route always called `HealthRoute.response()` (legacy `{"ok":true}` since v0.x). The `runnerVersion` field CHANGELOG v1.0.2 claimed was never emitted, so version drift has been invisible client-side across every prior release.

### New crate — `smix-runner-sources`

- Ships the Swift runner project as a checked-in gzipped tarball baked into the `smix-cli` binary via `include_bytes!`.
- Regenerated by `scripts/release/build-runner-tarball.sh` (`gzip -n` reproducible).
- Excludes the 13 MB `SmixCoreFFI.xcframework` binary — that continues to be fetched separately.
- `SOURCES_VERSION = env!("CARGO_PKG_VERSION")` — matches the workspace and every ecosystem publish.

### CLI (Rust)

- **Runner project auto-sync.** `resolve_runner_project` (called by every `smix runner up`) now reads `~/.local/share/smix/runner/.smix-runner-version` before dispatching xcodebuild. On drift OR missing, the embedded tarball extracts in place, backing up any prior tree to `~/.local/share/smix/runner.bak-<ts>/`. Zero user migration on upgrade. First-run consumers get sources populated transparently.
- **`smix runner install [--force] [--path <dir>]` verb.** Explicit sync for troubleshooting, first-time-setup, or `--force` re-extract when the tree has been hand-edited. Idempotent when already current.
- **CLI forwards `TEST_RUNNER_SMIX_RUNNER_VERSION=<CARGO_PKG_VERSION>` env.** Xcode strips the `TEST_RUNNER_` prefix; runner reads `SMIX_RUNNER_VERSION` via `ProcessInfo`.
- **Client-side version-mismatch gate at `runner up`.** After `/health` returns 200, parses `runnerVersion` field; refuses boot with actionable message ("run `smix runner install --force`") on mismatch. Legacy-body runners (pre-v1.0.10) get a warning but no refusal so existing consumers aren't broken by the upgrade.
- **Subprocess-ring persistence.** `smix-simctl::set_subprocess_ring_persist_path` (called at CLI startup with `~/.local/share/smix/subprocess-ring.json`) writes-through every simctl invocation record atomically. The v1.0.7 empty-`diagnostic dump`-payload-after-supervisor-cycles gap is closed — the file survives cycles; post-mortem tools read the file, not in-memory state.

### Runner-side (Swift)

- **`/health` route wires `HealthRoute.responseDetail`.** Returns `{ok, runnerVersion, uptimeMs, lastRequestAtMs, sessionsOpen, activationsTotal}`. `runnerVersion` sourced from `SMIX_RUNNER_VERSION` env (fallback `"unknown"`).
- **`AppAliveCache` observability counters.** `markDeadTotal`, `markAliveTotal`, `suppressHitTotal`, `suppressMissTotal`, `reprobeAttemptedTotal`, `reprobeSucceededTotal`, `reprobeInvalidatedEarly`, `reprobeExhaustedWindow`. Every mutation on the actor advances the paired counter.
- **Re-probe path wired to counters.** The v1.0.9 background Task now calls `noteReprobeAttempted` at spawn, `noteReprobeSucceeded` on invalidate-alive, `noteReprobeInvalidatedEarly` when external `markAlive` beat the probe, `noteReprobeExhaustedWindow` on the 6-iteration exhaustion path. The grep-for-log-line problem is now a numeric check on `/diagnostic/dump` counter deltas.
- **`/diagnostic/dump` extended.** `DiagnosticSnapshot.aliveCache: AliveCacheCounters?` — `nil` when the runner opted out; JSON body omits the field to preserve wire compatibility.

### Wire

- `HealthResponse` fields were already declared in v1.0.2's wire crate but never populated — this release makes them non-zero.
- `DiagnosticSnapshot` gains optional `aliveCache` object; parsers ignoring unknown fields keep working.

### Infrastructure

- **`scripts/release/corpus-gate.sh`.** Runs every yaml under `SMIX_CORPUS_DIR` (defaults to a bootstrap-corpus fixture directory under `crates/smix-cli/tests/fixtures/`). Fails the release on any yaml failure. Dumps `smix diagnostic dump --json` on teardown into `.tmp/release-gate/<ts>/`.

### Tests

- `smix-runner-sources`: 7 tests (extract round-trip, version file write, xcframework-excluded regression guard, backup-on-force, refuse-on-non-empty, version-file read).
- `smix-cli::runner`: 3 auto-sync tests (extract on missing, re-extract on stale, no-op when current) + 1 env test (`TEST_RUNNER_SMIX_RUNNER_VERSION` set correctly).
- `smix-simctl::subprocess_ring`: 1 persist round-trip test simulating supervisor cycle.
- Swift `AppAliveCacheCountersTests`: 6 tests covering mutation counters + diagnostic JSON serialisation + null-cache omission.

### Ship-gate observations (real-sim, iOS 26.5)

Observations satisfying the RFC's real-sim gate:

1. `smix runner install` — extracted 303 files at v1.0.10, previous 2212-line SmixRunnerUITests.swift (pre-v1.0.3) → 2706-line v1.0.10; xcframework preserved from backup tree via the carry-over patch.
2. `GET /health` — `{"ok":true,"runnerVersion":"1.0.10","uptimeMs":16105,"lastRequestAtMs":0,"sessionsOpen":0,"activationsTotal":0}` — the field CHANGELOG v1.0.2 claimed but never emitted is now real.
3. `POST /session/open` (bundleId `com.apple.Preferences`, activate=false) — HTTP 200 + `{"sessionId":"6F7C4A73-…","activatedOnce":false,"serverTimeMs":1783746973931}`. **The chronic 404 that spanned v1.0.4-v1.0.9 is permanently closed.**
4. `POST /diagnostic/dump` — `aliveCache:{"markDeadTotal":0,"markAliveTotal":1,…}` — counters wire end-to-end (markAliveTotal:1 came from the /session/open handler's `cache.markAlive` per D2 §"successful open re-establishes the target").

The consumer app was not installed on the validation sim (unrelated to smix), so the corpus gate remains for a follow-up validation with a real consumer app installed. The systemic fix itself — the CLI-vs-runner drift closure — was observed working on real sim before publish.


## [1.0.9] — 2026-07-11

App-alive cache adaptive re-probe + supervisor RunnerCycled log context. Closes the two named v1.0.8 deferrals.

### Runner-side (Swift)

- **App-alive cache adaptive re-probe.** When an XCTIssue "Application X is not running" is observed, the cache still marks the bundle dead for 20 s. Now the runner spawns a background `Task` that polls `XCUIApplication.state` every 3 s during the window; on the first observation of a non-`.notRunning` state, calls `markAlive` immediately + emits `smix-runner: app-alive cache re-probe hit <bundle> state=<n>; early invalidate` on stderr. Fixes a reported failure mode where slow-bootstrap apps sat blocked for the full 20 s while they were actually alive again.
- Bounded to 6 iterations (18 s) — matches the cache window minus one probe interval for slack. If the app is still `.notRunning` after 6 probes the cache expires naturally.

### CLI (Rust)

- **Supervisor `RunnerCycled` event with log context.** The JSON emitted on every cycle now carries a `context` field with ±5 lines around the matched trigger:
  ```json
  {"event":"RunnerCycled","reasonMatched":"** TEST INTERRUPTED **","context":["2026-07-11 …", "…"],"atMs":1720689124321}
  ```
  Consumers get cycle-cascade classification data without needing a separate `grep` pass on the runner log. Best-effort — if the log rotated between the match and the read the `context` array comes back empty.

### Wire + ABI compatibility

- No wire changes.
- No SDK ABI changes.
- Runner behaviour change is invisible to consumers not observing stderr.
- Supervisor JSON gains a new optional `context` field; parsers ignoring unknown fields keep working.

### Deferred (still)

- **`launchApp: clearState: true` deprecation + auto-expand** — waiting on downstream corpora to migrate to `clearAppData` first. Once migration is confirmed, v1.0.10 will emit the WARN + auto-expand.



Eliminate the "app quit unexpectedly" ReportCrash system dialog that fired during in-place data clears.

### Root cause revisited

v1.0.4 replaced `simctl uninstall + install` with an in-place clear (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`). Dogfooding reported the dialog STILL fired. Diagnosis: even without the uninstall, `simctl terminate` sends SIGKILL to the target, which `com.apple.ReportCrash` on iOS 26.5 sim treats as a crash. The whole `simctl` termination pathway is what triggers the dialog — not just the uninstall.

The systemic answer: move termination + launch INSIDE the XCUITest runner process via `XCUIApplication.terminate()` / `.launch()` (cooperative via `testmanagerd`; does NOT signal ReportCrash). The sandbox wipe stays on the host via `SimctlClient::clear_app_sandbox` but ONLY after the cooperative terminate, so ReportCrash was never signalled.

### Runner-side (Swift)

- **`POST /session/terminate-app { sessionId }`** → cooperative `XCUIApplication.terminate()` on the session's cached binding. testmanagerd stop; no SIGKILL; no ReportCrash signal.
- **`POST /session/launch-app { sessionId }`** → cooperative `XCUIApplication.launch()`. Fresh instance sees whatever sandbox state the SDK left for it.
- Both are additive routes; v1.0.7 runners return 404 and consumers should either upgrade the runner or route through the legacy `Session::relaunch_app`.

### CLI + adapter (Rust)

- **New yaml verb `clearAppData`** — session-scoped in-place data clear. Bare verb, no args. Maps to `App::clear_app_data` which orchestrates the 3 steps host-side. Requires an open session (auto-populated by `smix run`).
- **`App::clear_app_data() → Result<wall_ms>`** on the Rust SDK. Grabs `session_id` + `bundle_id` from the driver + `udid` from `App::require_udid`; calls `runner.terminate_session_app` → `simctl.clear_app_sandbox` → `runner.launch_session_app`.
- **`Session::reset_app_data()`** — thin ergonomic wrapper on `App::clear_app_data`, for consumers who hold a `Session` handle directly.
- **`launchApp: clearState: true` NOT yet deprecated** in this cycle — legacy shape still runs the pre-v1.0.8 `LaunchFreshOp` sequence. Consumers migrating to `clearAppData` get the crash-dialog fix; consumers who keep the legacy shape stay unaffected until v1.0.9 flips the default.

### Wire additions

- `SessionAppLifecycleRequest` / `SessionAppLifecycleResponse` in `smix-runner-wire`.
- `HttpRunnerClient::terminate_session_app(req)` / `launch_session_app(req)` on the Rust client.

### Deferred to v1.0.9

- **Adaptive app-alive cache re-probe** (parked because the crash-dialog fix is enough to unblock downstream gates and the a11y-cache work has its own testing surface).
- **Supervisor `RunnerCycled` reason with log context.**
- **Deprecation of `launchApp: clearState: true`** — emit WARN + auto-expand to `clearAppData + launchApp: {}`. Deferred because the deprecation needs a full-corpus consumer migration, and consumers should migrate their subflows first on their own timeline.

### Wire + ABI compatibility

- Additive routes; v1.0.7 runners return 404 on the new endpoints.
- Additive `Step::ClearAppData` variant on the yaml Step enum; `#[non_exhaustive]` was already in play (via yaml deserialization), so consumers using pattern matching are unaffected.



Systemic observability + subprocess integrity. Three reported symptoms shared one root cause: smix was opaque about its own runtime.

### Subprocess integrity

- **`SimctlClient::clear_app_sandbox` uses `/bin/rm`** (not `"rm"`). `xcrun simctl spawn <UDID> <cmd>` uses `posix_spawn` inside the sim; PATH resolution is NOT run, so a bare command name fails `NSPOSIXErrorDomain code 2: No such file or directory` on iOS 17+ sims. This was the direct root cause of a reported ENOENT failure on `launchApp: clearState: true` mid-flow. `current_locale` + `set_locale` similarly use `/usr/bin/defaults`.
- **`SimctlError::NonZeroExit` extended with `argv: Vec<String>` + `wall_ms: u64`**. Display impl now surfaces every arg simctl was asked to run — `xcrun simctl spawn <UDID> /bin/rm -rf /Users/.../Documents ... exited 2 (312ms): ...` — instead of just the subcommand name. Consumers reading the error know exactly what smix asked simctl to do.
- `SimctlError` marked `#[non_exhaustive]`; `SimctlError::non_zero_exit(sub, code, stderr)` helper for callers translating foreign errors.

### Observability surface

- **Ring buffer of recent `simctl` invocations** (capped 128; oldest evicted). Public accessor `smix_simctl::recent_subprocesses() -> Vec<SubprocessRecord>` — `argv`, `exit_code`, `wall_ms`, `stderr_head` (first 256 bytes), `timestamp`.
- **`POST /diagnostic/dump`** runner-side route — snapshot of `{ sessions, simHealth, supervisorPid, uptimeMs, recentSubprocesses }`.
- **`smix diagnostic dump [--json]`** CLI verb — calls `/diagnostic/dump` on the runner, merges with the client-side ring, pretty-prints a runtime post-mortem view. `--json` for CI consumption. Legacy runners (v1.0.6-) return 404; CLI degrades gracefully to client-side ring only.
- `HttpRunnerClient::diagnostic_dump()` Rust client method.

### Streaming discipline

- **`smix runner supervise` flushes stdout after every `RunnerCycled` JSON event**. Supervisor events now reach the consumer's parser even when the outer flow crashes fast right after a cycle.

### Cold-rebuild progress banner

- **`smix runner up` prints an explicit cold vs warm banner**. Detects warm by checking `.smix/runner/derived-data-<UDID>/` presence + populated. Cold path prints `COLD REBUILD expected up to 10 minutes` and emits a `xcodebuild still working (Ns elapsed)` heartbeat every 30 s. Warm path prints `warm rebuild ~3 s expected`. Fixes the reported case where a consumer harness's `spawnSync` timeout (300 s) tripped during cold recompile after a version bump with no visible progress signal.

### Related regression fix

- `smix-sdk/tests/launch_fresh_plan.rs` was pre-v1.0.4; asserted `Uninstall+Install` on the default clear_state path. v1.0.4 flipped the default to in-place (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`); tests updated to match shipping behaviour. Force-reinstall path exercised via `plan_launch_fresh_calls_v2(true)`.

### Wire + ABI compatibility

- All wire additions additive. `POST /diagnostic/dump` on runners < v1.0.7 returns 404; CLI degrades gracefully.
- `SimctlError` is `#[non_exhaustive]`; construction sites updated to fill new fields via `non_zero_exit` helper.



Sidecar supervise + symmetric down-cascade + rust 1.97 baseline. Follow-up to v1.0.5 folding the supervisor's spawn-and-teardown into the runner lifecycle so consumers who want automatic `TEST INTERRUPTED` recovery just add `--supervise` to their existing `smix runner up`.

### CLI (Rust)

- **`smix runner up --supervise`** — after `/health` returns 200, spawn a detached `smix runner supervise` process, redirect stdout/stderr to `.smix/runner/supervise-<UDID>.log`, and record its pid in `state.json` under a new `supervisorPid` field. Sidecar runs in its own process group so a ctrl-C on the CLI doesn't tear it down.
- **`smix runner down` cascades supervisor teardown.** Before the xcodebuild SIGINT, `down` reads `state.json` and if a `supervisorPid` is present + still matches a `smix runner supervise` process, sends SIGTERM (5 s), escalates to SIGKILL if needed. `down` invoked from inside the supervisor itself (re-entrant case, during auto-cycle) skips the self-kill.
- **`smix runner cycle` preserves the sidecar flag.** If the pre-cycle `state.json` records a supervisor, the post-cycle `up` re-attaches one. Consumers who ran `up --supervise` get supervision back automatically after a cycle.

### Runner state schema (backward-compatible)

- `state.json` gains optional `supervisorPid: u32` field via `#[serde(default)]`. State files written by v1.0.5 or earlier deserialize without change.

### Workspace hygiene

- `rust-version = "1.97"` in the workspace `Cargo.toml`. Baseline bump for the `if let` chain stabilizations + std ergonomics. Consumers on `cargo install` see no change (prebuilt binary); consumers building from source now need rustc 1.97+.

### Documentation

- CHANGELOG format going forward groups entries under `### CLI (Rust)`, `### Runner-side (Swift)`, `### SDK — all four`, `### Documentation`, `### Deferred`. First entry using the new pattern; retroactive edit of v1.0.4/v1.0.5 not required.

### Deferred (v1.0.7+)

- **Opportunistic 1.97 idiom cleanups.** A handful of nested `if let` sites collapse under 1.97's chain stabilizations. Not a functional change; queued as a hygiene sweep for a slow release cycle.

### Wire + ABI compatibility

- No wire additions.
- No SDK ABI additions.
- CLI additions are opt-in via `--supervise`; the classic path is unchanged.



Session persistence across XCTest lifecycle, host-side XCTest supervisor daemon, runner idle-close sweep, and the release smoke gate script. Closes the three v1.0.4 deferrals + the "shipped on build-green only" gap.

### Added — session persistence

- **`POST /session/list`** → `{sessions: [{sessionId, bundleId, openedAtMs, lastActivatedAtMs}]}`. Rust: `HttpRunnerClient::list_sessions()`. CLI: `smix runner list-sessions` (pretty-printed table).
- **`Session::still_valid()` on all 4 SDKs** — probes `/session/list` and returns `true` iff the runner still knows this session id. Consumers wire it after a `Session::state` transition to `Cycling` or `Dead` to decide whether to keep using the session (§D1 preserves them across cycles) or reopen.
- **Runner-side persistence** — session table serializes to `~/Documents/smix-sessions.json` inside the sim on every mutation via `Data.write(.atomic)` (atomic-rename write). Boot rehydrates whatever's there, rebuilding each `XCUIApplication(bundleIdentifier:)` fresh (no `.activate()` call — the client's next request drives that). `smix runner cycle` preserves the file, so consumer `Session-Id` survives the cycle transparently.

### Added — supervisor daemon

- **`smix runner supervise [--runner-project <path>]`** — foreground process that tails `.smix/runner/runner-<UDID>.log`, matches interrupt patterns (`** TEST INTERRUPTED **`, `SchemeActionResultOperation started unexpectedly`), and auto-invokes `runner::cycle()` on hit. Backoff: 60 s per-cycle cooldown. Circuit breaker: 5 cycles in 10 minutes → exit non-zero so a monitoring layer can escalate. Emits `{"event":"RunnerCycled","reasonMatched":"...","atMs":N}` JSON on stdout per cycle.

### Added — idle-close sweep

- **Runner-side session idle-close** — `SessionEntry` gains `lastAccessedAt`; `resolveApp()` refreshes it on every `Session-Id` hit. Detached `Task.detached` in `test_runForever` reaps sessions whose `lastAccessedAt` is older than 60 s every 15 s. Half-orphaned client sessions (SIGKILL wipes client without close) vanish within 60-75 s instead of accumulating until runner restart. Emits a stderr line on non-zero reap for operator visibility.

### Added — release smoke gate + ship script

- **`scripts/release/smoke-v1.smoke.sh` + `.smoke.yaml`** — real-sim gate exercising every net-new v1.0.4/v1.0.5 code path: pacer floor (`takeScreenshot × 10`), `--debug-output` `fail.tree.json` emit on a deliberate `assertVisible` fail, `runner cycle` + `/session/list` persistence, supervisor 5 s alive check. Requires jq + a booted sim.
- **`scripts/release/ship.sh <version> [--i-know-what-im-doing]`** — DAG-ordered 4-ecosystem publisher, refuses to run unless the smoke gate has passed in the last hour. Bypass flag is an audit-visible knob, not a silent default.

### Wire + ABI compatibility

- All additions are additive (routes, response fields, CLI verbs).
- v1.0.5 clients work against v1.0.4 runners (missing `/session/list` → 404; SDK `Session::still_valid()` propagates the error and consumers treat as invalid).
- v1.0.4 clients keep working against v1.0.5 runners.



Studio protection + gate-hardening. Motivation: a downstream gate loop running against a v1.0.3 runner triggered a `SimRenderServer` `brk 1` assertion inside the `com.apple.display.captureservice` dispatch queue, cascading into shutdown_stall and forced macOS restarts. This release closes the full gate-hardening feedback plus the SimRenderServer stress fix, plus lifecycle-safe-exit primitives.

### Added — sense layer

- **`smix-sim-health` — new stone crate.** Watches SimRenderServer + xcodebuild pids + `/health` age + rolling screenshot wall times. State machine `Healthy | Degraded | Dead`; transitions broadcast on a `tokio::sync::broadcast` channel. Business-unaware; SDK-facing state is exposed via `Session::state` (below), driver-side auto-cycle policies live per driver.
- **`HttpRunnerClient::with_sim_health(monitor)`** — `/health` outcomes feed `SimHealthMonitor::record_health_ok`/`record_health_fail`. `HttpRunnerClient::sim_health()` accessor.

### Added — act layer

- **`smix-simctl` screenshot pacer.** Adaptive interval floor: 100 ms in the fast path (recent wall < 800 ms), 1500 ms in the slow path (recent wall ≥ 800 ms). Circuit breaker: any recent wall ≥ 1500 ms or any failure trips a 3 s hold that surfaces the new typed error `SimctlError::CaptureBackpressure { retry_after }`. Consumers whose gates already screenshot at ≥ 200 ms cadence are unaffected; tight loops slow to the pacer floor. This is the direct fix for the `SimRenderServer` `brk 1` triggering pattern on iOS 26.5.2 (25F84).
- **`SimctlClient::with_screenshot_pacer(cfg)`** builder + **`SimctlClient::with_sim_health(monitor)`** builder — wire the pacer's observations back to the sim-health monitor for global state classification.

### Added — CLI

- **`smix runner cycle`** — new verb. Reads the current runner state, tears down (SIGINT + wait, preserves per-udid `derived-data-<udid>/`), brings up on the same device + port + bundle. Warm re-up in ~3 s vs cold ~15 s. Errors if no `state.json` exists (`runner up` for a cold start).
- **`smix runner up` bundle validation** — refuses to boot without `--bundle`, prints a clear error + example. `SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1` bypasses the guard (opts back into the legacy Preferences default with an explicit warning). With `--bundle` set, logs `[runner] target bundle-id: <id>` at boot.
- **`smix run --gate-signal <regex>` + `--gate-signal-timeout <ms>`** — prepends an implicit `expect.signal { regex, timeoutMs }` step at the START of the flow (index 0), blocking until the regex is observed in the metro log tail. Requires `--metro-log-url` also set. Symmetric to the existing `--await-signal` end-of-flow gate. Default timeout 60 s; zero disables. Replaces the node-side wait-for-metro-signal helper consumers had to write.

### Added — debug output

- **`--debug-output <dir>/step-<N>-<verb>.tree.json`** — on step failure, alongside the fail PNG the adapter now writes a full a11y-tree snapshot captured at the moment the step's expectation was evaluated. Turns "screenshot shows the text but assertVisible failed" mysteries into "here's exactly what the runner saw."
- **`run-summary.json` per-step trace** — the summary now carries `steps: [{n, verb, verdict, wallMs, jsonPath, pngPath?, treePath?, failureKind?, failureMessage?}]`. Populated for both success and failure runs (partial trace on failure preserved via a snapshot taken before the `?`-return early-exit).

### Added — session lifecycle

- **`POST /session/close-all`** — closes every open session on the runner. Idempotent (`{ok, closed:N}`). Rust: `HttpRunnerClient::close_all_sessions()`.
- **`POST /session/relaunch-app {sessionId}`** — runner does `terminate() + launch()` on the session's cached `XCUIApplication` binding IN PLACE, preserving the session id and XCUITest binding. Returns `{ok, wallMs}`. Recovers from a downstream app crash without cycling the runner. Rust: `HttpRunnerClient::relaunch_session_app(&req)`; SDK: `Session::relaunch_app()` (Rust), `session.relaunchApp()` (TS / Swift / Kotlin).
- **`Session::state` + state stream/flow/event across all 4 SDKs.** The runner emits `X-Sim-Health: healthy|degraded|cycling|dead` on every response; SDKs parse it and surface transitions to consumers:
  - Rust — `Session::state() -> SessionState`.
  - TypeScript — `session.state` + `session.on('state', listener)`.
  - Swift — `session.state` + `session.stateStream: AsyncStream<SessionState>`.
  - Kotlin — `session.state` + `session.stateFlow: StateFlow<SessionState>`.

### Added — extended health

- **Extended `GET /health` body** now includes `simRenderServer: {alive, pid}` and `xcodebuildTestHost: {alive, pid, restartCount}`. Legacy clients that only read `{ok:true}` continue to work.

### Added — safe-exit cascade

- **`smix run` SIGINT / SIGTERM handling.** `tokio::signal::ctrl_c()` and SIGTERM race against the flow execution; on signal the CLI aborts the in-flight flow, runs a best-effort `/session/close` under a 2 s timeout, prints `interrupted (SIGINT|SIGTERM) — running session-close cascade` on stderr, and exits with POSIX-conventional 130 (SIGINT) / 143 (SIGTERM). The Rust adapter's `--debug-output` partial-trace file still fires on interrupt so the last-attempted step is captured. Solves the "ctrl-C leaves a session hanging until runner idle-close fires" complaint.

### Fixed — `openLink` URL preservation

- **`SimctlClient::open_url` argv preservation** — verified byte-identical URL passthrough (`openurl_argv` test helper + 3 unit tests covering percent-encoded schemes, query params with `&`/`#`, unicode). The dev-launcher picker behavior reported on `expo-dev-client 57.0.5` is upstream (not smix); the finding lives on expo-dev-client's side and is documented for the record.

### Documented — `--activate` per-request cost auto-resolution

- **`--activate` per-request cost** is auto-resolved for consumers who upgrade to v1.0.3 sessions (via `smix run` auto-session or explicit `Session.open`). The runner short-circuits `App-Activate: true` when a `Session-Id` header is present, so the 50-100 ms per-request main-actor hop described in earlier feedback no longer applies for session-mode flows. No code change needed; documented here so consumers know to prefer `--activate` inside a session rather than passing it per-request.

### Wire + ABI compatibility

- All additions are additive (routes, response fields, enum variants, SDK types).
- v1.0.4 clients work against v1.0.3 runners (missing routes → 404 → fall through; missing headers → `Session::state` stays `Healthy`).
- v1.0.3 clients work against v1.0.4 runners (extra fields / headers ignored).

### Verified builds

- Rust workspace (26 crates): fresh `cargo check --workspace --jobs 1` clean 3m06s.
- Swift Package: `swift build` clean; `xcodebuild build-for-testing -project SmixRunner.xcodeproj -scheme SmixRunner -destination 'generic/platform=iOS Simulator'` — `** TEST BUILD SUCCEEDED **`.
- Kotlin: `./gradlew :sdk:build` — BUILD SUCCESSFUL in 28s.
- TypeScript: `tsc --noEmit` clean.

### Deferred to v1.0.5 (independent charters)

- **Session-persistence across XCTest lifecycle.** Needs a separate design for state serialization.
- **Host-side XCTest supervisor** — auto-cycle-on-`TEST INTERRUPTED`. v1.0.4 provides the manual escape hatch (`smix runner cycle` verb) plus the programmatic detection surface (`Session::state` transitions via `X-Sim-Health` + `AppAliveCache` markDead from parsed XCTIssues); a fully-automatic supervisor daemon is v1.0.5 material.
- **Runner-side idle-close 120 s → 60 s tightening** — deferred; the client-side `smix run` SIGINT / SIGTERM cascade already covers the primary orphaned-session case.



Session lifecycle at the runner boundary. Building on v1.0.2's rate-limited activation, v1.0.3 lets consumers open a session at the start of a flow, run the entire flow against a cached `XCUIApplication` binding, and close on exit — no per-request activation. This is the systemic fix that supersedes the interim rate-limit; the legacy per-request path stays as a fallback.

### Added

- **Session routes on the iOS runner** — `POST /session/open {bundleId, activate}` returns `{sessionId, activatedOnce, serverTimeMs}`; `POST /session/close {sessionId}` (idempotent) returns `{ok}`; `POST /session/renew-activation {sessionId}` returns `{ok, activated}` subject to a 2 s per-session rate limit. Wire types available on `smix-runner-wire` since v1.0.2; runner-side handlers land in v1.0.3.
- **`Session-Id` header** on every runner request. When present, `resolveApp()` short-circuits to the session's cached binding — no per-request activation regardless of `App-Activate`.
- **Rust SDK `Session`** — `App::open_session(bundle_id, activate) -> Session`. Consumer flow: `let session = app.open_session("com.example.app", true).await?; session.app().tap(...).await?; session.close().await?;`. `Session::renew_activation()` for consumer-driven drift recovery.
- **TypeScript SDK `Session`** — `Session.open(runner, "com.example.app", { activate: true })` on any `HttpRunnerClient`-shaped runtime. Consumers pair with `try / finally { await session.close() }`.
- **Swift SDK `HttpSmixSimRuntime` + `Session`** — URLSession-backed `SmixSimRuntime` implementation speaking the SmixRunnerCore wire directly, with session-aware header attachment. `Session.open(runtime, activate: true)` acquires a session; `session.close()` releases. Every request from the runtime while the session is open carries `Session-Id`.
- **Kotlin SDK `HttpSmixSimRuntime` + `Session`** — java.net.HttpURLConnection-backed runtime (no additional dependencies beyond the existing kotlinx-serialization-json), same wire contract. `Session.open(runtime, activate = true)` / `session.close()`. Thread-safe on the session-id field via `AtomicReference`.
- **`smix run` opens a session automatically** — every CLI invocation opens a session at start, closes on exit. Runners that don't implement `/session/open` (v1.0.x pre-1.0.3) return non-2xx; the CLI emits a WARN and falls through to the legacy per-request path (rate-limited since v1.0.2, so still safe).

### Wire + ABI compatibility

- All new routes are additive
- All new SDK types are additive (`Session`, `SessionOpenRequest`, etc.)
- v1.0.x clients keep working against v1.0.3 runners (Session-Id header optional)
- v1.0.3 clients work against v1.0.2 runners with a WARN + legacy fallback

## [1.0.2] — 2026-07-09

### Fixed

- **Runner activation storm** — the XCUITest-side `resolveApp()` no longer calls `.activate()` on every request when `App-Activate: true` is set. Instead, `.activate()` runs at most once per bundle-id per 5 s. Long-running gates (visual / perf regression, ~340 s of continuous requests against the runner) previously accumulated ~1000+ activate calls, exhausting XCTest process arbitration on iOS 26.5+ and crashing `test_runForever()` mid-run. Recovery semantics preserved: after 5 s of silence a subsequent activate hint is honored, so a foreground steal by SpringBoard is auto-recovered within the same window.
- **Simulator screenshot PNG colorspace metadata** — `xcrun simctl io <udid> screenshot` on iOS 26.5 sub-builds started omitting the `sRGB` ancillary chunk from its PNG output. macOS Preview.app and other viewers fall back to Display P3 in the absence of an embedded ICC profile, over-saturating red and adding yellow anti-alias fringing on text. `SimctlClient::screenshot` now byte-splices a synthesized `sRGB` chunk (rendering intent = 0, perceptual) into the PNG stream immediately before the first IDAT when none is present. IDAT bytes are never decoded or modified — pixel-comparison consumers (dhash, hamming) see byte-identical decoded pixel arrays.

### Added

- **Runner liveness observability** (Rust client) — `HttpRunnerClient::with_liveness_window(N)` opts in to rolling-window request outcome tracking. If a majority of the last N requests failed, subsequent calls surface `RunnerTransportError::RunnerDegraded { window, non_success_recent, last_endpoint, last_error }` instead of returning silent stale bodies. Any transport-level `is_connect()` error additionally probes `/health` with a 1 s timeout; if the runner is unreachable, subsequent calls surface `RunnerTransportError::RunnerDied { last_seen_ms, last_error }`.
- **Extended `GET /health` body** — the runner-side JSON response now includes `runnerVersion`, `uptimeMs`, `lastRequestAtMs`, `sessionsOpen`, and `activationsTotal`. Legacy clients that jq-parse `{"ok":true}` continue to work — the extended body is a superset. The Rust client's `HttpRunnerClient::health_detail()` parses the new fields.
- **Wire types for session lifecycle** — `smix-runner-wire` exports `SessionOpenRequest / SessionOpenResponse / SessionCloseRequest / SessionCloseResponse / SessionRenewActivationRequest / SessionRenewActivationResponse`. The Rust client (`HttpRunnerClient::open_session`, `close_session`, `renew_session_activation`) can drive these when a runner implements the endpoints; the corresponding runner-side routes are queued for v1.0.3.

## [1.0.1] — 2026-07-09

### Fixed

- **Parser** — `smix run` now accepts the `expect: { visible: <selector>, timeoutMs?: N }` and `expect: { notVisible: <selector>, timeoutMs?: N }` shapes emitted by `smix migrate` for `extendedWaitUntil`. The `expect: { visible: ... }` shorthand (no timeout, equivalent to `assertVisible`) is likewise accepted. Previously the parser only recognized the top-level `expect: { text | id: ... }` maestro-alias form, so codemodded corpora failed at run time with `expected 'text' or 'id' key`. Regression tests in `smix-adapter-maestro/tests/parser.rs` pin every accepted shape.
- **`smix migrate --help`** — help text corrected to state that comments, copyright headers, and blank lines survive the codemod byte-identical (matches 1.0.0's actual behavior).

### Added

- **`smix run --check`** — parse-only pre-flight. Reads every listed flow YAML and reports parse or include errors without connecting to a runner or booting a simulator. Exit 0 on clean parse across every flow; non-zero (2) on any error. Suitable for CI without simulator infrastructure.

## [1.0.0] — 2026-07-08

First public release.

### Added

- **CLI** — `smix` binary with subcommands `run`, `sim`, `runner`, `migrate`, `annotate`, `authoring`, `tree`, `find`, `tap`, `fill`, `clear`, `scroll`, `screenshot`, `describe`, `doctor`.
- **Rust SDK** — `smix-sdk` crate exposing the `App`, `Selector`, `KeyName`, and `Runtime` types plus a fluent builder for connection configuration.
- **TypeScript SDK** — `@goliapkg/smix` on npm; Playwright-shape API surface mirrored to the Rust SDK.
- **Swift SDK** — Swift Package published as a GitHub Release; provides a prebuilt `SmixCoreFFI.xcframework`.
- **Kotlin SDK** — `jp.golia.smix:smix-sdk` on Maven Central; UiAutomator-backed runner for the Android Emulator.
- **YAML runtime** — Maestro-compatible YAML syntax accepted directly (both maestro-canonical `tapOn` and smix-canonical `tap` forms).
- **Codemod** — `smix migrate` rewrites YAML from maestro-canonical to smix-canonical while preserving comments, copyright headers, and blank lines byte-identical.
- **Fixture registry** — `--fixture-registry <file.ts|file.json>` enables the `- fixture: <id>` YAML verb.
- **Metro log signals** — `expect.signal`, `expect.signals`, `expectLogClean`, and the `--metro-log-url ws:// | file:// | -` transport with configurable allowlists.
- **Annotation** — bundled Inter Regular and Noto Sans SC fonts; the `takeScreenshot` verb accepts `annotate:` clauses composing `circle`, `line`, `arrow`, `text`, and `box` primitives; `smix annotate` standalone CLI.
- **Auto-annotate on failure** — `--debug-output` fail-step PNGs receive an automatic red circle, step label, and summary; opt out with `--no-fail-annotate`.
- **JUnit output** — `smix run --format junit --output report.xml` writes a JUnit-XML testsuite consumable by common CI pipelines.
- **Authoring tier** — `smix authoring suggest`, `capture-tree`, `diff-tree`, and `record` for authoring flows against a live simulator or emulator.
- **Standard subflows** — bundled `std/wipe-app-state.yaml`, `std/wait-metro-bundle.yaml`, `std/quit-qa-mode.yaml`, `std/dismiss-open-in.yaml`, and `std/ensure-locale.yaml`.
- **MCP server** — `smix mcp` subcommand exposes the SDK surface to Claude Code and other MCP-aware clients.

### Stability commitments

- Wire format frozen — any breaking wire change is a v2.0 release.
- ABI frozen for the ten core "stone" crates (`smix-error`, `smix-selector`, `smix-screen`, `smix-runner-wire`, `smix-input`, `smix-verbs`, `smix-metro-log`, `smix-fixture`, `smix-annotate`, `smix-migrate`) — additive changes only within v1.x.
- All CLI flags shipped in v1.0 remain accepted for the v1.x lifetime.
- The YAML verb table (`smix-verbs`) is the single source of truth; removing a verb is a major-version change.

See [`docs/ai-guide/wire-format.md`](./docs/ai-guide/wire-format.md) and [`docs/ai-guide/abi-stability.md`](./docs/ai-guide/abi-stability.md) for the full contracts.
