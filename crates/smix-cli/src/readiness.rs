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
    /// Whether this workspace's store could still be opened by a smix
    /// built against kevy 3.
    ///
    /// `None` when the question does not apply — no store on disk yet,
    /// or one configured without persistence. Not the same as `false`,
    /// and reporting it as such would tell someone their downgrade path
    /// is gone when it was never a question.
    pub downgradeable: Option<bool>,
    /// Whether the capture server is answering.
    ///
    /// `capsule up` records the session by default and fails outright when
    /// this process is absent — so on a machine without it, the obvious
    /// next command does not work, and a readiness oracle that suggested it
    /// anyway would be sending people into that wall.
    pub capture_server_up: bool,
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
/// emulators.
///
/// §9#1 was amended on 2026-08-06 — a physical device is no longer
/// refused on principle — and this sentence deliberately did **not**
/// change with it. The amendment lifted a ban; it did not add a
/// capability. It changes when the capability lands, not when the
/// charter allows it.
///
/// It landed for both, on 2026-08-06, and each half was measured on the
/// hardware rather than argued from the code:
///
/// * **Physical Android**: `runner up --platform android` reached
///   `/health = 200`, `/tree` returned 106 nodes at the device's true
///   1080×2340, and a normalised tap landed. Nothing in that path
///   branches on emulator-versus-phone; `smix-adb` names its device on
///   every call.
/// * **Physical iOS**: `runner up <phone>` brought the runner up over a
///   self-written usbmux tunnel, `/health` answered through the port
///   forward, the ledger carried both the runner and the forward, and
///   `/tree` came back at 393×852 — the iPhone's own point size, not a
///   simulator's. Teardown closed both in order.
///
/// One caveat stays in the sentence rather than out of it: a locked phone
/// parks `xcodebuild` instead of failing it, so the device has to be
/// unlocked. That is a precondition somebody can act on, which is why it
/// is worth the words.
///
/// The capture gap that used to sit beside it is closed (C20): the runner
/// now serves `GET /screenshot` from `XCUIScreen`, and a real iPhone was
/// photographed at 1178×2556 through it. Nothing about that belongs in
/// this sentence — a greeting lists what a reader must do, not what smix
/// managed to build.
pub const PLATFORM_NOTE: &str = "smix drives iOS Simulators, Android emulators, and — once \
     registered — physical iPhones and Android devices. A physical iPhone must be unlocked.";

/// Judge the facts.
///
/// Checks are ordered by dependency and stop at the first blocked one:
/// reporting "no registry" to someone whose Xcode tools are missing sends
/// them to a command that cannot succeed yet.
pub fn assess(facts: &Facts) -> Readiness {
    let mut checks = Vec::new();

    // First, and outside the chain below, because it is not a step
    // towards driving anything — it is a standing fact about this
    // workspace's store. Reported wherever the chain happens to stop:
    // someone whose Xcode tools are missing still wants to know whether
    // their downgrade path is open, and burying it behind four passing
    // checks would mean they only learn it once they no longer need it.
    //
    // Never blocking. Being past the window is the ordinary state of a
    // store that has been running a while, and a doctor that flagged it
    // would be crying about the passage of time.
    match facts.downgradeable {
        Some(true) => checks.push(Check {
            id: "store",
            status: Status::Ok,
            detail: "the store is still readable by kevy 3 — downgrading smix is an \
                     install away"
                .into(),
        }),
        Some(false) => checks.push(Check {
            id: "store",
            status: Status::Ok,
            detail: "the store has upgraded its log format — going back to an older \
                     smix now needs a keyspace export through a client, not an install"
                .into(),
        }),
        // No store, or one without persistence: the question has no answer,
        // and inventing one would report a loss that never happened.
        None => {}
    }

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
        // Capture is on by default and needs a separate smix-server. Suggest
        // the invocation that works on THIS machine rather than the one that
        // works on a machine with more running on it.
        let (flags, note) = if facts.capture_server_up {
            ("", "")
        } else {
            (
                " --no-capture",
                " (--no-capture because the capture server is not running; \
                 start smix-server first to record the session)",
            )
        };
        return blocked(
            checks,
            format!("smix capsule up {alias} --bundle <your.bundle.id>{flags}"),
            format!(
                "boots the device and starts the runner that carries every tap and query{note}"
            ),
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
            capture_server_up: false,
            downgradeable: None,
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
            capture_server_up: false,
            downgradeable: None,
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
            capture_server_up: false,
            downgradeable: None,
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
            capture_server_up: true,
            downgradeable: None,
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
            capture_server_up: false,
            downgradeable: None,
        });
        assert_eq!(r.next.expect("blocked").command, "smix init");
    }

    #[test]
    fn readiness_serializes_with_the_fields_a_script_reads() {
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: None,
            runner_up: false,
            capture_server_up: false,
            downgradeable: None,
        });
        let v: serde_json::Value = serde_json::to_value(&r).expect("serialize");
        assert_eq!(v["ready"], serde_json::Value::Bool(false));
        assert_eq!(v["next"]["command"], "smix init");
        assert_eq!(v["checks"][0]["id"], "simctl");
        assert_eq!(v["checks"][0]["status"], "ok");
    }

    #[test]
    fn the_suggested_command_works_on_the_machine_it_is_suggested_on() {
        // capsule up records by default and dies when the capture server is
        // absent. Suggesting the bare form on such a machine hands someone a
        // command that fails, which is worse than saying nothing.
        let without = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 1,
                first_alias: Some("dev".into()),
            }),
            runner_up: false,
            capture_server_up: false,
            downgradeable: None,
        });
        let cmd = without.next.expect("blocked").command;
        assert!(cmd.contains("--no-capture"), "got: {cmd}");

        let with = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 1,
                first_alias: Some("dev".into()),
            }),
            runner_up: false,
            capture_server_up: true,
            downgradeable: None,
        });
        assert!(!with.next.expect("blocked").command.contains("--no-capture"));
    }

    #[test]
    fn a_store_still_in_the_downgrade_window_says_so() {
        // The window kevy's AOF format upgrade opens and then closes.
        // Someone deciding whether to keep an escape hatch needs to know
        // which side of it they are on, and doctor is where they look.
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 1,
                first_alias: Some("dev".into()),
            }),
            runner_up: true,
            capture_server_up: true,
            downgradeable: Some(true),
        });
        let store = r
            .checks
            .iter()
            .find(|c| c.id == "store")
            .expect("a store check");
        assert_eq!(store.status, Status::Ok);
        assert!(store.detail.contains("kevy 3"), "got: {}", store.detail);
    }

    #[test]
    fn a_store_past_the_window_says_that_instead() {
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 1,
                first_alias: Some("dev".into()),
            }),
            runner_up: true,
            capture_server_up: true,
            downgradeable: Some(false),
        });
        let store = r
            .checks
            .iter()
            .find(|c| c.id == "store")
            .expect("a store check");
        // Not a failure: being past the window is the normal state after a
        // rewrite, and a doctor that cried about it would be noise.
        assert_eq!(store.status, Status::Ok);
        assert!(
            store.detail.contains("export"),
            "should name the way back: {}",
            store.detail
        );
    }

    #[test]
    fn no_store_is_not_the_same_as_no_way_back() {
        // `None` means the question does not apply. Reporting it as
        // "cannot downgrade" would invent a loss that has not happened.
        let r = assess(&Facts {
            simctl: Some(simctl_ok()),
            registry: Some(RegistryFacts {
                aliases: 1,
                first_alias: Some("dev".into()),
            }),
            runner_up: true,
            capture_server_up: true,
            downgradeable: None,
        });
        assert!(
            r.checks.iter().all(|c| c.id != "store"),
            "with no store there is nothing to report about one"
        );
    }

    #[test]
    fn the_platform_note_claims_only_what_has_been_measured() {
        // The first command a new user runs, so every clause here is a
        // promise. Android emulators have been drivable since the parity
        // work; physical Android was measured on real hardware; physical
        // iPhones have never been seen to take a runner.
        assert!(!PLATFORM_NOTE.contains("iOS Simulator only"));
        assert!(PLATFORM_NOTE.contains("Android"));
        // Both physical halves were driven on real hardware on
        // 2026-08-06, so the note may finally say so — and must carry the
        // one precondition a reader can act on, because a locked phone
        // parks xcodebuild instead of failing it.
        assert!(
            PLATFORM_NOTE.contains("physical iPhones"),
            "{PLATFORM_NOTE}"
        );
        assert!(
            PLATFORM_NOTE.contains("unlocked"),
            "a locked phone stalls rather than fails; the note must say so: {PLATFORM_NOTE}"
        );
        assert!(PLATFORM_NOTE.contains("registered"), "{PLATFORM_NOTE}");
        // The two claims it must never be flattened back into: one was
        // false the day physical Android landed, the other is still false
        // for anyone who has not registered a device.
        assert!(!PLATFORM_NOTE.contains("Physical devices are out of scope"));
        assert!(!PLATFORM_NOTE.contains("Physical devices are supported"));
    }
}
