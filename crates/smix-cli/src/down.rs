//! `smix down` — one-shot teardown of every smix-owned residual process.
//!
//! Scope discipline: kills only processes identifiable as smix's by
//! command text, and shuts down only sims registered in
//! the registered devices, one UDID at a time — never a global verb that
//! would hit sims other projects are using.

use smix_simctl::SimctlClient;
use smix_simctl::registry::SimRegistry;
use std::path::Path;

fn pkill(sig: &str, pattern: &str, label: &str) -> bool {
    let hit = std::process::Command::new("pkill")
        .args([sig, "-f", pattern])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if hit {
        println!("  {sig} {label}");
    }
    hit
}

/// May this teardown turn a device off?
///
/// Two conditions and both must hold. The boot row (`may_shut_down`)
/// says smix started it once; a live holder that is not this process
/// says somebody is on it now. The first alone shut down a consumer's
/// simulator: its ledger carried an old boot row from a session that
/// smix had opened, and their runner was alive on it. The rule was
/// applied to the record and not to the device.
///
/// Pure so both sides can be pinned without a device — the state that
/// bites (our row + their live session) is hard to hold still on a
/// shared machine long enough to observe, and was not observed: two
/// attempts had `down`'s own earlier passes clear the session before
/// this pass could read it.
#[derive(Debug, PartialEq)]
pub enum Teardown {
    Proceed,
    NotOurs,
    OursButHeld { pid: u32 },
}

pub fn teardown_verdict(
    ledger: Option<&smix_lease::Lease>,
    admission: Option<&smix_lease::Admission>,
) -> Teardown {
    if !smix_lease::may_shut_down(ledger) {
        return Teardown::NotOurs;
    }
    if let Some(smix_lease::Admission::Denied(c)) = admission
        && c.holder_alive
    {
        return Teardown::OursButHeld { pid: c.holder.pid };
    }
    Teardown::Proceed
}

/// One process the residue pass matched, and whose it is.
#[derive(Debug, PartialEq)]
pub enum Residue {
    /// Nobody alive holds its device: this sweep's to answer for.
    Ours(String),
    /// Its device is held by a live process that is not this one — the
    /// same run step 1 and step 5 left alone. Named, not counted.
    Held {
        line: String,
        device: String,
        pid: u32,
    },
    /// Its device is in none of this sweep's books (ledger or registry):
    /// nothing here started it or answers for it. Named, not counted.
    Elsewhere { line: String, device: String },
}

/// Sort the residue pass's matches into this sweep's and other people's.
///
/// The pass matches on process text across the whole machine, so it
/// also finds a runner another project has up on its own simulator. That
/// runner surviving is not this teardown failing: `smix down` settled
/// nothing on that device, by the same holder rule as its first and
/// fifth steps. Counting it made `down` fail whenever anyone else on the
/// machine had a runner up (2026-09-26: a consumer's runner on their
/// simulator failed ours).
///
/// `held` maps a device id to the pid of a live holder that is not this
/// process; `known` is every device this sweep's ledger or registry
/// names. A device in neither is not this sweep's either — run against
/// an isolated machine directory, the ledger knows nothing of a runner
/// another project has up on its own simulator. A line names its device as `id=<udid>` (an xcodebuild
/// destination) or as the operand of `simctl io`.
pub fn classify_residue(
    lines: &[String],
    held: &std::collections::HashMap<String, u32>,
    known: &std::collections::HashSet<String>,
) -> Vec<Residue> {
    lines
        .iter()
        .map(|line| {
            let named = device_named_by(line).and_then(|d| {
                held.iter()
                    .find(|(id, _)| id.eq_ignore_ascii_case(d))
                    .map(|(id, pid)| (id.clone(), *pid))
            });
            if let Some((device, pid)) = named {
                return Residue::Held {
                    line: line.clone(),
                    device,
                    pid,
                };
            }
            match device_named_by(line) {
                Some(d) if !known.iter().any(|k| k.eq_ignore_ascii_case(d)) => Residue::Elsewhere {
                    line: line.clone(),
                    device: d.to_string(),
                },
                _ => Residue::Ours(line.clone()),
            }
        })
        .collect()
}

/// The device a matched process line is about, if it says.
fn device_named_by(line: &str) -> Option<&str> {
    let rest = if let Some(at) = line.find("id=") {
        &line[at + 3..]
    } else {
        let at = line.find("simctl io ")?;
        &line[at + "simctl io ".len()..]
    };
    leading_device_id(rest)
}

/// The UDID / serial at the start of `rest`, up to the first character
/// no device id contains.
fn leading_device_id(rest: &str) -> Option<&str> {
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// Devices with a live holder that is not this process.
fn held_by_others(leases: &smix_lease::store::LeaseDir) -> std::collections::HashMap<String, u32> {
    let mine = std::process::id();
    let Ok(entries) = std::fs::read_dir(leases.path()) else {
        return std::collections::HashMap::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let id = e
                .file_name()
                .to_string_lossy()
                .strip_suffix(".json")?
                .to_string();
            let held = smix_lease::store::collect_facts(leases, &id)
                .ok()?
                .existing?;
            let alive = held.holder.pid_exists && held.holder.identity_matches;
            (alive && held.lease.holder.pid != mine).then_some((id, held.lease.holder.pid))
        })
        .collect()
}

/// Close what the ledgers say is open, on every device that has one.
///
/// This is the pass that knows what it is doing. Everything after it
/// matches on process text, which fails in both directions — it kills
/// another project's runner when the pattern is too broad, and misses
/// the same resource when its command line reads differently. Those
/// passes stay as a backstop for what no ledger covers, but they are no
/// longer how smix tears down its own work.
///
/// A device whose session is still alive is left alone and said so:
/// `smix down` is a sweep of *this* operator's leftovers, not a claim
/// on every device on the machine.
fn settle_ledgers(leases: &smix_lease::store::LeaseDir) {
    let Ok(entries) = std::fs::read_dir(leases.path()) else {
        println!("  no device ledgers");
        return;
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".json")
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    if ids.is_empty() {
        println!("  no device ledgers");
        return;
    }
    for id in ids {
        let facts = match smix_lease::store::collect_facts(leases, &id) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("  {id}: ledger unreadable: {e}");
                continue;
            }
        };
        let Some(held) = facts.existing else {
            println!("  {id}: nothing owed");
            continue;
        };
        // A live holder that is not this process is left alone.
        //
        // This used to close every ledger it found, and the reasoning
        // was sound while the ledgers were per checkout: "`down` is
        // this workspace's operator saying close what I started", and a
        // live runner is precisely what they mean. The ledgers are the
        // machine's now, so the directory holds other people's work —
        // on 2026-08-11 it held pid 50057, an `smix-mcp` still serving
        // somebody. "Which directory it is in" was never the question;
        // "is its holder still there" is, and it is answerable.
        let mine = std::process::id();
        let holder_alive = held.holder.pid_exists && held.holder.identity_matches;
        if holder_alive && held.lease.holder.pid != mine {
            println!(
                "  {id}: held by pid {} ({}) — still alive, left alone",
                held.lease.holder.pid, held.lease.holder.cmd
            );
            continue;
        }
        let cleanup = smix_lease::plan_cleanup(&held.lease);
        if cleanup.is_empty() {
            println!("  {id}: nothing owed");
        }
        let mut all_clean = true;
        for outcome in smix_capsule::reconcile::execute(&cleanup) {
            println!("  {id}: {}", outcome.line());
            all_clean &= outcome.is_clean();
        }
        // The whole ledger, not just the process rows — but only when the
        // closes actually closed. `plan_cleanup` includes the shutdown for
        // a device this workspace booted, so after a clean pass nothing on
        // that device is owed, and a ledger kept for a row nobody owes
        // anything on makes the next `down` look like it has work to do.
        //
        // When a close failed, the opposite is true and the ledger is the
        // only thing that still knows: a shutdown that did not happen
        // leaves the device running, and deleting the row that says smix
        // turned it on loses that fact for good. `lease_cmd.rs`'s
        // reconcile has always drawn this line — "the next command must
        // still see what did not close" — and this path did not, which
        // meant the same situation was judged two different ways
        // depending on which verb you reached for.
        if all_clean {
            if let Err(e) = smix_lease::store::remove(leases, &id) {
                eprintln!("  {id}: ledger not updated: {e}");
            }
        } else {
            println!("  {id}: some closes failed — ledger kept so they stay visible");
        }
    }
}

/// Run the full sweep. Returns Err with the residue list if smix-shaped
/// processes this sweep answers for survive; one on a device somebody
/// else holds alive is named and not counted (`classify_residue`).
pub async fn run(root: &Path, runner_port: u16) -> Result<(), String> {
    let leases = smix_capsule::runner::machine_leases()?;
    println!("=== 1. device ledgers (close what we opened) ===");
    settle_ledgers(&leases);

    println!("=== 2. XCUITest runner ===");
    smix_capsule::runner::down(runner_port)?;

    println!("=== 3. web demo stack ===");
    pkill("-TERM", "smix/web/node_modules/.bin/vite", "vite");
    pkill("-TERM", "smix-demo-target/debug/smix-server", "smix-server");

    // Gated on the store, not on a legacy file. Checking for
    // `.smix/sims.json` meant a machine that had only ever run the
    // store looked like it had no registered devices, and the
    // orphan-cleanup pass below silently did nothing.
    // Every device this machine has registered, not this tree's copy of
    // the list. Which devices to *consider* and which this workspace may
    // turn off are separate questions, and the second one is answered
    // below by the boot ledger — so widening the first only widens what
    // gets looked at. Narrowing it is what let a device booted from here
    // survive a teardown run from a sibling checkout.
    let smix_dir = root.join(".smix");
    let reg = {
        let merged = SimRegistry::open_all(root);
        (!merged.registry.sims().is_empty()).then_some(merged.registry)
    };

    println!("=== 4. orphan recorders / motion loops (registered UDIDs only) ===");
    // Other projects run recordVideo / app-cycling loops on their own sims
    // too — a bare pattern would kill theirs. Scope by registered UDID.
    if let Some(reg) = &reg {
        for sim in reg.sims().values() {
            pkill(
                "-INT",
                &format!("simctl io.*{}.*recordVideo", sim.udid),
                &format!("recordVideo ({})", sim.device_name),
            );
            pkill(
                "-TERM",
                &format!("simctl launch.*{}.*com.apple.Preferences", sim.udid),
                &format!("motion loop ({})", sim.device_name),
            );
        }
    }

    println!("=== 5. shutdown registered sims (per-UDID) ===");
    if let Some(reg) = &reg {
        let simctl = SimctlClient::new();
        let devices = simctl.list_devices().await.map_err(|e| e.to_string())?;
        // Emulators are not in simctl's list, and this pass used to
        // ask only simctl — so a registered emulator was skipped as
        // "not booted" every time, and the six smoke scripts that had
        // to stop one did it with a hard-coded `emu kill` on 5554
        // instead. The ownership rule below is the same for both; only
        // the question "is it running" and the verb that stops it
        // differ.
        let adb = smix_adb::AdbClient::new();
        let running_emulators: std::collections::HashSet<String> = adb
            .devices()
            .await
            .map(|list| {
                list.into_iter()
                    .filter(|d| d.state == "device")
                    .map(|d| d.serial)
                    .collect()
            })
            .unwrap_or_default();
        for (alias, sim) in reg.sims() {
            let is_emulator = sim.kind == smix_simctl::registry::DeviceKind::Emulator;
            let booted = if is_emulator {
                running_emulators.contains(&sim.udid)
            } else {
                devices
                    .iter()
                    .any(|d| d.udid.eq_ignore_ascii_case(&sim.udid) && d.state == "Booted")
            };
            if !booted {
                continue;
            }
            // Only devices this workspace booted.
            //
            // Being in the registry means "smix knows how to address
            // it", not "smix may turn it off". Shutting down every
            // registered device took away sessions nobody here started —
            // a developer running `expo start` against a registered
            // simulator lost it to a teardown of somebody else's work.
            // The boot row is the record of who is entitled, and it is
            // the same rule reconcile follows.
            let ledger = smix_lease::store::read(&leases, &sim.udid).ok().flatten();
            let admission = smix_lease::store::collect_facts(&leases, &sim.udid)
                .ok()
                .map(|f| smix_lease::assess(&f));
            match teardown_verdict(ledger.as_ref(), admission.as_ref()) {
                Teardown::NotOurs => {
                    println!("  {alias} ({}) is up but not ours — left alone", sim.udid);
                    continue;
                }
                Teardown::OursButHeld { pid } => {
                    println!(
                        "  {alias} ({}) is ours by boot row but pid {pid} is alive on it — left alone",
                        sim.udid
                    );
                    continue;
                }
                Teardown::Proceed => {}
            }
            {
                if is_emulator {
                    adb.stop_emulator(&sim.udid)
                        .await
                        .map_err(|e| format!("shutdown {alias} ({}): {e}", sim.udid))?;
                } else {
                    simctl
                        .shutdown(&sim.udid)
                        .await
                        .map_err(|e| format!("shutdown {alias} ({}): {e}", sim.udid))?;
                }
                // Once it is off, nobody owes it a shutdown. Leaving the
                // boot row behind would have the next teardown shut down
                // a device this workspace never turned on — and would
                // keep a ledger file alive with nothing left in it worth
                // acting on.
                if let Err(e) = smix_lease::store::drop_resource_kind(
                    &leases,
                    &sim.udid,
                    &smix_lease::Resource::Booted { by_us: true },
                ) {
                    eprintln!("  {}: boot row not cleared: {e}", sim.udid);
                }
                println!("  shutdown {alias} ({})", sim.udid);
            }
        }
    } else {
        println!(
            "  no registry at {} — skipping sim shutdown",
            smix_dir.display()
        );
    }

    println!("=== 6. residue report ===");
    let mut patterns = vec!["xcodebuild.*Smix|smix-server|smix/web.*vite".to_string()];
    if let Some(reg) = &reg {
        for sim in reg.sims().values() {
            patterns.push(format!("simctl io.*{}.*recordVideo", sim.udid));
        }
    }
    let mut lines: Vec<String> = Vec::new();
    for p in &patterns {
        let out = std::process::Command::new("pgrep")
            .args(["-fl", p])
            .output()
            .map_err(|e| format!("pgrep: {e}"))?;
        lines.extend(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string),
        );
    }
    let mut residue = String::new();
    let mut known: std::collections::HashSet<String> = std::fs::read_dir(leases.path())
        .map(|it| {
            it.flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .strip_suffix(".json")
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(reg) = &reg {
        known.extend(reg.sims().values().map(|s| s.udid.clone()));
    }
    for r in classify_residue(&lines, &held_by_others(&leases), &known) {
        match r {
            Residue::Ours(line) => {
                residue.push_str(&line);
                residue.push('\n');
            }
            Residue::Held { line, device, pid } => {
                println!("  not this sweep's: {device} is held by live pid {pid} — {line}");
            }
            Residue::Elsewhere { line, device } => {
                println!("  not this sweep's: {device} is in none of its books — {line}");
            }
        }
    }
    if !residue.is_empty() {
        return Err(format!("STILL RUNNING (inspect manually):\n{residue}"));
    }
    println!("clean — no smix residual processes.");
    Ok(())
}

#[cfg(test)]
mod teardown_verdict_tests {
    use super::*;
    use smix_lease::{Admission, Contention, Lease, ProcIdentity, Resource, Row};

    fn proc(pid: u32) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: String::new(),
            cmd: String::new(),
        }
    }

    fn ours() -> Lease {
        Lease {
            device_id: "emulator-5560".into(),
            holder: proc(1),
            acquired_at: String::new(),
            heartbeat_at: String::new(),
            resources: vec![Row::Known(Resource::Booted { by_us: true })],
        }
    }

    fn held_by(pid: u32) -> Admission {
        Admission::Denied(Contention {
            holder: proc(pid),
            acquired_at: String::new(),
            holder_alive: true,
        })
    }

    #[test]
    fn a_device_without_our_boot_row_is_not_ours() {
        assert_eq!(teardown_verdict(None, None), Teardown::NotOurs);
    }

    /// The case that bit. Our boot row, and somebody's session alive on
    /// top of it: the row is history and the session is now.
    #[test]
    fn our_boot_row_does_not_outrank_a_live_session() {
        let l = ours();
        assert_eq!(
            teardown_verdict(Some(&l), Some(&held_by(42098))),
            Teardown::OursButHeld { pid: 42098 }
        );
    }

    #[test]
    fn our_boot_row_and_nobody_on_it_proceeds() {
        let l = ours();
        assert_eq!(
            teardown_verdict(Some(&l), Some(&Admission::Granted)),
            Teardown::Proceed
        );
    }
}

#[cfg(test)]
mod residue_tests {
    use super::*;
    use std::collections::HashMap;

    const THEIRS: &str = "FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1";
    const OURS: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";

    fn known() -> std::collections::HashSet<String> {
        [THEIRS.to_string(), OURS.to_string()].into()
    }

    /// Run against an isolated machine directory, `down` knows nothing of
    /// a runner another project has up: not held in its books, and not in
    /// them at all.
    #[test]
    fn a_runner_on_a_device_outside_this_sweeps_books_is_elsewhere() {
        let out = classify_residue(
            &[runner(THEIRS)],
            &HashMap::new(),
            &[OURS.to_string()].into(),
        );
        assert_eq!(
            out,
            vec![Residue::Elsewhere {
                line: runner(THEIRS),
                device: THEIRS.into()
            }]
        );
    }

    fn runner(udid: &str) -> String {
        format!(
            "47915 /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild test \
             -project /x/SmixRunner.xcodeproj -scheme SmixRunner \
             -destination platform=iOS Simulator,id={udid} -derivedDataPath .smix"
        )
    }

    /// The case that failed `down`: somebody else's runner, alive on a
    /// device they hold.
    #[test]
    fn a_runner_on_a_device_someone_else_holds_is_theirs() {
        let held = HashMap::from([(THEIRS.to_string(), 812)]);
        let out = classify_residue(&[runner(THEIRS)], &held, &known());
        assert_eq!(
            out,
            vec![Residue::Held {
                line: runner(THEIRS),
                device: THEIRS.into(),
                pid: 812
            }]
        );
    }

    #[test]
    fn a_runner_on_a_device_nobody_holds_is_ours() {
        let held = HashMap::from([(THEIRS.to_string(), 812)]);
        let out = classify_residue(&[runner(OURS)], &held, &known());
        assert_eq!(out, vec![Residue::Ours(runner(OURS))]);
    }

    /// A line that names no device cannot be anyone else's by this rule.
    #[test]
    fn a_line_that_names_no_device_is_ours() {
        let line = "999 /x/smix-demo-target/debug/smix-server".to_string();
        let held = HashMap::from([(THEIRS.to_string(), 812)]);
        assert_eq!(
            classify_residue(std::slice::from_ref(&line), &held, &known()),
            vec![Residue::Ours(line)]
        );
    }

    #[test]
    fn a_recorder_names_its_device_as_the_simctl_io_operand() {
        let line = format!("321 xcrun simctl io {THEIRS} recordVideo /tmp/a.mov");
        let held = HashMap::from([(THEIRS.to_string(), 812)]);
        assert_eq!(
            classify_residue(std::slice::from_ref(&line), &held, &known()),
            vec![Residue::Held {
                line,
                device: THEIRS.into(),
                pid: 812
            }]
        );
    }
}
