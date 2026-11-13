//! `smix doctor`: whether this machine can drive anything yet, and what
//! to run next.

use smix_simctl::SimctlClient;

use crate::{CliError, fence, load_registry, readiness};

/// Whether the capture server is answering on its default endpoint.
///
/// A plain TCP connect: `capsule up` only needs the process to be there,
/// and asking for a health route would couple doctor to a surface that is
/// not this module's business.
fn capture_server_reachable() -> bool {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let addr: SocketAddr = ([127, 0, 0, 1], 8787).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

pub(crate) async fn cmd_doctor(
    simctl: &SimctlClient,
    json: bool,
    port: u16,
) -> Result<u8, CliError> {
    // Gather, then judge. The judging is `readiness::assess`, a pure
    // function with its ordering under test — which is the only way the
    // "what do I run next" answer stays correct as checks are added.
    let simctl_facts = match simctl.list_runtimes().await {
        Ok(runtimes) => {
            let devices = simctl.list_devices().await.unwrap_or_default();
            Some(readiness::SimctlFacts {
                available_runtimes: runtimes.iter().filter(|r| r.is_available).count(),
                available_devices: devices.iter().filter(|d| d.is_available).count(),
            })
        }
        // Not an error to report and abort on: "simctl does not run" is
        // the most useful thing doctor can say, and it can only say it by
        // continuing.
        Err(_) => None,
    };

    // Every device this machine knows, not every device this tree
    // knows. A doctor that reports on the checkout answers a question
    // nobody asked: the simulators are the machine's.
    let reg = load_registry().registry;
    let registry = (!reg.sims().is_empty()).then(|| readiness::RegistryFacts {
        aliases: reg.sims().len(),
        first_alias: reg.sims().keys().next().cloned(),
    });

    // Existence checked before opening, because `Store::open` creates
    // what it cannot find — and a health check that brings a store into
    // being has changed the thing it was asked to report on. A workspace
    // with no `.smix` yet simply has no store fact.
    let downgradeable = std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(".smix"))
        .filter(|dir| dir.join("kv").is_dir())
        .and_then(|dir| smix_store::Store::open(&dir).ok())
        .and_then(|store| store.downgradeable_to_kevy3());

    let mut verdict = readiness::assess(&readiness::Facts {
        simctl: simctl_facts,
        registry,
        // health-not-a-decider: doctor prints whether a runner is
        // answering. It concludes nothing and drives nothing.
        runner_up: smix_capsule::runner::health_ok(port),
        capture_server_up: capture_server_reachable(),
        downgradeable,
        adb: adb_facts(),
    });
    let code = u8::from(fence::doctor_checks(simctl, &mut verdict.checks).await);
    if json {
        let out = serde_json::to_string_pretty(&verdict)
            .map_err(|e| CliError::Other(format!("serialize: {e}")))?;
        println!("{out}");
        return Ok(code);
    }

    println!("smix doctor");
    println!("============");
    for check in &verdict.checks {
        let mark = match check.status {
            readiness::Status::Ok => "\u{2713}",
            readiness::Status::Blocked => "\u{2717}",
            readiness::Status::Skipped => "\u{2022}",
        };
        println!("{mark} {}", check.detail);
    }
    println!("{}", readiness::PLATFORM_NOTE);
    match &verdict.next {
        Some(next) => {
            println!();
            println!("next: {}", next.command);
            println!("      {}", next.reason);
        }
        None => println!("\nready — nothing left to set up"),
    }
    Ok(code)
}

/// The adb smix runs, as doctor reports it; `None` when there is none.
fn adb_facts() -> Option<readiness::AdbFacts> {
    let (path, source) = smix_adb::adb_path();
    // The file a bare name stands for, which is the thing to know.
    let found = if path.components().count() == 1 {
        std::env::var_os("PATH").and_then(|dirs| {
            std::env::split_paths(&dirs)
                .map(|d| d.join(path))
                .find(|p| p.is_file())
        })
    } else {
        path.is_file().then(|| path.clone())
    };
    found.map(|found| readiness::AdbFacts {
        path: found.display().to_string(),
        source: source.to_string(),
        cannot_start: smix_adb::adb_usable().err(),
    })
}
