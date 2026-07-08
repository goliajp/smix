//! `mod stream` — the first feature module of smix-server.
//!
//! Serves per-sim live HLS observability: a REST sim registry (this file)
//! plus a `tower-http` ServeDir mount (wired in `lib::app`) that serves the
//! rolling `index.m3u8 + seg_*.ts` produced by the web-v0.3 c1 recorder.
//! Future capabilities (metrics, control API) attach to the same server as
//! sibling modules — hence the generic crate name, not `-stream-server`.

use crate::{capture::CAPTURING_SET, error::Result, state::AppState, valkey};
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize, sqlx::FromRow)]
pub struct SimEntry {
    pub udid: String,
    pub device_name: String,
    pub runtime: String,
    pub stream_path: String,
    pub started_at: DateTime<Utc>,
    // Live capture state from valkey (not a pg column): true iff this udid is
    // currently a member of the `smix:capturing` set.
    #[sqlx(default)]
    pub capturing: bool,
}

pub async fn list_sims(State(mut st): State<AppState>) -> Result<Json<Vec<SimEntry>>> {
    let mut rows = sqlx::query_as::<_, SimEntry>(
        "SELECT udid, device_name, runtime, stream_path, started_at \
         FROM stream_sessions ORDER BY started_at DESC",
    )
    .fetch_all(&st.pg)
    .await?;

    let capturing = valkey::smembers(&mut st.valkey, CAPTURING_SET)
        .await
        .map_err(crate::error::Error::Internal)?;
    for row in &mut rows {
        row.capturing = capturing.iter().any(|u| u == &row.udid);
    }
    Ok(Json(rows))
}
