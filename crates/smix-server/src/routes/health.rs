use crate::{error::Result, state::AppState, valkey};
use axum::{Json, extract::State};
use serde_json::{Value, json};

pub async fn health(State(mut st): State<AppState>) -> Result<Json<Value>> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&st.pg)
        .await?;
    valkey::ping(&mut st.valkey)
        .await
        .map_err(crate::error::Error::Internal)?;
    Ok(Json(json!({
        "status": "ok",
        "service": "smix-server",
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
