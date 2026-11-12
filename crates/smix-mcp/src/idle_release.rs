//! The server's half of giving a quiet device back: the watch that
//! notices, the heartbeat that lets a refusal say how quiet, and the
//! release `smix_release` shares.

use std::time::Duration;

use rmcp::ErrorData as McpError;
use smix_sdk::App;

use crate::SmixMcpService;

impl SmixMcpService {
    /// A tool is about to run against the bound device: note it, and keep
    /// the ledger's heartbeat within a minute of it.
    pub(crate) fn note_command(&self, app: &App, udid: &str) {
        self.activity.touch();
        if !app.holds_device_lease() || !self.activity.beat_due() {
            return;
        }
        let beat = smix_capsule::runner::machine_leases()
            .and_then(|leases| smix_lease::beat::beat(&leases, udid).map_err(|e| e.to_string()));
        if let Err(e) = beat {
            eprintln!("warning: device ledger heartbeat not written: {e}");
        }
    }

    /// Take the runner on `port` down and give the device's lease back.
    pub(crate) async fn give_back(&self, app: &mut App, port: u16) -> Result<(), McpError> {
        // An MCP release runs without anyone watching, so `down` ends
        // only the runner on this port: ending another session's runner
        // from here would be the same accident as before, with nobody
        // present to notice.
        tokio::task::spawn_blocking(move || smix_capsule::runner::down(port))
            .await
            .map_err(|e| McpError::internal_error(format!("runner down panicked: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("runner down: {e}"), None))?;
        // The lease is what told everybody else the device was taken;
        // left behind, it keeps the next `smix run` waiting on a session
        // that has already ended.
        if let Err(e) = app.release_device_lease() {
            eprintln!("warning: device lease not released: {e}");
        }
        Ok(())
    }
}

/// Give the bound device back once no command has come for `limit`.
pub(crate) async fn watch(service: SmixMcpService, limit: Duration) {
    let tick = (limit / 4).clamp(Duration::from_secs(1), Duration::from_secs(60));
    loop {
        tokio::time::sleep(tick).await;
        if service.session.current().is_none() || service.activity.idle() < limit {
            continue;
        }
        // A tool in flight holds the app; the next tick will look again.
        let Ok(mut app) = service.app.try_lock() else {
            continue;
        };
        let idle = service.activity.idle();
        if idle < limit {
            continue;
        }
        let Some(bound) = service.session.current() else {
            continue;
        };
        let why = smix_mcp::idle::released_note(&bound.udid, idle);
        service.session.let_go(why.clone());
        match service.give_back(&mut app, bound.port).await {
            Ok(()) => eprintln!("{why}"),
            Err(e) => eprintln!("idle release of {}: {}", bound.udid, e.message),
        }
    }
}
