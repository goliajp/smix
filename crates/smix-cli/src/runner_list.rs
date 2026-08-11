//! `smix runner list` — what is running here, next to what is written down.
//!
//! The question this exists to answer was asked on 2026-08-11 and could
//! not be: a runner held port 22087, the rule said find its owner before
//! touching it, and the ledger the rule pointed at had no record of it.
//! It was on the books the whole time, in another workspace's books.
//!
//! Neither side alone answers it. The machine's ledgers say what somebody
//! *said* they opened; `lsof` says what is *actually* listening. This
//! prints both, and when only one has something the row says which.
//!
//! It never acts. No process is signalled, no ledger is written, and it
//! always exits 0 — a command whose job is to be run before touching
//! anything must be safe to run, and a non-zero code would make the
//! scripts that most need it afraid to call it.

use smix_capsule::runner_view::{self, Seen};
use smix_lease::store::{self, CheckoutLedgers, LeaseDir};

/// Read both sides and print them.
///
/// # Errors
///
/// Only a ledger that will not parse. A machine with no ledgers and
/// nothing listening is a normal answer, not a failure.
pub fn run(leases: &LeaseDir) -> Result<u8, crate::CliError> {
    let machine = ledgers_in(leases);
    // The tree underfoot, read for one purpose: to say which checkout
    // has a record of a runner this machine's ledgers do not. It names
    // nothing else and decides nothing — the rule since the ledgers
    // moved is that a checkout's book may stop a decision, and here
    // there is no decision to stop.
    let checkout_dir = std::env::current_dir()
        .ok()
        .and_then(|cwd| CheckoutLedgers::discover(&cwd));
    let checkout: Vec<(String, smix_lease::Lease)> = match &checkout_dir {
        Some(c) => c
            .device_ids()
            .into_iter()
            .filter_map(|id| c.read(&id).ok().flatten().map(|l| (id, l)))
            .collect(),
        None => Vec::new(),
    };

    let listeners = runner_view::listeners();
    let rows = runner_view::attribute(&machine, &listeners, &checkout);

    if rows.is_empty() {
        println!(
            "no runner ledgers under {leases}, and nothing on this machine is \
             listening as a smix runner"
        );
        return Ok(0);
    }

    for row in &rows {
        match &row.seen {
            Seen::Both {
                app_pid,
                ledger_session_pid,
                live_session_pid,
            } => {
                println!(
                    ":{:<6} {:<40} both          app pid {app_pid}, ledger session pid \
                     {ledger_session_pid}",
                    row.port, row.device_id
                );
                // Two pids for one runner is the healthy state — the
                // ledger records the `xcodebuild` on this host, the
                // socket belongs to the app inside the simulator. Only
                // say something when the live session is not the one
                // written down.
                match live_session_pid {
                    Some(live) if live != ledger_session_pid => println!(
                        "{:>8} the session driving it now is pid {live}, not the one \
                         recorded",
                        ""
                    ),
                    None => println!(
                        "{:>8} no xcodebuild session here answers for it — it keeps \
                         serving after the session that started it exits",
                        ""
                    ),
                    _ => {}
                }
            }
            Seen::LedgerOnly { ledger_pid } => {
                println!(
                    ":{:<6} {:<40} ledger-only   recorded at pid {ledger_pid}; nothing is \
                     listening there",
                    row.port, row.device_id
                );
            }
            Seen::ProcessOnly {
                app_pid,
                session_pid,
                named_by,
            } => {
                println!(
                    ":{:<6} {:<40} process-only  app pid {app_pid}{}",
                    row.port,
                    row.device_id,
                    match session_pid {
                        Some(p) => format!(", session pid {p}"),
                        None => String::new(),
                    }
                );
                println!("{:>8} this machine's ledgers have no record of it.", "");
                if let (Some(c), Some(_)) = (&checkout_dir, named_by) {
                    println!(
                        "{:>8} {c} names this device — that book is evidence about \
                         where to look, not authority.",
                        ""
                    );
                }
            }
            Seen::NotProbed { ledger_pid, why } => {
                println!(
                    ":{:<6} {:<40} not-probed    recorded at pid {ledger_pid} — {why}",
                    row.port, row.device_id
                );
            }
        }
    }
    Ok(0)
}

/// Every device ledger under `leases`, paired with its device id.
fn ledgers_in(leases: &LeaseDir) -> Vec<(String, smix_lease::Lease)> {
    let Ok(entries) = std::fs::read_dir(leases.path()) else {
        return Vec::new();
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
    ids.into_iter()
        .filter_map(|id| store::read(leases, &id).ok().flatten().map(|l| (id, l)))
        .collect()
}
