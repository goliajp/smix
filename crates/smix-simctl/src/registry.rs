//! The device registry — deterministic device addressing.
//!
//! Records live in the `smix-store` under `.smix/`. A pre-store
//! `sims.json` sitting beside it is imported on open and then left
//! alone; smix never writes that file again.
//!
//! Every smix device operation targets either an explicit UDID or an
//! alias recorded in this file. Resolution never consults the live
//! simulator set: the registry file is the only mapping source, so a
//! given input always resolves to the same device regardless of what
//! happens to be booted on the machine.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Failure variants for registry load / device-ref resolution.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Registry file could not be read.
    #[error("cannot read sim registry {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Registry file is not valid registry JSON.
    #[error("malformed sim registry {path}: {detail}")]
    Malformed {
        /// Path that failed to parse.
        path: String,
        /// Parser-side detail.
        detail: String,
    },
    /// Input is neither a UDID nor a recorded alias.
    #[error(
        "unknown device ref {device_ref:?} — pass an explicit UDID or one of \
         the recorded aliases: {}",
        known.join(", ")
    )]
    UnknownDevice {
        /// The input that failed to resolve.
        device_ref: String,
        /// Alias keys and device names available in the registry.
        known: Vec<String>,
    },
}

/// What kind of device a registry entry names.
///
/// The distinction exists for one reason: what smix is allowed to do to
/// it. A simulator can be erased and rebuilt in a minute; a phone in
/// somebody's pocket cannot. §9#1 hangs the destructive-action guard off
/// this field.
///
/// Defaults to `Simulator` when the field is absent, and that direction
/// is deliberate. Every registry written before this field existed was
/// written by a simulator; reading those as physical would lock a working
/// setup behind an opt-in nobody asked for. Guessing "simulator" costs at
/// most one missing gate on a device that never needed it — guessing
/// "physical" breaks people who did nothing wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    /// iOS Simulator.
    #[default]
    Simulator,
    /// Android emulator.
    Emulator,
    /// A physical iPhone or iPad.
    PhysicalIos,
    /// A physical Android device.
    PhysicalAndroid,
}

impl DeviceKind {
    /// Is this a device somebody might be carrying around?
    #[must_use]
    pub fn is_physical(self) -> bool {
        matches!(self, DeviceKind::PhysicalIos | DeviceKind::PhysicalAndroid)
    }
}

/// One registered simulator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredSim {
    /// Human-chosen device name (also usable as an alias).
    #[serde(rename = "deviceName")]
    pub device_name: String,
    /// What kind of device this is. Absent in registries written before
    /// physical devices were addressable — see [`DeviceKind`] for why
    /// that reads as `Simulator`.
    #[serde(default)]
    pub kind: DeviceKind,
    /// Whether destructive actions have been allowed on this device.
    ///
    /// Only consulted for physical devices; a simulator is never gated.
    /// Recorded once here rather than confirmed per command, because a
    /// confirmation that must be typed every time ends up in a script,
    /// which is the same as not having one.
    #[serde(default, rename = "destructiveOptIn")]
    pub destructive_opt_in: bool,
    /// CoreSimulator UDID.
    pub udid: String,
    /// Runtime identifier.
    pub runtime: String,
    /// Device type identifier.
    #[serde(rename = "deviceType")]
    pub device_type: String,
    /// Desired BCP 47 locale tag (e.g. `"en-US"`, `"ja-JP"`). When set,
    /// `smix sim boot` enforces it via
    /// `defaults write -g AppleLanguages + AppleLocale` and reboots the
    /// sim if the current locale differs. `None` (field absent) =
    /// honor whatever locale the sim boots with, no enforcement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Desired runner port (SmixRunner FlyingFox HTTP port). When set,
    /// `smix runner up <alias>` binds the runner to this port instead
    /// of the CLI default 22087. Two sims can then run their own runner
    /// in parallel without port collision
    /// (e.g. `sim-a.runnerPort = 22087` + `sim-b.runnerPort = 22088`).
    /// Falls through to `--runner-port` flag or `SMIX_RUNNER_PORT` env
    /// when absent.
    #[serde(
        default,
        rename = "runnerPort",
        skip_serializing_if = "Option::is_none"
    )]
    pub runner_port: Option<u16>,
}

/// What [`SimRegistry::register`] did with the alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// The alias did not exist; a new row was written.
    Added,
    /// The alias existed; its row was replaced.
    Updated,
}

/// Loaded view of the registry, keyed by alias.
#[derive(Debug, Default, Clone)]
pub struct SimRegistry {
    sims: BTreeMap<String, RegisteredSim>,
}

/// What a migration did, per alias.
///
/// Reported rather than summarised as a count. A migration is a one-way
/// door and the thing somebody needs afterwards is the ability to look
/// up one device by name and see what happened to it.
#[derive(Debug, Default, Clone)]
pub struct MigrationReport {
    /// Aliases the destination did not have.
    pub added: Vec<String>,
    /// Aliases that were already there, pointing at the same device.
    pub unchanged: Vec<String>,
    /// Aliases that were already there and whose record changed —
    /// which, under [`SimRegistry::merge`], can only mean consent
    /// narrowed.
    pub narrowed: Vec<String>,
    /// Sources that opened and held nothing.
    pub empty: Vec<PathBuf>,
    /// Sources that would not open, and why.
    pub unreadable: Vec<(PathBuf, String)>,
}

/// Every registry this machine can read, folded into one.
#[derive(Debug, Default, Clone)]
pub struct MergedRegistry {
    /// The merged view. Which book a device came from is not visible
    /// here, deliberately: a device is a device.
    pub registry: SimRegistry,
    /// Aliases a checkout is the only holder of, and which checkout.
    ///
    /// Not an error — those devices work. It is the one thing a caller
    /// has to be able to say out loud, because a record only one tree
    /// holds is a record the next tree cannot act on, and the whole
    /// point of moving these was that "check who owns this runner" can
    /// be carried out from anywhere.
    pub unmigrated: BTreeMap<String, PathBuf>,
}

/// Whether `s` has CoreSimulator UDID form (8-4-4-4-12 hex).
///
/// Answers the shape question only. It used to answer more than that —
/// UDID-form input was treated as a deliberate instruction that skipped
/// the registry entirely, and the CLI still short-circuits alias lookup
/// on it. What changed on 2026-08-06 is that skipping the registry no
/// longer means skipping every check: a raw identifier now has to be one
/// the platform itself claims, because the shape alone stopped being
/// evidence the moment a `devicectl` path appeared that reaches phones
/// whose CoreDevice UUIDs wear exactly this form.
pub fn is_udid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *b != b'-' {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Why an identifier does not fit the kind it was registered under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierMismatch {
    /// The identifier as given.
    pub given: String,
    /// What that kind's identifiers look like.
    pub expected: &'static str,
}

impl std::fmt::Display for IdentifierMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} is not how that kind of device is identified — {}",
            self.given, self.expected
        )
    }
}

/// Does this identifier fit the kind it is being registered as?
///
/// Shape only; whether the device exists is a separate question, asked
/// against a different catalogue per kind, and asked by the caller.
///
/// The three answers differ because the world does:
///
/// * A **simulator** is a directory on this Mac with a CoreSimulator
///   UDID, and `simctl` can list every one of them.
/// * An **emulator** is named by `adb`, which calls them `emulator-<port>`
///   and will list them too.
/// * A **phone** has no catalogue at all. Nothing on this machine can
///   enumerate the world's devices, so its identifier is taken as given —
///   which is exactly why registering one has to be a deliberate act
///   rather than a lookup. That is not a hole in the check; it is the
///   reason the check exists.
///
/// # Errors
///
/// Returns what that kind's identifiers look like, so the message can
/// say which of the three worlds the caller landed in the wrong one of.
pub fn identifier_fits(kind: DeviceKind, id: &str) -> Result<(), IdentifierMismatch> {
    let ok = match kind {
        DeviceKind::Simulator => is_udid(id),
        DeviceKind::Emulator => is_emulator_serial(id),
        // No catalogue exists to check against.
        DeviceKind::PhysicalIos | DeviceKind::PhysicalAndroid => !id.trim().is_empty(),
    };
    if ok {
        return Ok(());
    }
    Err(IdentifierMismatch {
        given: id.to_string(),
        expected: match kind {
            DeviceKind::Simulator => {
                "a simulator has a CoreSimulator UDID (8-4-4-4-12 hex); find it with `smix sim list`"
            }
            DeviceKind::Emulator => {
                "adb names an emulator `emulator-<port>`, e.g. emulator-5554; find it with `adb devices`"
            }
            DeviceKind::PhysicalIos | DeviceKind::PhysicalAndroid => {
                "a physical device needs a non-empty identifier: a UDID for iOS, an adb serial for Android"
            }
        },
    })
}

/// Put an identifier in the form its platform matches on.
///
/// Normalise what has a normal form; preserve what is matched verbatim.
///
/// Apple's identifiers are hex and canonically upper-case, and this is
/// not a style preference: `devicectl` was measured on 2026-08-06 to
/// reject the lower-case spelling of a UDID it accepts in upper-case
/// (`ERROR: The specified device was not found`). Upper-casing therefore
/// rescues a typed-in identifier rather than mangling it.
///
/// `adb` matches serials byte for byte, so there is nothing to rescue and
/// everything to break: `EMULATOR-5554` is not a device, and neither is a
/// vendor serial that came with lower-case letters in it.
///
/// Done once, here, where the kind is known — never again downstream. A
/// value normalised twice by two different rules is how `sim resolve` came
/// to hand out `EMULATOR-5554`.
#[must_use]
pub fn canonical_identifier(kind: DeviceKind, id: &str) -> String {
    match kind {
        DeviceKind::Simulator | DeviceKind::PhysicalIos => id.to_ascii_uppercase(),
        DeviceKind::Emulator | DeviceKind::PhysicalAndroid => id.to_string(),
    }
}

/// Whether `s` is an adb emulator serial.
///
/// `adb` is the one naming these, so this recognises rather than guesses:
/// an emulator is `emulator-<port>`, and a physical device answers with a
/// hardware serial that never takes that form. Case matters — `adb`
/// matches serials verbatim and `EMULATOR-5554` is not a device.
#[must_use]
pub fn is_emulator_serial(s: &str) -> bool {
    s.strip_prefix("emulator-")
        .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
}

/// Resolve a caller-supplied path to the `.smix` directory holding the
/// store.
///
/// Callers pass either form: `SMIX_SIMS_JSON` documents a file, while
/// discovery yields a directory. Accepting both is also what lets the
/// pre-store test suite keep exercising the legacy import path
/// unchanged, instead of being rewritten into agreement with the code
/// it is supposed to check.
pub fn store_dir(path: &Path) -> PathBuf {
    smix_dir(path)
}

fn smix_dir(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e == "json") {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    }
}

/// Open the store under `path` and fold in any legacy `sims.json`
/// sitting beside it.
///
/// The legacy file is read, never written and never removed: a user who
/// has to go back to a pre-store smix must still find their registry.
fn open_store(path: &Path) -> Result<smix_store::Store, RegistryError> {
    let dir = smix_dir(path);
    std::fs::create_dir_all(&dir).map_err(|source| RegistryError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let store = smix_store::Store::open(&dir).map_err(|e| RegistryError::Io {
        path: dir.display().to_string(),
        source: std::io::Error::other(e.to_string()),
    })?;
    let legacy = dir.join("sims.json");
    smix_store::import_legacy_records(&store.sims(), &legacy, "sims").map_err(|e| {
        RegistryError::Malformed {
            path: legacy.display().to_string(),
            detail: e.to_string(),
        }
    })?;
    Ok(store)
}

impl SimRegistry {
    /// Write `sim` into the registry under `alias`.
    ///
    /// One key, not a whole file. The read-modify-write this replaces
    /// lost an alias whenever two processes registered at once — each
    /// read the file, each inserted its own row, and the second write
    /// erased the first, with no error on either side.
    pub fn register(
        path: &Path,
        alias: &str,
        sim: RegisteredSim,
    ) -> Result<RegisterOutcome, RegistryError> {
        let store = open_store(path)?;
        let existed = store
            .sims()
            .get(alias)
            .map_err(|e| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?
            .is_some();
        store
            .sims()
            .put_json(alias, &sim)
            .map_err(|e| RegistryError::Io {
                path: path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        store.sync().map_err(|e| RegistryError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
        Ok(if existed {
            RegisterOutcome::Updated
        } else {
            RegisterOutcome::Added
        })
    }

    /// Remove one alias from the registry at `path`.
    ///
    /// The other half of [`Self::register`], and absent until the
    /// records moved to machine scope made its absence permanent: a
    /// device registered by mistake — a test that wrote to the real
    /// book, an alias for a phone somebody no longer has — could be
    /// added and never taken back. A registry that only grows stops
    /// describing the machine.
    ///
    /// Removes the name, not the device. Another alias for the same
    /// device keeps working, which is why this takes an alias key
    /// rather than a device ref: "forget this device" and "forget this
    /// name for it" are different requests, and only the second one is
    /// unambiguous.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownDevice`] when no such alias exists.
    /// Silently succeeding would let a typo read as a removal.
    pub fn unregister(path: &Path, alias: &str) -> Result<RegisteredSim, RegistryError> {
        let reg = Self::load(path)?;
        let Some(sim) = reg.sims.get(alias).cloned() else {
            return Err(RegistryError::UnknownDevice {
                device_ref: alias.to_string(),
                known: reg.sims.keys().cloned().collect(),
            });
        };
        let store = open_store(path)?;
        store.sims().delete(alias).map_err(|e| RegistryError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
        store.sync().map_err(|e| RegistryError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
        Ok(sim)
    }

    /// Allow destructive actions on one registered device, once.
    ///
    /// Goes through [`Self::register`] rather than rewriting the file,
    /// for the reason that function documents: a read-modify-write of the
    /// whole registry loses a concurrent registration silently. One key
    /// in, one key out.
    ///
    /// Returns the alias it was recorded against and whether it was
    /// already allowed — the caller can then say "already allowed"
    /// instead of implying something changed.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownDevice`] when nothing matches the ref. The
    /// opt-in is per device, so there is nothing to record it against —
    /// and silently creating an entry would mean allowing destruction on
    /// a device nobody registered.
    pub fn allow_destructive(
        path: &Path,
        device_ref: &str,
    ) -> Result<(String, bool), RegistryError> {
        let reg = Self::load(path)?;
        let Some((alias, sim)) = reg
            .sims()
            .iter()
            .find(|(alias, sim)| {
                alias.as_str() == device_ref
                    || sim.device_name == device_ref
                    || sim.udid.eq_ignore_ascii_case(device_ref)
            })
            .map(|(a, s)| (a.clone(), s.clone()))
        else {
            let mut known: Vec<String> = Vec::new();
            for (alias, sim) in reg.sims() {
                known.push(alias.clone());
                known.push(sim.device_name.clone());
            }
            return Err(RegistryError::UnknownDevice {
                device_ref: device_ref.to_string(),
                known,
            });
        };
        if sim.destructive_opt_in {
            return Ok((alias, true));
        }
        let updated = RegisteredSim {
            destructive_opt_in: true,
            ..sim
        };
        Self::register(path, &alias, updated)?;
        Ok((alias, false))
    }

    /// Read every registered sim.
    ///
    /// `path` may be the `.smix` directory or a legacy `sims.json`
    /// inside it; both land on the same store.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let store = open_store(path)?;
        let mut sims = BTreeMap::new();
        for alias in store.sims().list() {
            let sim: RegisteredSim = store
                .sims()
                .get_json(&alias)
                .map_err(|e| RegistryError::Malformed {
                    path: path.display().to_string(),
                    detail: e.to_string(),
                })?
                .ok_or_else(|| RegistryError::Malformed {
                    path: path.display().to_string(),
                    detail: format!("`{alias}` vanished between listing and reading"),
                })?;
            sims.insert(alias, sim);
        }
        Ok(Self { sims })
    }

    /// Where device facts live: this machine, not this checkout.
    ///
    /// A simulator is an operating-system object. Its UDID, its runtime
    /// version, whether it is booted and who booted it do not change
    /// when you `cd` — so storing them per checkout means four trees on
    /// one machine each hold their own answer, which is what they do
    /// today. On 2026-08-11 two of them held a lease on the same
    /// simulators, and a runner on port 22087 was simultaneously on the
    /// books and invisible: the rule says check the owner before
    /// touching a runner, and the checkout doing the checking was not
    /// the one holding the record.
    ///
    /// `$XDG_DATA_HOME/smix` or `~/.local/share/smix`, which is where
    /// the runner tree and its version stamp already live — machine
    /// scope is not a new idea here, it was just not used for this.
    ///
    /// `None` when neither variable is set, which is the same condition
    /// under which the runner tree has no home either; the caller falls
    /// back to the checkout and says so.
    pub fn machine_dir() -> Option<PathBuf> {
        smix_lease::store::machine_root().map(|r| r.join("devices"))
    }

    /// Every registry a read may draw on, in precedence order.
    ///
    /// `SMIX_SIMS_JSON` names one registry and means exactly that one:
    /// it is how a test works against a book of its own, and a
    /// machine-level fallback under it would let the real one leak in.
    ///
    /// Otherwise the machine registry first, then whatever checkout is
    /// underfoot. The checkout entry keeps books written before this
    /// move working until `smix sim migrate` folds them in; nothing is
    /// ever written back to it.
    pub fn read_paths(start: &Path) -> Vec<PathBuf> {
        if let Some(p) = std::env::var_os("SMIX_SIMS_JSON") {
            return vec![PathBuf::from(p)];
        }
        let mut paths = Vec::new();
        if let Some(m) = Self::machine_dir() {
            paths.push(m);
        }
        if let Some(c) = Self::discover(start) {
            paths.push(c);
        }
        paths
    }

    /// Read every registry that applies and fold them into one.
    ///
    /// A source that will not open is skipped rather than fatal: one
    /// corrupt book must not strand the devices recorded in the others.
    /// Which is why this returns a value and not a `Result` — there is
    /// no failure here other than "nothing was readable anywhere", and
    /// that is an empty registry, which reads the same as a machine
    /// where nothing has been registered yet.
    pub fn open_all(start: &Path) -> MergedRegistry {
        let paths = Self::read_paths(start);
        // Under `SMIX_SIMS_JSON` there is no machine/checkout split to
        // report: the caller named the registry, and calling their own
        // choice "unmigrated" would be advice to move a book they put
        // where they wanted it.
        let machine = if std::env::var_os("SMIX_SIMS_JSON").is_some() {
            None
        } else {
            Self::machine_dir()
        };
        let mut loaded: Vec<Self> = Vec::new();
        let mut unmigrated: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut on_machine: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for path in &paths {
            let Ok(reg) = Self::load(path) else {
                continue;
            };
            if machine.as_deref() == Some(path.as_path()) {
                on_machine.extend(reg.sims.values().map(|s| s.udid.to_ascii_uppercase()));
            } else if machine.is_some() {
                for (alias, sim) in &reg.sims {
                    if !on_machine.contains(&sim.udid.to_ascii_uppercase()) {
                        unmigrated.insert(alias.clone(), path.clone());
                    }
                }
            }
            loaded.push(reg);
        }
        MergedRegistry {
            registry: Self::merge(loaded),
            unmigrated,
        }
    }

    /// Walk up from `start` looking for a `.smix` that holds a
    /// registry — either the store or a legacy `sims.json`.
    pub fn discover(start: &Path) -> Option<PathBuf> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let smix = d.join(".smix");
            if smix.join("sims.json").is_file() || smix.join("kv").is_dir() {
                return Some(smix);
            }
            dir = d.parent();
        }
        None
    }

    /// Resolve a device ref to the identifier its platform is addressed by.
    ///
    /// CoreSimulator-form input passes through whether or not it is
    /// registered. Otherwise the ref must match an alias key, a
    /// `deviceName`, or the registered identifier itself.
    ///
    /// That last one was missing until 2026-08-06, and [`Self::lookup`]
    /// had it — so the two disagreed about whether a device's own
    /// identifier names it. A real phone found the disagreement: an iOS
    /// device UDID is 25 characters, not CoreSimulator's 36, so it fell
    /// past the short-circuit into a search that never looked at the one
    /// field it matched. `smix runner forward 00008120-…` answered
    /// "unknown device ref" about a device that was registered right
    /// there in the file it was reading.
    pub fn resolve(&self, device_ref: &str) -> Result<String, RegistryError> {
        if is_udid(device_ref) {
            return Ok(device_ref.to_ascii_uppercase());
        }
        // Stored verbatim, returned verbatim. The value was already put
        // in its platform's form at registration by
        // [`canonical_identifier`]; upper-casing it a second time here is
        // what turned a registered `emulator-5554` into the
        // `EMULATOR-5554` that adb does not answer to.
        if let Some(sim) = self.sims.get(device_ref) {
            return Ok(sim.udid.clone());
        }
        if let Some(sim) = self
            .sims
            .values()
            .find(|s| s.device_name == device_ref || s.udid.eq_ignore_ascii_case(device_ref))
        {
            return Ok(sim.udid.clone());
        }
        // Deduplicated: an alias and a device name are commonly the same
        // word, and "one of the recorded aliases: phone, phone" reads as
        // a bug in the tool rather than a list of choices.
        let mut known: Vec<String> = Vec::with_capacity(self.sims.len() * 2);
        for (alias, sim) in &self.sims {
            for name in [alias, &sim.device_name] {
                if !known.iter().any(|k| k == name) {
                    known.push(name.clone());
                }
            }
        }
        Err(RegistryError::UnknownDevice {
            device_ref: device_ref.to_string(),
            known,
        })
    }

    /// All registered sims, keyed by alias.
    /// Fold several registries into one, losing nothing.
    ///
    /// Four checkouts on this machine each keep their own, and merging
    /// them is a one-way door: whatever this drops, the tree that was
    /// relying on it stops working, and nobody can tell which of the
    /// four to look in. So the rules are chosen to be safe, not tidy.
    ///
    /// - **Two aliases for one device: keep both.** An alias is how
    ///   somebody types a device's name. Dropping one breaks whatever
    ///   script used it; a duplicate is noise.
    /// - **Conflicting destructive consent: take the stricter.**
    ///   Consent is a per-device authorisation (§9 #1), and merging two
    ///   books is not a moment to widen it. Granting it again is one
    ///   command; un-wiping a phone is not a command at all.
    /// - **Same alias, different devices: keep both**, the later one
    ///   under a suffixed alias. Silently overwriting means one tree's
    ///   alias stops resolving with no way to know which.
    ///
    /// Order-independent for the facts that matter: four checkouts have
    /// no natural order, and a merge that depends on read order changes
    /// when somebody renames a directory.
    pub fn merge(sources: impl IntoIterator<Item = Self>) -> Self {
        let mut out: BTreeMap<String, RegisteredSim> = BTreeMap::new();
        // Sorted, so the result cannot depend on which tree was read
        // first.
        let mut incoming: Vec<(String, RegisteredSim)> = sources
            .into_iter()
            .flat_map(|r| r.sims.into_iter())
            .collect();
        incoming.sort_by(|a, b| (&a.0, &a.1.udid).cmp(&(&b.0, &b.1.udid)));

        for (alias, sim) in incoming {
            match out.get_mut(&alias) {
                None => {
                    out.insert(alias, sim);
                }
                Some(existing) if existing.udid == sim.udid => {
                    // Same device under the same name: the only thing
                    // that can differ is consent, and it narrows.
                    existing.destructive_opt_in =
                        existing.destructive_opt_in && sim.destructive_opt_in;
                }
                Some(_) => {
                    // Same name, different device. Both survive; the
                    // second takes a suffix derived from its UDID so the
                    // name is stable across merges rather than depending
                    // on how many collisions came before it.
                    let short = sim.udid.chars().take(8).collect::<String>().to_lowercase();
                    out.insert(format!("{alias}-{short}"), sim);
                }
            }
        }
        Self { sims: out }
    }

    /// Fold `sources` into the registry at `into`, keeping everything.
    ///
    /// The merge rules are [`Self::merge`]'s; this applies them to books
    /// on disk. What it adds to them is a promise about the sources:
    /// they are read and left alone. Somebody who has to go back to a
    /// smix from before device records became machine-scoped must still
    /// find their registry where they left it — and a migration that
    /// deletes what it has just copied has no way to be run twice by
    /// somebody who is not sure whether it worked.
    ///
    /// Writes go through [`Self::register`], one key at a time, for the
    /// reason that function documents: a whole-file rewrite loses a
    /// concurrent registration without saying so.
    pub fn migrate(into: &Path, sources: &[PathBuf]) -> Result<MigrationReport, RegistryError> {
        let mut report = MigrationReport::default();
        let mut books = vec![Self::load(into)?];
        let before: BTreeMap<String, String> = books[0]
            .sims
            .iter()
            .map(|(a, s)| (a.clone(), s.udid.clone()))
            .collect();

        for src in sources {
            match Self::load(src) {
                Ok(reg) if reg.sims.is_empty() => report.empty.push(src.clone()),
                Ok(reg) => books.push(reg),
                // Named, not fatal. One book nobody can open must not
                // strand the devices recorded in the others — and the
                // path is what somebody needs in order to go look.
                Err(e) => report.unreadable.push((src.clone(), e.to_string())),
            }
        }

        let merged = Self::merge(books);
        for (alias, sim) in &merged.sims {
            match before.get(alias) {
                Some(udid) if udid == &sim.udid => report.unchanged.push(alias.clone()),
                Some(_) => report.narrowed.push(alias.clone()),
                None => report.added.push(alias.clone()),
            }
            Self::register(into, alias, sim.clone())?;
        }
        Ok(report)
    }

    /// Every alias and what it points at.
    pub fn all(&self) -> impl Iterator<Item = (&str, &RegisteredSim)> {
        self.sims.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Add or replace one entry. For merging and for tests; the
    /// registering path goes through `register`, which also writes.
    pub fn insert(&mut self, alias: impl Into<String>, sim: RegisteredSim) {
        self.sims.insert(alias.into(), sim);
    }

    /// Every registered sim, keyed by alias.
    pub fn sims(&self) -> &BTreeMap<String, RegisteredSim> {
        &self.sims
    }

    /// Look up a [`RegisteredSim`] by alias key, device name, or UDID.
    /// Returns `None` if no entry matches any of the three. Mirrors
    /// [`Self::resolve`]'s match precedence so cli callers can fetch
    /// the full spec (e.g. `locale` field) after they already resolved
    /// the UDID.
    pub fn lookup(&self, device_ref: &str) -> Option<&RegisteredSim> {
        if let Some(sim) = self.sims.get(device_ref) {
            return Some(sim);
        }
        self.sims
            .values()
            .find(|sim| sim.device_name == device_ref || sim.udid.eq_ignore_ascii_case(device_ref))
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;

    const UDID: &str = "47ACEAE5-36BA-4C62-811B-F09B397910D7";

    #[test]
    fn each_virtual_kind_takes_its_own_platforms_identifiers() {
        assert!(identifier_fits(DeviceKind::Simulator, UDID).is_ok());
        assert!(identifier_fits(DeviceKind::Emulator, "emulator-5554").is_ok());
        // And not each other's. A UDID registered as an emulator would
        // be an alias for something adb can never be handed.
        assert!(identifier_fits(DeviceKind::Simulator, "emulator-5554").is_err());
        assert!(identifier_fits(DeviceKind::Emulator, UDID).is_err());
    }

    #[test]
    fn a_physical_identifier_is_taken_as_given() {
        // Nothing on this machine can enumerate the world's phones, so
        // there is no catalogue to check against — which is precisely
        // why registering one is a deliberate act. Both spellings are
        // legitimate: a UDID for iOS, an adb serial for Android.
        assert!(identifier_fits(DeviceKind::PhysicalIos, "00008120-001410C11A42201E").is_ok());
        assert!(identifier_fits(DeviceKind::PhysicalAndroid, "R5CT52DF07D").is_ok());
        // Empty is still nothing.
        assert!(identifier_fits(DeviceKind::PhysicalIos, "   ").is_err());
    }

    #[test]
    fn an_emulator_serial_is_recognised_not_guessed() {
        assert!(is_emulator_serial("emulator-5554"));
        assert!(!is_emulator_serial("emulator-"));
        assert!(!is_emulator_serial("emulator-abcd"));
        // Case matters: adb matches serials verbatim, and this is not a
        // device. The UDID path upper-cases; this one must not.
        assert!(!is_emulator_serial("EMULATOR-5554"));
        assert!(!is_emulator_serial("R5CT52DF07D"));
    }

    #[test]
    fn apple_identifiers_are_normalised_and_adb_serials_are_not() {
        // Measured, not assumed: `devicectl` rejects the lower-case
        // spelling of a UDID it accepts in upper-case, so upper-casing
        // an Apple identifier rescues it. `adb` matches byte for byte,
        // so the same move would break it.
        assert_eq!(
            canonical_identifier(
                DeviceKind::Simulator,
                "47aceae5-36ba-4c62-811b-f09b397910d7"
            ),
            "47ACEAE5-36BA-4C62-811B-F09B397910D7"
        );
        assert_eq!(
            canonical_identifier(DeviceKind::PhysicalIos, "00008120-001410c11a42201e"),
            "00008120-001410C11A42201E"
        );
        assert_eq!(
            canonical_identifier(DeviceKind::Emulator, "emulator-5554"),
            "emulator-5554"
        );
        assert_eq!(
            canonical_identifier(DeviceKind::PhysicalAndroid, "abc123xyz"),
            "abc123xyz"
        );
    }

    #[test]
    fn an_alias_resolves_to_what_was_stored_not_to_an_upper_cased_copy() {
        // The bug this pins: `sim resolve` used to upper-case whatever
        // it returned, so a registered `emulator-5554` came back as
        // `EMULATOR-5554` — a string adb does not answer to. Normalising
        // happens once, at registration, where the kind is known.
        let mut sims = BTreeMap::new();
        sims.insert(
            "emu".to_string(),
            RegisteredSim {
                device_name: "emu".into(),
                udid: "emulator-5554".into(),
                runtime: String::new(),
                device_type: String::new(),
                locale: None,
                runner_port: None,
                kind: DeviceKind::Emulator,
                destructive_opt_in: false,
            },
        );
        let reg = SimRegistry { sims };
        assert_eq!(reg.resolve("emu").unwrap(), "emulator-5554");
    }

    #[test]
    fn the_mismatch_says_what_that_kind_looks_like() {
        // "not UDID-form" told an Android user the shape of a thing they
        // were not registering. The message has to name the world they
        // are actually in.
        let e = identifier_fits(DeviceKind::Emulator, UDID).expect_err("must refuse");
        let msg = e.to_string();
        assert!(msg.contains("emulator-<port>"), "got: {msg}");
        assert!(msg.contains("adb devices"), "got: {msg}");
        assert!(!msg.contains("8-4-4-4-12"), "wrong world named: {msg}");
    }

    #[test]
    fn a_registry_written_before_this_field_reads_as_simulator() {
        // The compatibility case that matters: every existing registry on
        // every machine was written without `kind`. Reading those as
        // physical would put a working simulator setup behind an opt-in
        // its owner never asked for.
        let json = r#"{
            "deviceName": "sim-smix-02",
            "udid": "5D087114-ECB3-443C-8DDB-40EEF9CFB90C",
            "runtime": "iOS-26-5",
            "deviceType": "iPhone-17-Pro"
        }"#;
        let sim: RegisteredSim = serde_json::from_str(json).expect("old record still parses");
        assert_eq!(sim.kind, DeviceKind::Simulator);
        assert!(!sim.kind.is_physical());
        assert!(!sim.destructive_opt_in, "opt-in defaults to off");
    }

    #[test]
    fn every_kind_knows_whether_it_is_physical() {
        assert!(!DeviceKind::Simulator.is_physical());
        assert!(!DeviceKind::Emulator.is_physical());
        assert!(DeviceKind::PhysicalIos.is_physical());
        assert!(DeviceKind::PhysicalAndroid.is_physical());
    }

    #[test]
    fn kind_roundtrips_with_its_wire_spelling_pinned() {
        let sim = RegisteredSim {
            device_name: "panda".into(),
            kind: DeviceKind::PhysicalIos,
            destructive_opt_in: true,
            udid: "00008120-001410C11A42201E".into(),
            runtime: "iOS-26-5".into(),
            device_type: "iPhone15,4".into(),
            locale: None,
            runner_port: None,
        };
        let json = serde_json::to_string(&sim).expect("serialize");
        assert!(json.contains("\"physicalIos\""), "got: {json}");
        assert!(json.contains("\"destructiveOptIn\":true"), "got: {json}");
        let back: RegisteredSim = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind, DeviceKind::PhysicalIos);
        assert!(back.destructive_opt_in);
    }
}
