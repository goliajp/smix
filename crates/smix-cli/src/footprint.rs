//! How much memory a running simulator or emulator holds, as the machine
//! counts it.
//!
//! The sum of each process's `phys_footprint` (`footprint -f bytes`): what
//! it holds of its own, shared read-only pages left out. A simulator is
//! two hundred processes over the same system frameworks, so their RSS
//! summed counted those frameworks two hundred times — 27–36 GiB for one
//! idle iPhone, where this reads 8.5 GiB. An emulator is one `qemu`
//! process, the guest's memory inside it.

use std::collections::HashMap;
use std::process::Command;

/// `(pid, ppid, command)` for every process, from `ps`.
fn processes() -> Vec<(u32, u32, String)> {
    let Ok(out) = Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,command="])
        .output()
    else {
        return Vec::new();
    };
    parse_ps(&String::from_utf8_lossy(&out.stdout))
}

fn parse_ps(text: &str) -> Vec<(u32, u32, String)> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let ppid = it.next()?.parse().ok()?;
            Some((pid, ppid, it.collect::<Vec<_>>().join(" ")))
        })
        .collect()
}

/// The pids under a simulator: its `launchd_sim` processes (tvOS runs
/// two) and everything they started.
fn simulator_pids(rows: &[(u32, u32, String)], udid: &str) -> Vec<u32> {
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, ppid, _) in rows {
        kids.entry(*ppid).or_default().push(*pid);
    }
    let mut out = Vec::new();
    let mut stack: Vec<u32> = rows
        .iter()
        .filter(|(_, _, c)| c.contains("launchd_sim") && c.contains(udid))
        .map(|(p, _, _)| *p)
        .collect();
    while let Some(p) = stack.pop() {
        out.push(p);
        stack.extend(kids.get(&p).into_iter().flatten());
    }
    out
}

/// The `qemu` process serving `emulator-<port>`.
fn emulator_pid(rows: &[(u32, u32, String)], serial: &str) -> Option<u32> {
    let port = serial.strip_prefix("emulator-")?;
    rows.iter()
        .find(|(_, _, c)| {
            c.contains("qemu-system")
                && c.split_whitespace()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|w| w == ["-port", port])
        })
        .map(|(p, _, _)| *p)
}

/// The `phys_footprint` lines of `footprint`'s output, summed.
fn sum_footprint(text: &str) -> Option<u64> {
    let vals: Vec<u64> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("phys_footprint:"))
        .filter_map(|v| v.trim().strip_suffix(" B")?.trim().parse().ok())
        .collect();
    (!vals.is_empty()).then(|| vals.iter().sum())
}

fn footprint_of(pids: &[u32]) -> Option<u64> {
    if pids.is_empty() {
        return None;
    }
    let mut cmd = Command::new("footprint");
    cmd.args(["-f", "bytes"]);
    for p in pids {
        cmd.args(["-p", &p.to_string()]);
    }
    let out = cmd.output().ok()?;
    sum_footprint(&String::from_utf8_lossy(&out.stdout))
}

/// Bytes a device holds, `None` when it is not running or cannot be read.
pub(crate) fn of_device(id: &str) -> Option<u64> {
    let rows = processes();
    if smix_simctl::registry::is_emulator_serial(id) {
        footprint_of(&emulator_pid(&rows, id).into_iter().collect::<Vec<_>>())
    } else {
        footprint_of(&simulator_pids(&rows, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "    1     0 /sbin/launchd
  100     1 launchd_sim /Users/u/Library/Developer/CoreSimulator/Devices/AAAA-1/data/var/run/launchd_bootstrap.plist
  101   100 /runtime/SpringBoard
  102   101 /runtime/child
  200     1 launchd_sim /Users/u/Library/Developer/CoreSimulator/Devices/BBBB-2/data/var/run/launchd_bootstrap.plist
  201   200 /runtime/SpringBoard
  300     1 /sdk/qemu-system-aarch64-headless -avd phone -port 5554 -no-window
  301     1 /sdk/qemu-system-aarch64-headless -avd tab -port 5584 -no-window";

    #[test]
    fn a_simulator_is_its_launchd_sim_and_everything_under_it() {
        let rows = parse_ps(PS);
        let mut got = simulator_pids(&rows, "AAAA-1");
        got.sort();
        assert_eq!(got, vec![100, 101, 102]);
        assert!(simulator_pids(&rows, "CCCC-3").is_empty());
    }

    #[test]
    fn an_emulator_is_the_qemu_on_its_port() {
        let rows = parse_ps(PS);
        assert_eq!(emulator_pid(&rows, "emulator-5584"), Some(301));
        assert_eq!(emulator_pid(&rows, "emulator-555"), None);
        assert_eq!(emulator_pid(&rows, "R5CT0000000"), None);
    }

    #[test]
    fn footprints_are_summed_and_nothing_read_is_none() {
        let out = "======= a\n    phys_footprint: 1048576 B\n    phys_footprint_peak: 9 B\n======= b\n    phys_footprint: 1048576 B\n";
        assert_eq!(sum_footprint(out), Some(2 * 1_048_576));
        assert_eq!(sum_footprint("no such process"), None);
    }
}
