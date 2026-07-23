//! napi-rs binding of the smix wire client for the TypeScript/Node SDK.
//!
//! The peer of `smix-ffi`'s UniFFI surface: `smix-ffi/src/driving.rs` wraps
//! one `HttpRunnerClient` and exposes it to Swift/Kotlin through UniFFI; this
//! crate wraps the same client and exposes it to Node through napi. Two
//! binding models, one wire client — kept in separate crates so a change to
//! either binding never drags the other's consumers.

use std::sync::Arc;

use napi_derive::napi;
use smix_runner_client::HttpRunnerClient;

#[napi]
pub struct SmixNodeDriver {
    client: Arc<HttpRunnerClient>,
}

#[napi]
impl SmixNodeDriver {
    /// Build a driver aimed at a runner on `port`. Construction connects to
    /// nothing — it only builds the reqwest-backed client — so a never-served
    /// port is fine until the first call. `port` is `u32` because napi has no
    /// native `u16`; the client's port is `u16`, so the boundary narrows here.
    #[napi(constructor)]
    pub fn new(port: u32) -> Self {
        Self {
            client: Arc::new(HttpRunnerClient::new(port as u16)),
        }
    }

    /// Tap the normalized point `(nx, ny)` (each 0..1) and return the hit
    /// chain as JSON — the named elements containing the point, innermost
    /// first, the same `TapAtCoordResult` the wire returns. JSON string
    /// rather than a marshaled struct keeps C1's boundary minimal; the Node
    /// side reads `.chain`. The Arc is cloned before the await so the future
    /// owns its client and never borrows `&self` across napi's runtime.
    #[napi]
    pub async fn tap_at_coord(&self, nx: f64, ny: f64) -> napi::Result<String> {
        let client = Arc::clone(&self.client);
        let result = client
            .tap_at_norm_coord(nx, ny)
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        serde_json::to_string(&result).map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}
