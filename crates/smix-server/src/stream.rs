//! `mod stream` — the first feature module of smix-server.
//!
//! Serves per-sim live HLS observability: a REST sim registry (this file)
//! plus a `tower-http` ServeDir mount (wired in `lib::app`) that serves the
//! rolling `index.m3u8 + seg_*.ts` produced by the recorder.
//! Future capabilities (metrics, control API) attach to the same server as
//! sibling modules — hence the generic crate name, not `-stream-server`.

use crate::{error::Result, state::AppState};
use axum::{Json, extract::State};

pub use crate::sessions::SimEntry;

/// Record a sim as having a live stream.
///
/// Called when a capture starts, which is the only moment a stream
/// comes into existence — nothing wrote this before, so `list_sims`
/// returned an empty list in every real deployment while capture
/// happily ran.
pub fn register_session(
    store: &smix_store::Store,
    udid: &str,
    device_name: &str,
    runtime: &str,
    stream_path: &str,
) -> Result<()> {
    crate::sessions::register(store, udid, device_name, runtime, stream_path)
        .map_err(|e| crate::error::Error::Internal(e.into()))
}

/// The live view: every recorded stream, newest first, each marked with
/// whether it is capturing right now.
pub async fn list_sims(State(st): State<AppState>) -> Result<Json<Vec<SimEntry>>> {
    let mut rows =
        crate::sessions::list(&st.store).map_err(|e| crate::error::Error::Internal(e.into()))?;
    let capturing = crate::capturing::members(&st.store)
        .map_err(|e| crate::error::Error::Internal(e.into()))?;
    for row in &mut rows {
        row.capturing = capturing.iter().any(|u| u == &row.udid);
    }
    Ok(Json(rows))
}
