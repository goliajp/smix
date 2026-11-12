//! `cleanup --orphans` for settings: a run that changed a device's
//! environment and died before putting it back left the values from
//! before in the device's ledger. With nobody holding the device any
//! more, they are put back from there — the device itself left as it is.

use smix_lease::Resource;

/// Put back the settings left changed on running devices nobody holds.
/// Says each one; returns how many could not be put back.
pub(crate) async fn restore_left_changed(dry_run: bool) -> Result<usize, String> {
    let leases = smix_capsule::runner::machine_leases()?;
    let running = crate::status_cmd::running_ids().await;
    let mut failed = 0;
    for id in leases.device_ids() {
        let Ok(Some(lease)) = smix_lease::store::read(&leases, &id) else {
            continue;
        };
        let Some(changes) = lease.known_resources().find_map(|r| match r {
            Resource::Environment { changes } => Some(changes.clone()),
            _ => None,
        }) else {
            continue;
        };
        let held = smix_lease::store::probe(&lease.holder);
        if (held.pid_exists && held.identity_matches) || !running.contains(&id) {
            continue;
        }
        let names: Vec<&str> = changes.iter().map(|c| c.setting.as_str()).collect();
        let what = format!(
            "{id}: {} left changed by a run that has ended",
            names.join(", ")
        );
        if dry_run {
            println!("would put back {what}");
            continue;
        }
        let action = smix_lease::CleanupAction::RestoreEnvironment {
            device_id: id.clone(),
            changes,
        };
        match smix_capsule::reconcile::execute(&[action]).first() {
            Some(smix_capsule::reconcile::Outcome::Closed(line)) => eprintln!("put back {line}"),
            other => {
                failed += 1;
                eprintln!("could not put back {what}: {other:?}");
            }
        }
    }
    Ok(failed)
}
