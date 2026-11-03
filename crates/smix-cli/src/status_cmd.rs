//! `smix status`: every device running on this machine, and every device
//! smix has a ledger for — whose it is, whether smix turned it on, since
//! when, who holds it now, and how much memory it takes.
//!
//! Several tasks share one machine. Each used to see only what it had
//! started, and two of them together booted eleven simulators and nearly
//! stopped it. This is the one place that shows all of it. A phone shows
//! only if a ledger names it; nothing here sends a device a command.

use std::collections::BTreeMap;
use std::process::ExitCode;

use serde::Serialize;

/// Take `--owner` (or `SMIX_OWNER`) for this command and the processes it
/// starts. A name that is not one is refused here, at the edge, rather
/// than silently left unwritten in the ledger.
pub(crate) fn adopt_owner(owner: Option<&str>) -> Result<(), String> {
    let Some(name) = owner else { return Ok(()) };
    smix_lease::owners::validate(name)?;
    // SAFETY: set right after parsing, before this command starts any task
    // that could read the environment
    unsafe { std::env::set_var(smix_lease::owners::OWNER_ENV, name) };
    Ok(())
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceRow {
    id: String,
    name: Option<String>,
    running: bool,
    owners: BTreeMap<String, String>,
    booted_by_smix: bool,
    since: Option<String>,
    holder: Option<Holder>,
    runner_ports: Vec<u16>,
    memory_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Holder {
    pid: u32,
    alive: bool,
    cmd: String,
}

fn ledger_rows(rows: &mut BTreeMap<String, DeviceRow>) {
    let Ok(leases) = smix_capsule::runner::machine_leases() else {
        return;
    };
    for id in leases.device_ids() {
        let Ok(Some(lease)) = smix_lease::store::read(&leases, &id) else {
            continue;
        };
        let row = rows.entry(id.clone()).or_default();
        row.id = id.clone();
        row.since = Some(lease.acquired_at.clone());
        let held = smix_lease::store::probe(&lease.holder);
        row.holder = Some(Holder {
            pid: lease.holder.pid,
            alive: held.pid_exists && held.identity_matches,
            cmd: lease.holder.cmd.clone(),
        });
        for r in lease.known_resources() {
            match r {
                smix_lease::Resource::Booted { by_us: true } => row.booted_by_smix = true,
                smix_lease::Resource::Runner { port, .. }
                | smix_lease::Resource::AndroidRunner { port, .. } => row.runner_ports.push(*port),
                _ => {}
            }
        }
        if let Ok(owners) = smix_lease::owners::read(&leases, &id) {
            row.owners = owners.rows.into_iter().map(|o| (o.kind, o.owner)).collect();
        }
    }
}

async fn running_ids() -> Vec<String> {
    let mut ids = Vec::new();
    if let Ok(devices) = smix_simctl::SimctlClient::new().list_devices().await {
        ids.extend(
            devices
                .into_iter()
                .filter(|d| d.state == "Booted")
                .map(|d| d.udid),
        );
    }
    if let Ok(devices) = smix_adb::AdbClient::new().devices().await {
        ids.extend(
            devices
                .into_iter()
                .filter(|d| {
                    d.state == "device" && smix_simctl::registry::is_emulator_serial(&d.serial)
                })
                .map(|d| d.serial),
        );
    }
    ids
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / f64::from(1u32 << 30))
}

fn render(rows: &[DeviceRow]) -> String {
    if rows.is_empty() {
        return "no device is running, and smix has no ledger for any".into();
    }
    let mut out = String::new();
    let mut total = 0u64;
    for r in rows {
        total += r.memory_bytes.unwrap_or(0);
        let name = r.name.as_deref().unwrap_or("—");
        let state = if r.running { "running" } else { "off" };
        let owners = if r.owners.is_empty() {
            "owner —".to_string()
        } else {
            let mut names: Vec<&str> = r.owners.values().map(String::as_str).collect();
            names.dedup();
            format!("owner {}", names.join(", "))
        };
        let booted = if r.booted_by_smix {
            "booted by smix"
        } else {
            "not booted by smix"
        };
        out.push_str(&format!("{} ({name}): {state}, {owners}, {booted}", r.id));
        if let Some(since) = &r.since {
            out.push_str(&format!(", ledger since {since}"));
        }
        if !r.runner_ports.is_empty() {
            let ports: Vec<String> = r.runner_ports.iter().map(u16::to_string).collect();
            out.push_str(&format!(", runner on {}", ports.join(", ")));
        }
        if let Some(m) = r.memory_bytes {
            out.push_str(&format!(", {}", gib(m)));
        }
        out.push('\n');
        if let Some(h) = &r.holder {
            let alive = if h.alive { "alive" } else { "gone" };
            out.push_str(&format!("    held by pid {} ({alive}): {}\n", h.pid, h.cmd));
        }
    }
    let running = rows.iter().filter(|r| r.running).count();
    out.push_str(&format!("{running} running, {} in all\n", gib(total)));
    out
}

pub(crate) async fn run(json: bool) -> ExitCode {
    let mut rows: BTreeMap<String, DeviceRow> = BTreeMap::new();
    ledger_rows(&mut rows);
    for id in running_ids().await {
        let row = rows.entry(id.clone()).or_default();
        row.id = id;
        row.running = true;
    }
    let registry = crate::load_registry();
    for row in rows.values_mut() {
        row.name = registry
            .registry
            .lookup(&row.id)
            .map(|s| s.device_name.clone());
        if row.running {
            row.memory_bytes = crate::footprint::of_device(&row.id);
        }
    }
    let rows: Vec<DeviceRow> = rows.into_values().collect();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).expect("rows serialize")
        );
    } else {
        print!("{}", render(&rows));
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_says_whose_it_is_who_holds_it_and_what_it_takes() {
        let mut owners = BTreeMap::new();
        owners.insert("booted".to_string(), "task-a".to_string());
        owners.insert("runner".to_string(), "task-a".to_string());
        let rows = [DeviceRow {
            id: "SIM-1".into(),
            name: Some("sim-smix-02".into()),
            running: true,
            owners,
            booted_by_smix: true,
            since: Some("2026-11-03T06:00:00Z".into()),
            holder: Some(Holder {
                pid: 42,
                alive: false,
                cmd: "smix run a.yaml".into(),
            }),
            runner_ports: vec![22191],
            memory_bytes: Some(9 << 30),
        }];
        let out = render(&rows);
        assert!(
            out.contains("SIM-1 (sim-smix-02): running, owner task-a, booted by smix"),
            "{out}"
        );
        assert!(out.contains("runner on 22191, 9.0 GiB"), "{out}");
        assert!(
            out.contains("held by pid 42 (gone): smix run a.yaml"),
            "{out}"
        );
        assert!(out.contains("1 running, 9.0 GiB in all"), "{out}");
    }

    #[test]
    fn no_owner_is_said_as_none_not_guessed() {
        let rows = [DeviceRow {
            id: "emulator-5554".into(),
            running: true,
            ..Default::default()
        }];
        assert!(render(&rows).contains("owner —"));
        assert!(render(&[]).contains("no device is running"));
    }

    #[test]
    fn an_owner_name_is_checked_at_the_edge() {
        assert!(adopt_owner(Some("two words")).is_err());
        assert!(adopt_owner(None).is_ok());
    }
}
