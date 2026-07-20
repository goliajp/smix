use crate::{error::Result, state::AppState};
use axum::{Json, extract::State};
use serde_json::{Value, json};

pub async fn health(State(st): State<AppState>) -> Result<Json<Value>> {
    // The store replaces the valkey PING: reading the capturing set
    // proves the same thing the ping did — that the place this server
    // keeps its state answers.
    crate::capturing::members(&st.store).map_err(|e| crate::error::Error::Internal(e.into()))?;
    Ok(Json(json!({
        "status": "ok",
        "service": "smix-server",
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
