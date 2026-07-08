use crate::capture::CaptureHandle;
use crate::config::Config;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type CaptureRegistry = Arc<Mutex<HashMap<String, CaptureHandle>>>;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Config,
    pub pg: PgPool,
    pub valkey: ConnectionManager,
    pub captures: CaptureRegistry,
}
