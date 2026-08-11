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
fn settle_ledgers(root: &Path, leases: &smix_lease::store::LeaseDir) {
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
        for outcome in smix_capsule::reconcile::execute(root, &cleanup) {
            println!("  {id}: {}", outcome.line());
        }
        // The whole ledger, not just the process rows. `plan_cleanup`
        // already includes the shutdown for a device this workspace
        // booted, so after this pass nothing on that device is owed —
        // and a ledger kept for a row nobody owes anything on is a file
        // that makes the next `down` look like it has work to do.
        if let Err(e) = smix_lease::store::remove(leases, &id) {
            eprintln!("  {id}: ledger not updated: {e}");
        }
    }
}

/// Run the full sweep. Returns Err with the residue list if smix-shaped
/// processes survive.
pub async fn run(root: &Path, runner_port: u16) -> Result<(), String> {
    let leases = smix_capsule::runner::machine_leases()?;
    println!("=== 1. device ledgers (close what we opened) ===");
    settle_ledgers(root, &leases);

    println!("=== 2. XCUITest runner ===");
    smix_capsule::runner::down(root, runner_port)?;

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
        for (alias, sim) in reg.sims() {
            let booted = devices
                .iter()
                .any(|d| d.udid.eq_ignore_ascii_case(&sim.udid) && d.state == "Booted");
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
            let ours = smix_lease::may_shut_down(ledger.as_ref());
            if !ours {
                println!("  {alias} ({}) is up but not ours — left alone", sim.udid);
                continue;
            }
            {
                simctl
                    .shutdown(&sim.udid)
                    .await
                    .map_err(|e| format!("shutdown {alias} ({}): {e}", sim.udid))?;
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
    let mut residue = String::new();
    for p in &patterns {
        let out = std::process::Command::new("pgrep")
            .args(["-fl", p])
            .output()
            .map_err(|e| format!("pgrep: {e}"))?;
        let hits = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !hits.is_empty() {
            residue.push_str(&hits);
            residue.push('\n');
        }
    }
    if !residue.is_empty() {
        return Err(format!("STILL RUNNING (inspect manually):\n{residue}"));
    }
    println!("clean — no smix residual processes.");
    Ok(())
}
