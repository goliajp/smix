//! Whether this machine can drive anything yet, and what to run next.
//!
//! `smix doctor` counted runtimes and devices and stopped there. Someone
//! meeting smix for the first time reads that and still does not know
//! whether they are ready, and if not, what to do about it — the answer
//! was five commands long and lived in the README.
//!
//! So the question this module answers is not "what is installed" but
//! "what is the next command". Judging is separated from looking: the
//! caller gathers [`Facts`] by running simctl and reading the filesystem,
//! and everything after that is a pure function, which is what makes the
//! ordering testable without a simulator.

use serde::Serialize;

/// What the environment looks like. Gathered by the caller; never read
/// from the world in here.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// `None` when `xcrun simctl` could not be run at all.
    pub simctl: Option<SimctlFacts>,
    /// `None` when no `.smix` registry was found from the working directory.
    pub registry: Option<RegistryFacts>,
    /// Whether something is already answering on the runner port.
    pub runner_up: bool,
}

/// What simctl reported.
#[derive(Debug, Clone, Default)]
pub struct SimctlFacts {
    /// Runtimes marked available — a runtime that is present but not
    /// available cannot boot a device.
    pub available_runtimes: usize,
    /// Devices marked available.
    pub available_devices: usize,
}

/// What the workspace registry holds.
#[derive(Debug, Clone, Default)]
pub struct RegistryFacts {
    /// Aliases registered. Zero means the file exists but names no device.
    pub aliases: usize,
    /// An alias to suggest in the next command.
    pub first_alias: Option<String>,
}

/// How one check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Satisfied.
    Ok,
    /// Not satisfied, and it is what stands between here and driving.
    Blocked,
    /// Not looked at, because something it depends on is blocked.
    Skipped,
}

/// One line of the verdict.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Stable identifier, so a script can match on something other than prose.
    pub id: &'static str,
    /// How it came out.
    pub status: Status,
    /// What was found, in a sentence.
    pub detail: String,
}

/// The command to run next, and why it is the one.
#[derive(Debug, Clone, Serialize)]
pub struct NextStep {
    /// Ready to paste into a shell.
    pub command: String,
    /// What running it gets you.
    pub reason: String,
}

/// The verdict.
#[derive(Debug, Clone, Serialize)]
pub struct Readiness {
    /// True when nothing is blocked and there is nothing left to set up.
    pub ready: bool,
    /// Every check, in dependency order.
    pub checks: Vec<Check>,
    /// `None` exactly when `ready` is true.
    pub next: Option<NextStep>,
}

/// The platform boundary, stated once.
///
/// The old wording said smix supports "iOS Simulator only", which stopped
/// being true when the Android emulator lane landed and left the first
/// command a new user runs telling them their platform was unsupported.
/// The boundary that is actually enforced is §9#1: simulators and
/// emulators, never a physical device.
pub const PLATFORM_NOTE: &str =
    "smix drives iOS Simulators and Android emulators. Physical devices are out of scope.";

/// Judge the facts.
///
/// Checks are ordered by dependency and stop at the first blocked one:
/// reporting "no registry" to someone whose Xcode tools are missing sends
/// them to a command that cannot succeed yet.
pub fn assess(facts: &Facts) -> Readiness {
    let mut checks = Vec::new();

    let Some(simctl) = &facts.simctl else {
        checks.push(Check {
            id: "simctl",
            status: Status::Blocked,
            detail: "xcrun simctl could not be run — the Xcode command-line tools are \
                     not installed, or no Xcode is selected"
                .into(),
        });
        return blocked(
            checks,
            "xcode-select --install",
            "installs the command-line tools smix drives the simulator through",
            2,
        );
    };
    checks.push(Check {
        id: "simctl",
        status: Status::Ok,
        // Reachability only — the runtime count is the next check's to
        // report, and saying it twice reads as two findings.
        detail: "xcrun simctl reachable".into(),
    });

    if simctl.available_runtimes == 0 {
        checks.push(Check {
            id: "runtime",
            status: Status::Blocked,
            detail: "no available simulator runtime — Xcode is installed but carries no \
                     usable iOS runtime"
                .into(),
        });
        return blocked(
            checks,
            "xcodebuild -downloadPlatform iOS",
            "downloads an iOS runtime, without which no simulator can boot",
            1,
        );
    }
    checks.push(Check {
        id: "runtime",
        status: Status::Ok,
        detail: format!("{} runtimes available", simctl.available_runtimes),
    });

    if simctl.available_devices == 0 {
        checks.push(Check {
            id: "device",
            status: Status::Blocked,
            detail: "no available simulator — a runtime is installed but no device uses it".into(),
        });
        return blocked(
            checks,
            "xcrun simctl create smix-dev 'iPhone 17 Pro' <runtime-id>",
            "creates a simulator for smix to register and drive",
            1,
        );
    }
    checks.push(Check {
        id: "device",
        status: Status::Ok,
        detail: format!("{} simulators available", simctl.available_devices),
    });

    let registered = match &facts.registry {
        Some(r) if r.aliases > 0 => r,
        _ => {
            checks.push(Check {
                id: "registry",
                status: Status::Blocked,
                detail: "no device registered in .smix — every smix command takes an \
                         explicit device, and an alias is where that comes from"
                    .into(),
            });
            return blocked(
                checks,
                "smix init",
                "registers a simulator under an alias and creates the .smix registry",
                1,
            );
        }
    };
    checks.push(Check {
        id: "registry",
        status: Status::Ok,
        detail: format!("{} device(s) registered in .smix", registered.aliases),
    });

    // An alias is guaranteed here: aliases > 0 is what got us past the
    // check above. The fallback keeps the next command printable rather
    // than silently dropping it if that ever stops holding.
    let alias = registered
        .first_alias
        .clone()
        .unwrap_or_else(|| "dev".into());

    if !facts.runner_up {
        checks.push(Check {
            id: "runner",
            status: Status::Blocked,
            detail: "no runner answering — sense and act need one on the device".into(),
        });
        return blocked(
            checks,
            format!("smix capsule up {alias} --bundle <your.bundle.id>"),
            "boots the device and starts the runner that carries every tap and query",
            0,
        );
    }
    checks.push(Check {
        id: "runner",
        status: Status::Ok,
        detail: "runner answering".into(),
    });

    Readiness {
        ready: true,
        checks,
        next: None,
    }
}

/// Finish a verdict at the first blocked check, marking what was not looked at.
///
/// `remaining` is how many checks below this one exist; they are reported
/// as skipped rather than omitted, so the output shows the whole path and
/// where along it the reader currently stands.
fn blocked(
    mut checks: Vec<Check>,
    command: impl Into<String>,
    reason: impl Into<String>,
    remaining: usize,
) -> Readiness {
    const ORDER: [&str; 5] = ["simctl", "runtime", "device", "registry", "runner"];
    let done = checks.len();
    for id in ORDER.iter().skip(done).take(remaining) {
        checks.push(Check {
            id,
            status: Status::Skipped,
            detail: "not checked — an earlier step has to pass first".into(),
        });
    }
    Readiness {
        ready: false,
        checks,
        next: Some(NextStep {
            command: command.into(),
            reason: reason.into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simctl_ok() -> SimctlFacts {
        SimctlFacts {
            available_runtimes: 1,
            available_devices: 3,
        }
    }

    #[test]
    fn readiness_sends_a_fresh_workspace_to_init() {
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: None,
            runner_up: false,
        });
        assert!(!r.ready);
        let next = r
            .next
            .expect("a blocked verdict must name the next command");
        assert_eq!(next.command, "smix init");
        assert!(
            next.reason.contains(".smix"),
            "the reason should name what init creates: {}",
            next.reason
        );
    }

    #[test]
    fn readiness_sends_a_registered_workspace_to_capsule_up() {
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 1,
                first_alias: Some("dev".into()),
            }),
            runner_up: false,
        });
        assert!(!r.ready);
        let next = r.next.expect("still blocked on the runner");
        assert!(
            next.command.starts_with("smix capsule up dev"),
            "next should drive the registered alias: {}",
            next.command
        );
    }

    #[test]
    fn a_missing_toolchain_is_reported_before_anything_it_would_break() {
        // Telling someone with no Xcode tools to run `smix init` sends them
        // to a command that cannot succeed: init resolves a device through
        // simctl. The first broken link has to be the one reported.
        let r = assess(&Facts {
            simctl: None,
            registry: None,
            runner_up: false,
        });
        let next = r.next.expect("blocked");
        assert!(
            next.command.contains("xcode-select"),
            "got: {}",
            next.command
        );
        assert!(!next.command.contains("smix init"));
        assert_eq!(r.checks[0].status, Status::Blocked);
        assert!(
            r.checks.iter().skip(1).all(|c| c.status == Status::Skipped),
            "checks below a blocked one are not verdicts about the machine"
        );
    }

    #[test]
    fn everything_satisfied_leaves_nothing_to_run() {
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 2,
                first_alias: Some("dev".into()),
            }),
            runner_up: true,
        });
        assert!(r.ready);
        assert!(r.next.is_none(), "a ready machine has no next command");
        assert!(r.checks.iter().all(|c| c.status == Status::Ok));
    }

    #[test]
    fn a_registry_file_naming_no_device_is_not_a_registry() {
        // The file exists after any `.smix` write, so its presence says
        // nothing about whether a device can be addressed.
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 0,
                first_alias: None,
            }),
            runner_up: false,
        });
        assert_eq!(r.next.expect("blocked").command, "smix init");
    }

    #[test]
    fn readiness_serializes_with_the_fields_a_script_reads() {
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: None,
            runner_up: false,
        });
        let v: serde_json::Value = serde_json::to_value(&r).expect("serialize");
        assert_eq!(v["ready"], serde_json::Value::Bool(false));
        assert_eq!(v["next"]["command"], "smix init");
        assert_eq!(v["checks"][0]["id"], "simctl");
        assert_eq!(v["checks"][0]["status"], "ok");
    }

    #[test]
    fn the_platform_note_does_not_claim_ios_only() {
        // Android emulators have been drivable since the parity work, and
        // the first command a new user runs was still telling them their
        // platform was unsupported.
        assert!(!PLATFORM_NOTE.contains("iOS Simulator only"));
        assert!(PLATFORM_NOTE.contains("Android"));
        assert!(PLATFORM_NOTE.contains("Physical devices are out of scope"));
    }
}
