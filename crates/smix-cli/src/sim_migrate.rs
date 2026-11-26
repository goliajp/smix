//! `smix sim migrate`: fold per-checkout device registries into this
//! machine's.

use std::path::PathBuf;

use smix_simctl::registry::SimRegistry;

use crate::{CliError, registry_path};

/// Fold per-checkout device registries into this machine's.
pub(crate) fn migrate(from: Vec<PathBuf>, dry_run: bool) -> Result<(), CliError> {
    let into = registry_path()?;
    let sources: Vec<PathBuf> = if from.is_empty() {
        let cwd = std::env::current_dir()
            .map_err(|e| CliError::Other(format!("cannot determine cwd: {e}")))?;
        SimRegistry::discover(&cwd).into_iter().collect()
    } else {
        from.iter()
            .map(|p| {
                if p.ends_with(".smix") {
                    p.clone()
                } else {
                    p.join(".smix")
                }
            })
            .collect()
    };
    if sources.is_empty() {
        println!(
            "nothing to migrate — no .smix registry above {}. \
             Name the checkouts to read with --from <dir>.",
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
        );
        return Ok(());
    }
    // Migrating a book into itself would read every row and write it
    // back, which is harmless but reports every device as "already
    // there" and reads like the migration did nothing. Say which one it
    // is instead.
    let sources: Vec<PathBuf> = sources
        .into_iter()
        .filter(|p| {
            if p == &into {
                println!("{} is already the machine registry — skipped", p.display());
                false
            } else {
                true
            }
        })
        .collect();

    // A rehearsal reads the same books and applies the same merge
    // rules; only the write is skipped. Working the answer out a second
    // way would produce a report about the second way.
    let report = if dry_run {
        SimRegistry::migrate_dry_run(&into, &sources)
    } else {
        SimRegistry::migrate(&into, &sources)
    }
    .map_err(|e| CliError::Other(format!("migrate: {e}")))?;

    for (path, why) in &report.unreadable {
        eprintln!("  {} could not be read: {why}", path.display());
    }
    for path in &report.empty {
        println!("  {} held no devices", path.display());
    }
    for alias in &report.added {
        println!("  + {alias}");
    }
    for alias in &report.narrowed {
        println!("  ~ {alias} — destructive consent narrowed to the stricter answer");
    }
    println!(
        "{} device(s) {} in {} ({} already there)",
        report.added.len(),
        if dry_run {
            "would be recorded"
        } else {
            "now recorded"
        },
        into.display(),
        report.unchanged.len()
    );
    if !report.added.is_empty() && !dry_run {
        // The source is deliberately left in place, so the next reader
        // still finds it and still calls it unmigrated. Saying so beats
        // having somebody wonder why the note did not go away.
        println!(
            "the source registries are untouched — delete them yourself once \
             every tree you use has been migrated"
        );
    }
    Ok(())
}
