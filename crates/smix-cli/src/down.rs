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

/// Run the full sweep. Returns Err with the residue list if smix-shaped
/// processes survive.
pub async fn run(root: &Path, runner_port: u16) -> Result<(), String> {
    println!("=== 1. XCUITest runner ===");
    crate::runner::down(root, runner_port)?;

    println!("=== 2. web demo stack ===");
    pkill("-TERM", "smix/web/node_modules/.bin/vite", "vite");
    pkill("-TERM", "smix-demo-target/debug/smix-server", "smix-server");

    // Gated on the store, not on a legacy file. Checking for
    // `.smix/sims.json` meant a machine that had only ever run the
    // store looked like it had no registered devices, and the
    // orphan-cleanup pass below silently did nothing.
    let smix_dir = root.join(".smix");
    let reg = if smix_dir.is_dir() {
        Some(SimRegistry::load(&smix_dir).map_err(|e| e.to_string())?)
    } else {
        None
    };

    println!("=== 3. orphan recorders / motion loops (registered UDIDs only) ===");
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

    println!("=== 4. shutdown registered sims (per-UDID) ===");
    if let Some(reg) = &reg {
        let simctl = SimctlClient::new();
        let devices = simctl.list_devices().await.map_err(|e| e.to_string())?;
        for (alias, sim) in reg.sims() {
            let booted = devices
                .iter()
                .any(|d| d.udid.eq_ignore_ascii_case(&sim.udid) && d.state == "Booted");
            if booted {
                simctl
                    .shutdown(&sim.udid)
                    .await
                    .map_err(|e| format!("shutdown {alias} ({}): {e}", sim.udid))?;
                println!("  shutdown {alias} ({})", sim.udid);
            }
        }
    } else {
        println!(
            "  no registry at {} — skipping sim shutdown",
            smix_dir.display()
        );
    }

    println!("=== 5. residue report ===");
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
