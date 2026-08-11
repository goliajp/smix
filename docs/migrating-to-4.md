# Migrating to smix 4.0

Device records moved. Where a simulator's UDID lives, who is recorded as
holding it, and which port a runner has on it are now facts about **the
machine**, not about the checkout you happen to be standing in.

If you drive smix by hand or from YAML flows, there is **nothing to
undo** — run `smix sim migrate` and `smix lease migrate` once and carry
on. Everything below is for code that calls the Rust crates.

---

## Why it moved

A simulator is an operating-system object. Its UDID, its runtime version,
whether it is booted and who booted it do not change when you `cd`.

They were stored in whichever `.smix/` sat above the working directory,
so a machine with four checkouts held four answers about the same
simulators. Measured on one machine: a runner was found holding port
22087 with no record of it. The rule is to find a runner's owner before
touching it; the check came back empty, so it could neither be confirmed
an orphan nor stopped. It was on the books the whole time — in another
workspace's books.

---

## 1. Your existing records keep working, once

Nothing is lost and nothing is moved out from under you. The old
locations are still read; the migrations copy and never remove.

```bash
smix sim migrate --dry-run      # say what would move
smix sim migrate                # move it
smix lease migrate              # the same, for who-holds-what
```

Run them from each checkout that has a `.smix/`, or name the trees with
`--from <DIR>`. Running either twice does nothing the second time, so
"run it again if you are not sure" is safe advice.

Until you do, a device only one checkout knows about is named as such
every time you list devices — no other tree can see it.

To check where you stand:

```bash
smix sim list --registered      # every recorded device, and whose book it is in
smix runner list                # every runner on this machine, and who knows about it
```

---

## 2. `smix_lease::store` takes a `LeaseDir`, not a path

**Before**: every ledger function took a workspace root and appended
`.smix/leases` to it itself.

```rust
smix_lease::store::write(&workspace_root, &lease)?;
let lease = smix_lease::store::read(&workspace_root, udid)?;
```

**Now**: they take the ledger directory as a type.

```rust
let leases = smix_lease::store::LeaseDir::machine()
    .ok_or("no HOME or XDG_DATA_HOME")?;
smix_lease::store::write(&leases, &lease)?;
let lease = smix_lease::store::read(&leases, udid)?;
```

`LeaseDir::machine()` is the answer you want in a program. `LeaseDir::at`
exists for tests, where a temporary directory stands in for a whole
machine.

This is a type rather than a path on purpose. When the first argument
changed meaning, twenty-five call sites inside smix went on compiling and
went on writing device facts into checkouts — nothing but a type could
have caught that, and nothing but a type can catch it in your code
either.

`store::lease_dir(root)` is gone. It existed to build `.smix/leases` from
a workspace root, which is the thing that no longer happens.

Reading a checkout's old book — to report a divergence, never to act on
one — is `CheckoutLedgers`:

```rust
use smix_lease::store::CheckoutLedgers;
if let Some(book) = CheckoutLedgers::discover(&cwd) {
    for id in book.device_ids() { /* book.read(&id)? */ }
}
```

It has no write path, deliberately: what a checkout holds was written by
whichever smix that tree last ran.

---

## 3. Two SDK calls take the ledger directory

`Leased::acquire` and `App::hold_device_lease` each gained one argument,
in the same position and for the same reason: the tree they were given
used to mean two things — where the ledger is, and where a dead holder's
build products would be settled. Only the second is still a tree.

```rust
// before
Leased::acquire(&control, &workspace_root, udid, &executor)?;
app.hold_device_lease(&workspace_root, udid, &reconciler)?;

// now
let leases = smix_lease::store::LeaseDir::machine().unwrap();
Leased::acquire(&control, &workspace_root, &leases, udid, &executor)?;
app.hold_device_lease(&workspace_root, &leases, udid, &reconciler)?;
```

---

## 4. Two things smix now refuses that it used to do

Neither needs a code change. Both change what happens on a machine
somebody else is also using, so they are worth knowing.

**A live holder is no longer reclaimed for going quiet.** The heartbeat
is written when the ledger is touched, so a holder that takes a device
and then serves requests for hours is silent by design. Ninety seconds of
silence used to make its lease reclaimable; a long-lived MCP session was
exactly that shape. `smix lease reconcile` now reports it as held.
`StaleReason::HeartbeatExpired` remains as a name and is never produced.

If you relied on that to clear a genuinely wedged holder, end the process
— which you can establish and smix cannot — and the ordinary
holder-is-gone path settles it.

**`smix down` leaves a live holder alone.** It used to close every ledger
it found, which was right while the ledgers were per checkout. The
directory is the machine's now, and it holds other people's sessions. It
closes what is no longer held, or what this process holds, and names the
rest.

---

## 5. Nothing falls back to the working directory any more

A device record used to fall back to `.smix/sims.json` beside you when
the machine location could not be resolved. It now says so and stops.
There is no good place to put such a record silently — one written where
nobody else reads it is the failure this release is about.

Set `SMIX_MACHINE_DIR` if you need to point smix's machine data
somewhere else; it moves all of it together.
