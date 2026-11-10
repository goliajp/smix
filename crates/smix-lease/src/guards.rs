//! What may be done to a device, decided before it is done.

/// Whether a device is physical, as far as the destructive-action gate
/// is concerned.
///
/// Deliberately not `smix_simctl::registry::DeviceKind` — this crate sits
/// below the registry and must not depend on it. The caller reads the
/// registry and passes the one bit that matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceClass {
    /// True for a phone or tablet somebody might be carrying.
    pub physical: bool,
    /// Whether destructive actions were allowed on this device once,
    /// recorded in the registry.
    pub destructive_opt_in: bool,
}

/// Why a destructive action was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructiveRefused {
    /// The device, as the person addressed it.
    pub device: String,
    /// The command that would allow it.
    pub remedy: String,
}

impl std::fmt::Display for DestructiveRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is a physical device and destructive actions are not allowed on it.\n\
             Erasing an app, resetting a keychain or wiping data on a phone somebody \
             carries is not undoable the way it is on a simulator, so it is off until \
             said otherwise — once, not per command:\n  {}",
            self.device, self.remedy
        )
    }
}

/// May a destructive action run on this device?
///
/// A simulator is never gated: it can be erased and rebuilt in a minute,
/// and gating it would add a step to the common case for no safety.
///
/// A physical device is gated until somebody says otherwise **once**,
/// recorded in the registry. Not a per-command `--yes`: a confirmation
/// that has to be typed every time ends up pasted into a script, which
/// is the same as not having one.
///
/// # Errors
///
/// Returns the refusal with the command that lifts it. A guard that only
/// says no gets worked around rather than obeyed — the same reason
/// `adb-guard`'s refusal names a way to do the thing safely.
pub fn may_destroy(device: &str, class: DeviceClass) -> Result<(), DestructiveRefused> {
    if !class.physical || class.destructive_opt_in {
        return Ok(());
    }
    Err(DestructiveRefused {
        device: device.to_string(),
        remedy: format!("smix sim allow-destructive {device}"),
    })
}

/// What is known about a device reference before anything is done to it.
///
/// The caller does the looking — reads the registry, asks `simctl` or
/// `adb` — and hands over what it found. Same split as [`DeviceClass`]:
/// this crate judges, it does not go and see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    /// In the registry, with these properties.
    Registered(DeviceClass),
    /// Not in the registry, but the platform itself calls it virtual —
    /// a simulator `simctl` lists, or a serial `adb` named `emulator-*`.
    UnregisteredVirtual,
    /// Not in the registry, and not something the platform calls virtual.
    Unknown,
}

/// Why a device reference was refused before anything ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAddressable {
    /// The device, as the person addressed it.
    pub device: String,
}

impl std::fmt::Display for NotAddressable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not a device smix may address: nothing here has registered it.\n\
             A simulator or emulator needs no registration; anything else does, \
             and being plugged in is not registration — that is the whole point \
             of the rule, because the phone that happens to be attached is \
             somebody's own.\n\
             Say so once and it becomes addressable:\n  \
             smix sim register <name> --udid {} --kind physical-ios|physical-android",
            self.device, self.device
        )
    }
}

/// May this device reference be addressed at all?
///
/// The charter's first constraint on physical devices is that one has to
/// be registered before it can be named — "whichever phone happens to be
/// plugged in" is never a target. This is where that stops being a
/// sentence and starts being a branch.
///
/// It is deliberately upstream of [`may_destroy`]. That gate asks whether
/// a *known* device may be harmed; this one asks whether smix is talking
/// to a device it was invited to talk to. Conflating them is how the two
/// holes of 2026-08-06 opened: an unregistered device reached the
/// executor and was stopped only because `simctl` happened not to
/// recognise it, and the Android path never asked at all.
///
/// # Errors
///
/// Returns the refusal, which names the registration that lifts it. A
/// device nobody registered is not a typo to retry — it is a decision
/// that has not been made yet, so the message asks for the decision.
pub fn may_address(device: &str, known: Known) -> Result<(), NotAddressable> {
    match known {
        // Registration is the invitation. What may then be *done* to it
        // is `may_destroy`'s question, not this one.
        Known::Registered(_) => Ok(()),
        // A simulator nobody registered stays addressable. Erasing one
        // costs a minute of rebuilding, so the charter's protection was
        // never aimed here — and `smix sim boot <fresh-udid>` is an
        // ordinary thing to do that must keep working.
        Known::UnregisteredVirtual => Ok(()),
        Known::Unknown => Err(NotAddressable {
            device: device.to_string(),
        }),
    }
}
