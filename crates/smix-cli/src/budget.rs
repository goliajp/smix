//! Whether this machine has room for one more device.
//!
//! Two tasks on one 64 GiB Mac once booted seven simulators — an Apple TV
//! and a Vision Pro among them — and four emulators. Load went to 1000,
//! memory filled, swap was down to 0.6 GiB, and every session and the
//! user's own screen nearly stopped. Each boot had been fine on its own.
//!
//! So a boot first weighs what is already running. Each kind of device
//! counts what it holds idle, measured on this machine's kind of hardware
//! (`phys_footprint` summed over its processes, rounded up, Vision Pro at
//! what a consumer saw with an app open). The sum may use 60% of physical
//! memory by default (`SMIX_DEVICE_BUDGET_GIB` to say otherwise), and a
//! Vision Pro or Apple TV runs alone (`SMIX_HEAVY_DEVICES_ALONE=0` to let
//! them share). Whatever the weights say, a machine already short — swap
//! in use reaching half of physical memory, or under 15% of memory free —
//! boots nothing. Swap *left* says nothing on macOS, which grows swap as
//! it needs it: an idle machine shows 1 GiB left of 5; the one that nearly
//! stopped had 36 GiB in use.
//! Refused, the boot names what is running, whose it is and since when;
//! `--wait` queues instead, for as long as it is given.

use std::process::Command;
use std::time::{Duration, Instant};

/// The kinds of device a boot weighs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    Phone,
    Tablet,
    Tv,
    Vision,
    Watch,
    AndroidPhone,
    AndroidTablet,
}

impl Class {
    /// GiB it holds idle.
    pub(crate) fn weight(self) -> u64 {
        match self {
            Class::Phone | Class::Tablet => 9,
            Class::Tv => 4,
            Class::Vision => 12,
            Class::Watch => 3,
            Class::AndroidPhone => 7,
            Class::AndroidTablet => 8,
        }
    }

    fn runs_alone(self) -> bool {
        match self {
            Class::Tv | Class::Vision => true,
            Class::Phone
            | Class::Tablet
            | Class::Watch
            | Class::AndroidPhone
            | Class::AndroidTablet => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Class::Phone => "iPhone",
            Class::Tablet => "iPad",
            Class::Tv => "Apple TV",
            Class::Vision => "Vision Pro",
            Class::Watch => "Apple Watch",
            Class::AndroidPhone => "Android phone",
            Class::AndroidTablet => "Android tablet",
        }
    }

    /// From a simulator's device type identifier.
    pub(crate) fn of_device_type(id: &str) -> Class {
        if id.contains("Apple-Vision") {
            Class::Vision
        } else if id.contains("Apple-TV") {
            Class::Tv
        } else if id.contains("iPad") {
            Class::Tablet
        } else if id.contains("Watch") {
            Class::Watch
        } else {
            Class::Phone
        }
    }

    /// From an AVD's `config.ini`.
    pub(crate) fn of_avd_config(ini: &str) -> Class {
        let device = ini
            .lines()
            .find_map(|l| {
                l.strip_prefix("hw.device.name")
                    .map(|v| v.trim_start_matches([' ', '=']).to_string())
            })
            .unwrap_or_default()
            .to_lowercase();
        if device.contains("tablet") {
            Class::AndroidTablet
        } else {
            Class::AndroidPhone
        }
    }
}

/// A device that is running.
#[derive(Debug, Clone)]
pub(crate) struct Running {
    pub id: String,
    pub avd: Option<String>,
    pub class: Class,
    pub owner: Option<String>,
    pub since: Option<String>,
}

/// The machine's memory as it stands.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Machine {
    pub total_gib: Option<u64>,
    pub swap_used_mib: Option<u64>,
    pub free_percent: Option<u64>,
}

/// The limits a boot is held to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub budget_gib: u64,
    pub heavy_alone: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    Boot,
    Refuse(String),
}

fn describe(r: &Running) -> String {
    format!(
        "{} {} (owner {}, since {})",
        r.class.name(),
        r.id,
        r.owner.as_deref().unwrap_or("—"),
        r.since.as_deref().unwrap_or("unknown")
    )
}

/// Whether `target` may boot now.
pub(crate) fn decide(target: Class, running: &[Running], m: Machine, l: Limits) -> Verdict {
    let listed = || running.iter().map(describe).collect::<Vec<_>>().join("; ");
    if l.heavy_alone && target.runs_alone() && !running.is_empty() {
        return Verdict::Refuse(format!(
            "a {} runs alone, and {} running: {}",
            target.name(),
            running.len(),
            listed()
        ));
    }
    if l.heavy_alone
        && let Some(alone) = running.iter().find(|r| r.class.runs_alone())
    {
        return Verdict::Refuse(format!("{} is running, and it runs alone", describe(alone)));
    }
    let used: u64 = running.iter().map(|r| r.class.weight()).sum();
    if used + target.weight() > l.budget_gib {
        return Verdict::Refuse(format!(
            "a {} needs about {} GiB; running devices hold about {used} of a {} GiB budget: {}",
            target.name(),
            target.weight(),
            l.budget_gib,
            listed()
        ));
    }
    if let (Some(used), Some(total)) = (m.swap_used_mib, m.total_gib)
        && used >= total * 512
    {
        return Verdict::Refuse(format!("the machine has {used} MiB of swap in use"));
    }
    if let Some(f) = m.free_percent.filter(|f| *f < 15) {
        return Verdict::Refuse(format!("the machine has {f}% of memory free"));
    }
    Verdict::Boot
}

/// The limits from the environment, refusing a value that is not one.
pub(crate) fn limits(total_gib: Option<u64>) -> Result<Limits, String> {
    let budget_gib = match std::env::var("SMIX_DEVICE_BUDGET_GIB") {
        Ok(v) => v.parse::<u64>().ok().filter(|n| *n > 0).ok_or_else(|| {
            format!("SMIX_DEVICE_BUDGET_GIB={v:?}: expected a whole number of GiB")
        })?,
        Err(_) => total_gib.map(|t| t * 6 / 10).ok_or(
            "could not read this machine's memory (sysctl hw.memsize); \
             set SMIX_DEVICE_BUDGET_GIB to say how much devices may use",
        )?,
    };
    let heavy_alone = match std::env::var("SMIX_HEAVY_DEVICES_ALONE").as_deref() {
        Err(_) | Ok("1") => true,
        Ok("0") => false,
        Ok(v) => return Err(format!("SMIX_HEAVY_DEVICES_ALONE={v:?}: expected 0 or 1")),
    };
    Ok(Limits {
        budget_gib,
        heavy_alone,
    })
}

fn sysctl(name: &str) -> Option<String> {
    let out = Command::new("sysctl").args(["-n", name]).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `used = 3977.75M` out of `sysctl vm.swapusage`.
fn parse_swap_used(s: &str) -> Option<u64> {
    let v = s.split("used = ").nth(1)?.split_whitespace().next()?;
    let mib: f64 = v.trim_end_matches('M').parse().ok()?;
    Some(mib as u64)
}

fn parse_free_percent(s: &str) -> Option<u64> {
    s.lines()
        .find_map(|l| l.strip_prefix("System-wide memory free percentage:"))
        .and_then(|v| v.trim().trim_end_matches('%').parse().ok())
}

pub(crate) fn machine() -> Machine {
    let pressure = Command::new("memory_pressure").output().ok();
    Machine {
        total_gib: sysctl("hw.memsize")
            .and_then(|v| v.parse::<u64>().ok())
            .map(|b| b >> 30),
        swap_used_mib: sysctl("vm.swapusage").as_deref().and_then(parse_swap_used),
        free_percent: pressure
            .and_then(|o| parse_free_percent(&String::from_utf8_lossy(&o.stdout))),
    }
}

fn avd_class(avd: &str) -> Class {
    let home = std::env::var("ANDROID_AVD_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".android/avd"))
        });
    // an AVD whose config cannot be read is weighed as the heavier kind
    match home
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join(format!("{avd}.avd/config.ini"))).ok())
    {
        Some(ini) => Class::of_avd_config(&ini),
        None => Class::AndroidTablet,
    }
}

/// Every simulator and emulator running here, with its owner and since
/// when, as the ledgers say.
pub(crate) async fn running() -> Vec<Running> {
    let mut out = Vec::new();
    if let Ok(devices) = smix_simctl::SimctlClient::new().list_devices().await {
        for d in devices.into_iter().filter(|d| d.state == "Booted") {
            out.push(Running {
                class: Class::of_device_type(&d.device_type_identifier),
                id: d.udid,
                avd: None,
                owner: None,
                since: None,
            });
        }
    }
    let ps = Command::new("ps")
        .args(["-A", "-o", "command="])
        .output()
        .ok();
    for line in ps.iter().flat_map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
    }) {
        if !line.contains("qemu-system") {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let after = |flag: &str| {
            words
                .windows(2)
                .find(|w| w[0] == flag)
                .map(|w| w[1].to_string())
        };
        if let (Some(avd), Some(port)) = (after("-avd"), after("-port")) {
            out.push(Running {
                class: avd_class(&avd),
                id: format!("emulator-{port}"),
                avd: Some(avd),
                owner: None,
                since: None,
            });
        }
    }
    if let Ok(leases) = smix_capsule::runner::machine_leases() {
        for r in &mut out {
            r.owner = smix_lease::owners::read(&leases, &r.id)
                .ok()
                .and_then(|o| o.names().first().map(|n| (*n).to_string()));
            r.since = smix_lease::store::read(&leases, &r.id)
                .ok()
                .flatten()
                .map(|l| l.acquired_at);
        }
    }
    out
}

/// What a `sim boot` of `device_ref` would add, or `None` when it adds
/// nothing to weigh: the device is already running, it is a physical
/// device, or nothing resolves it (the boot itself then says so).
async fn target_of(device_ref: &str, running: &[Running]) -> Option<Class> {
    use smix_simctl::registry::DeviceKind;
    let registered = crate::lookup_registered(device_ref);
    match registered.as_ref().map(|s| s.kind) {
        Some(DeviceKind::Emulator) => {
            let avd = registered.as_ref()?.avd_name()?.to_string();
            let up = running
                .iter()
                .any(|r| r.avd.as_deref() == Some(avd.as_str()));
            (!up).then(|| avd_class(&avd))
        }
        Some(DeviceKind::PhysicalIos) | Some(DeviceKind::PhysicalAndroid) => None,
        Some(DeviceKind::Simulator) | None => {
            let udid = registered
                .map(|s| s.udid)
                .unwrap_or_else(|| device_ref.to_string());
            if running.iter().any(|r| r.id == udid) {
                return None;
            }
            let devices = smix_simctl::SimctlClient::new().list_devices().await.ok()?;
            let d = devices.into_iter().find(|d| d.udid == udid)?;
            Some(Class::of_device_type(&d.device_type_identifier))
        }
    }
}

/// The check `smix sim boot` makes first: room for `device_ref`, waiting
/// up to `wait_secs` for it.
pub(crate) async fn check_boot(device_ref: &str, wait_secs: u64) -> Result<(), String> {
    let Some(target) = target_of(device_ref, &running().await).await else {
        return Ok(());
    };
    admit(target, Duration::from_secs(wait_secs)).await
}

/// Hold a boot of `target` until there is room, for up to `wait`.
pub(crate) async fn admit(target: Class, wait: Duration) -> Result<(), String> {
    let deadline = Instant::now() + wait;
    loop {
        // judged by when this look began: a slow look must not count as
        // having waited the whole time and then be refused unseen
        let look = Instant::now();
        let m = machine();
        let verdict = decide(target, &running().await, m, limits(m.total_gib)?);
        match verdict {
            Verdict::Boot => return Ok(()),
            Verdict::Refuse(why) if look >= deadline => {
                return Err(format!(
                    "not booting: {why}.\n  shut one down (`smix status` lists them, \
                     `smix cleanup --owner <task>` takes a task's), or wait with --wait <seconds>"
                ));
            }
            Verdict::Refuse(_) => tokio::time::sleep(Duration::from_secs(10)).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, class: Class, owner: &str) -> Running {
        Running {
            id: id.into(),
            avd: None,
            class,
            owner: Some(owner.into()),
            since: Some("2026-11-03T07:00:00Z".into()),
        }
    }

    const ROOMY: Machine = Machine {
        total_gib: Some(64),
        swap_used_mib: Some(4000),
        free_percent: Some(60),
    };
    const LIMITS: Limits = Limits {
        budget_gib: 38,
        heavy_alone: true,
    };

    #[test]
    fn a_boot_within_the_budget_goes_ahead() {
        let running = [
            run("SIM-1", Class::Phone, "a"),
            run("emulator-5554", Class::AndroidPhone, "b"),
        ];
        assert_eq!(
            decide(Class::Tablet, &running, ROOMY, LIMITS),
            Verdict::Boot
        );
    }

    #[test]
    fn past_the_budget_it_is_refused_naming_what_runs_and_whose() {
        let running = [
            run("SIM-1", Class::Phone, "task-a"),
            run("SIM-2", Class::Tablet, "task-a"),
            run("SIM-3", Class::Phone, "task-b"),
            run("emulator-5554", Class::AndroidPhone, "task-b"),
        ];
        let Verdict::Refuse(why) = decide(Class::Phone, &running, ROOMY, LIMITS) else {
            panic!()
        };
        assert!(why.contains("hold about 34 of a 38 GiB budget"), "{why}");
        assert!(
            why.contains("iPhone SIM-1 (owner task-a, since 2026-11-03T07:00:00Z)"),
            "{why}"
        );
    }

    #[test]
    fn a_vision_pro_or_apple_tv_runs_alone() {
        let one = [run("SIM-1", Class::Phone, "a")];
        assert!(
            matches!(decide(Class::Vision, &one, ROOMY, LIMITS), Verdict::Refuse(w) if w.contains("runs alone"))
        );
        let tv = [run("SIM-TV", Class::Tv, "a")];
        assert!(
            matches!(decide(Class::Phone, &tv, ROOMY, LIMITS), Verdict::Refuse(w) if w.contains("runs alone"))
        );
        let shared = Limits {
            heavy_alone: false,
            ..LIMITS
        };
        assert_eq!(decide(Class::Vision, &one, ROOMY, shared), Verdict::Boot);
    }

    #[test]
    fn a_machine_already_short_boots_nothing() {
        let low_swap = Machine {
            swap_used_mib: Some(36_000),
            ..ROOMY
        };
        assert!(
            matches!(decide(Class::Phone, &[], low_swap, LIMITS), Verdict::Refuse(w) if w.contains("36000 MiB of swap in use"))
        );
        let low_free = Machine {
            free_percent: Some(9),
            ..ROOMY
        };
        assert!(
            matches!(decide(Class::Phone, &[], low_free, LIMITS), Verdict::Refuse(w) if w.contains("9% of memory"))
        );
    }

    #[test]
    fn device_kinds_and_machine_readings_are_parsed() {
        assert_eq!(
            Class::of_device_type("com.apple.CoreSimulator.SimDeviceType.Apple-Vision-Pro-4K"),
            Class::Vision
        );
        assert_eq!(
            Class::of_device_type(
                "com.apple.CoreSimulator.SimDeviceType.Apple-TV-4K-4th-generation-4K"
            ),
            Class::Tv
        );
        assert_eq!(
            Class::of_device_type("com.apple.CoreSimulator.SimDeviceType.iPad-Pro-11-inch-M5-12GB"),
            Class::Tablet
        );
        assert_eq!(
            Class::of_device_type("com.apple.CoreSimulator.SimDeviceType.iPhone-17"),
            Class::Phone
        );
        assert_eq!(
            Class::of_avd_config("hw.device.name = pixel_tablet\nhw.ramSize=4096"),
            Class::AndroidTablet
        );
        assert_eq!(
            Class::of_avd_config("hw.device.name=pixel_9"),
            Class::AndroidPhone
        );
        assert_eq!(
            parse_swap_used("total = 5120.00M  used = 3977.75M  free = 1142.25M  (encrypted)"),
            Some(3977)
        );
        assert_eq!(
            parse_free_percent("...\nSystem-wide memory free percentage: 65%\n"),
            Some(65)
        );
    }
}
