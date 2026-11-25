//! The `smix lease` subcommands, as the command line takes them.

use std::path::PathBuf;

#[derive(clap::Subcommand, Debug)]
pub enum LeaseAction {
    /// Every device with a ledger, and whether its holder is still there.
    List,
    /// One device: the holder, what is open, and the verdict. With
    /// `--scope` instead, a scope's holder and queue (see `acquire`).
    Status {
        /// The device: an alias, a UDID or a serial.
        #[arg(required_unless_present = "scope", conflicts_with = "scope")]
        device: Option<String>,
        /// A scope instead of a device; `--scope` with no name lists every
        /// scope on this machine.
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        scope: Option<String>,
        /// Print the device's ledger as JSON — `{"device", "path", "lease",
        /// "heldBy"}`: the file it lives in, the lease as it is stored (null
        /// when there is none), and who holds the device now — `{"pid",
        /// "cmd", "alive"}`, or null when a new claim would be granted —
        /// for a script to read instead of building the ledger's path or
        /// judging the holder itself.
        ///
        /// With `--scope NAME`, the scope: `{"scope", "devices",
        /// "requireHolder", "holder", "queue", "released"}`, where `holder`
        /// is `{"holder", "since", "lastSeen", "idleSecs", "until", "pid"}`
        /// or null. With a bare `--scope`, an array of these.
        #[arg(long)]
        json: bool,
    },
    /// Close what a dead holder left open, by the graceful path it never
    /// got to take. A live holder is reported, never preempted.
    Reconcile { device: String },
    /// Answer for a device smix did not boot, so smix may drive it (not
    /// shut it down). Logged with its reason; refused over a live holder.
    Claim {
        /// The device to take on, by UDID, serial or registry alias.
        device: String,
        /// Why this device is taken on; written to the ledger and the log.
        #[arg(long)]
        reason: String,
    },
    /// Give up a claim made by `lease claim`. With `--scope` and
    /// `--holder` instead, give up a scope or leave its queue.
    ///
    /// Removes only the claim. A device this machine really did boot
    /// keeps saying so, because that row answers a different question.
    Release {
        #[arg(required_unless_present = "scope", conflicts_with = "scope")]
        device: Option<String>,
        /// The scope to give up.
        #[arg(long, requires = "holder")]
        scope: Option<String>,
        /// Who is giving it up.
        #[arg(long)]
        holder: Option<String>,
    },
    /// Take a scope: a group of devices one holder at a time, in the order
    /// they asked.
    ///
    /// Exits 0 holding it. Held by somebody else: exits 75 at once naming
    /// them, or with `--wait` queues and returns when it is this holder's
    /// turn. Asking again as the same holder keeps the place it has.
    ///
    /// The hold lasts while the holder keeps working: every device command
    /// run with `SMIX_LEASE_HOLDER=<holder>` renews it, `lease renew` does
    /// for a step that does not touch a device, and a hold not renewed
    /// within `--idle` minutes is released and the next in line gets it.
    /// While it is held, a device command on one of the scope's devices
    /// run by anyone else is refused.
    Acquire(crate::scope_lease::AcquireArgs),
    /// Renew a scope this holder has, for a long step that runs no device
    /// command (a build). Exits 3 when it does not hold it.
    Renew {
        #[arg(long)]
        scope: String,
        #[arg(long)]
        holder: String,
        /// Replace when the holder expects to be done, as for `acquire`;
        /// `+30m tests` counts from now.
        #[arg(long, value_name = "WHEN")]
        until: Option<String>,
    },
    /// Say which devices a scope covers. Names are compared as devices:
    /// a registry alias covers the serial or UDID it runs as. Declaring it
    /// again replaces the devices and `--require-holder`, and leaves the
    /// holder and queue alone.
    Scope {
        /// The scope's name.
        name: String,
        /// A device it covers; repeat for each. Replaces the list.
        #[arg(
            long = "device",
            required_unless_present = "remove",
            conflicts_with = "remove"
        )]
        devices: Vec<String>,
        /// Refuse device commands on it while nobody holds it, too.
        #[arg(long, conflicts_with = "remove")]
        require_holder: bool,
        /// Delete the scope. Exits 3 while somebody holds or waits for it.
        #[arg(long)]
        remove: bool,
    },
    /// Who answers for this device, in one line and an exit code.
    ///
    /// `0` means a ledger says smix booted it, and names the session;
    /// `3` means no ledger says that — somebody else turned it on, or
    /// nobody wrote it down; `1` means the question could not be asked.
    /// Three codes rather than two: a check that answers "safe" when it
    /// means "I do not know" is the shape this cycle keeps finding.
    ///
    /// A claim answers `0` as well, and says so: the same entitlement to
    /// drive, deliberately not to shut down.
    ///
    /// Not a teardown permission: it answers "did smix boot this", not
    /// "did *you*" — which task a device is for is its owner, in
    /// `smix status`.
    Owner { device: String },
    /// Fold per-checkout ledgers into this machine's.
    ///
    /// Ledgers used to live in whichever `.smix/` was above the working
    /// directory. Adds and never removes; running it twice does nothing
    /// the second time.
    Migrate {
        /// A checkout (or its `.smix/leases`) to read. Repeatable.
        /// Defaults to the one above the working directory.
        #[arg(long = "from", value_name = "DIR")]
        from: Vec<PathBuf>,
        /// Say what would move and move nothing.
        ///
        /// The same decision path as the real thing, split only at the
        /// write — a rehearsal that works the answer out for itself is
        /// answering about itself.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete ledgers that no longer describe anything.
    ///
    /// A ledger that can only be added to stops describing the machine:
    /// a holder that died without releasing, or a boot row for a device
    /// that is off, sits there for ever and every later command has to
    /// reason around it. Says what it deleted and why; keeps everything
    /// else and says why too.
    Prune {
        /// Report what would go, and delete nothing.
        #[arg(long)]
        dry_run: bool,
        /// Look at this one device's ledger and no other.
        ///
        /// Prune reads every ledger on the machine, and a machine is
        /// shared: without this, cleaning up after your own device meant
        /// running a judgement over somebody else's rows too.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
    },
    /// Devices that left while a ledger still said they were here.
    ///
    /// Noticed by the next `run`, `runner`, `sim` or `lease` command after
    /// the device went: an emulator that exited, a simulator shut down
    /// from outside, a phone unplugged. Each entry says which device (and
    /// its AVD), when it was noticed, when its ledger last heard from it,
    /// who held it, whether smix booted it, what answers on its port now,
    /// and — for an emulator smix started — the last lines it printed on
    /// its console. An emulator smix did not start has no console here to
    /// read, and its exit status is nobody's to collect.
    History {
        /// Print the history as JSON.
        #[arg(long)]
        json: bool,
    },
}
