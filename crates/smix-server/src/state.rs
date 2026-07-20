use crate::capture::CaptureHandle;
use crate::config::Config;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A slot is claimed (`None`) for the whole duration of the pipeline
/// bring-up, then filled with the handle. Holding the udid's key from
/// the moment the request is accepted is what makes a concurrent start
/// for the same device see "already_started" instead of racing: the
/// bring-up is seconds long (recording probe + ffprobe + fifo
/// handshake), and `CaptureHandle`'s own docs say dropping it does NOT
/// stop the pipeline — so the loser of that race used to leave an
/// ffmpeg encoder and a rolling recordVideo loop orphaned, holding
/// simctl's host-recording lock against the device forever.
pub type CaptureRegistry = Arc<Mutex<HashMap<String, Option<CaptureHandle>>>>;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Config,
    /// Where the server keeps what it must remember. Embedded: there
    /// is no separate process to start before smix-server can run.
    pub store: std::sync::Arc<smix_store::Store>,
    pub captures: CaptureRegistry,
}
