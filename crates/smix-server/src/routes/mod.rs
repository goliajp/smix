pub mod health;

use crate::{capture, state::AppState, stream};
use axum::{
    Router,
    routing::{get, post},
};

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/sims", get(stream::list_sims))
        .route("/capture/start", post(capture::start_capture))
        .route("/capture/stop", post(capture::stop_capture))
        .with_state(state)
}
