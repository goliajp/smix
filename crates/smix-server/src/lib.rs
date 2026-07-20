//! Library facade for `smix-server`.
//!
//! `app(state)` builds the full axum `Router` without binding a listener,
//! so integration tests in `tests/` drive it via `tower::ServiceExt::oneshot`.
//! The binary entrypoint (`src/main.rs`) is a thin wire that loads config,
//! connects pg, runs migrations, then serves `app(state)`.

pub mod capture;
pub mod capturing;
pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;
pub mod stream;

use axum::Router;
use state::AppState;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub fn app(state: AppState) -> Router {
    let stream_root = state.cfg.stream_root.clone();
    Router::new()
        .nest("/api", routes::api_router(state))
        .nest_service("/streams", ServeDir::new(stream_root))
        .layer(TraceLayer::new_for_http())
}
