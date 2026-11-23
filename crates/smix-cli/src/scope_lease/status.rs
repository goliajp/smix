//! `smix lease status --scope`: a scope's holder, queue and recent
//! automatic releases, for a person or as JSON.

use super::{alive, fail, now, scopes};
use smix_lease::scope::{Scope, shown};

pub(super) fn status(name: &str, json: bool) -> Result<u8, crate::CliError> {
    let dir = scopes().map_err(fail)?;
    let names = if name.is_empty() {
        dir.names()
    } else {
        vec![name.to_string()]
    };
    if names.is_empty() && !json {
        println!(
            "lease: no scopes on this machine ({})",
            dir.path().display()
        );
    }
    let mut shown_as_json = Vec::new();
    for n in names {
        let look = |s: &mut Scope| {
            s.settle(now(), &alive);
            s.clone()
        };
        // the listing shows one left without devices, so it can be removed
        let scope = if name.is_empty() {
            dir.update(&n, look)
        } else {
            dir.update_declared(&n, look)
        }
        .map_err(fail)?;
        if json {
            shown_as_json.push(as_json(&n, &scope));
        } else {
            print(&n, &scope);
        }
    }
    if json {
        let out = if name.is_empty() {
            serde_json::Value::Array(shown_as_json)
        } else {
            shown_as_json.remove(0)
        };
        println!("{out:#}");
    }
    Ok(0)
}

fn as_json(name: &str, scope: &Scope) -> serde_json::Value {
    serde_json::json!({
        "scope": name,
        "devices": scope.devices,
        "requireHolder": scope.require_holder,
        // `until` and `pid` are null rather than absent when not given
        "holder": scope.holder.as_ref().map(|h| serde_json::json!({
            "holder": h.holder,
            "since": h.since,
            "lastSeen": h.last_seen,
            "idleSecs": h.idle_secs,
            "until": h.until,
            "pid": h.pid,
        })),
        "queue": scope.queue.iter().map(|w| serde_json::json!({
            "holder": w.holder,
            "since": w.since,
            "waiting": w.waiting,
            "idleSecs": w.idle_secs,
            "until": w.until,
            "pid": w.pid,
        })).collect::<Vec<_>>(),
        "released": scope.released,
    })
}

fn print(n: &str, scope: &Scope) {
    println!(
        "{n}: devices {}",
        if scope.devices.is_empty() {
            "(none declared)".into()
        } else {
            scope.devices.join(", ")
        }
    );
    match &scope.holder {
        Some(h) => {
            let pid = h
                .pid
                .as_ref()
                .map(|p| format!(", ends with pid {}", p.pid))
                .unwrap_or_default();
            println!(
                "  held by {} since {}, last command {}, released after {} min idle{pid}{}",
                h.holder,
                shown(&h.since),
                shown(&h.last_seen),
                h.idle_secs / 60,
                h.until
                    .as_deref()
                    .map(|u| format!(", until {u}"))
                    .unwrap_or_default()
            );
        }
        None if scope.require_holder => println!("  free; driven only by a holder"),
        None => println!("  free"),
    }
    for (i, w) in scope.queue.iter().enumerate() {
        println!(
            "  {}. {} waiting since {} (pid {}){}",
            i + 1,
            w.holder,
            shown(&w.since),
            w.waiting.pid,
            w.until
                .as_deref()
                .map(|u| format!(", until {u}"))
                .unwrap_or_default()
        );
    }
    for r in scope.released.iter().rev().take(3) {
        println!(
            "  released automatically: {} at {} — {}",
            r.holder,
            shown(&r.at),
            r.reason
        );
    }
}
