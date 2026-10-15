//! `smix sim crashes`: the reports a device recorded, and which process
//! keeps crashing.

use smix_adb::{ENDED_AT_SHUTDOWN, crash_loops};
use smix_sdk::device_control::CrashReport;

use crate::CliError;

pub(crate) fn print(
    all: &[CrashReport],
    app: Option<&str>,
    device: &str,
    json: bool,
) -> Result<(), CliError> {
    let shown: Vec<&CrashReport> = match app {
        Some(name) => all.iter().filter(|r| r.process.contains(name)).collect(),
        None => all.iter().collect(),
    };
    if json {
        let rows: Vec<_> = shown
            .iter()
            .map(|r| {
                serde_json::json!({
                    "when": r.when,
                    "process": r.process,
                    "summary": r.summary,
                    "lines": r.lines,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).map_err(|e| CliError::Other(e.to_string()))?
        );
        return Ok(());
    }
    for report in &shown {
        println!("{} {} — {}", report.when, report.process, report.summary);
        for line in &report.lines {
            println!("  {line}");
        }
    }
    // How many were filtered out is the evidence that reads apart from an
    // empty buffer: "none of them was yours" and "the device recorded
    // none" are different answers.
    match app {
        Some(name) => println!(
            "crashes: {} of {} report(s) on {device} name {name}",
            shown.len(),
            all.len()
        ),
        None => println!("crashes: {} report(s) on {device}", all.len()),
    }
    let ended = shown
        .iter()
        .filter(|r| r.summary.contains(ENDED_AT_SHUTDOWN))
        .count();
    if ended > 0 {
        println!(
            "crashes: {ended} of them are processes ended at shutdown for not exiting in time ({ENDED_AT_SHUTDOWN}), not crashes"
        );
    }
    for l in crash_loops(&shown) {
        println!(
            "crashes: {} crashed {} times in {} min ({} – {}) on {device} — it keeps being started and crashing again",
            l.process,
            l.count,
            l.span.num_minutes().max(1),
            l.first,
            l.last
        );
    }
    Ok(())
}
