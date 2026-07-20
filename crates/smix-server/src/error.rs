use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found: {0}")]
    NotFound(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self {
            Error::Internal(e) => {
                tracing::error!(error = %e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
            Error::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Error::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
