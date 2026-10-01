//! `POST /hittable` — whether a touch at an element would reach it.
//!
//! The tree marks an element covered when something a window draws later
//! lies over its centre. A page sheet is such a thing, and so is a
//! container that lets touches through, so before a tap is refused for
//! being covered the runner hit-tests the element itself. iOS only.

use serde::{Deserialize, Serialize};

use crate::{HttpRunnerClient, RunnerTransportError};

#[derive(Serialize)]
struct Req<'a> {
    id: &'a str,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    exists: Option<bool>,
    #[serde(default)]
    hittable: Option<bool>,
}

impl HttpRunnerClient {
    /// `Some(true)` when a touch at the element with `id` would reach it,
    /// `Some(false)` when it would not or the element is gone, `None` when
    /// the runner could not tell.
    pub async fn element_hittable(&self, id: &str) -> Result<Option<bool>, RunnerTransportError> {
        let a: Answer = self.json_post("/hittable", &Req { id }, None).await?;
        Ok(match (a.ok, a.exists, a.hittable) {
            (Some(true), Some(true), Some(h)) => Some(h),
            (Some(true), Some(false), _) => Some(false),
            _ => None,
        })
    }
}
