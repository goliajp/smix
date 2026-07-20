use anyhow::{Context, Result};
use std::net::SocketAddr;

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub stream_root: String,
    /// Where the embedded store persists. Replaces the valkey
    /// connection that used to hold the capturing set.
    pub store_root: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind: SocketAddr = std::env::var("SMIX_SERVER_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8787".to_string())
            .parse()
            .context("parse SMIX_SERVER_BIND")?;
        // Same treatment as REDIS_URL: named as obsolete rather than
        // ignored, so nobody keeps a postgres alive for a server that
        // stopped connecting to it.
        if std::env::var("DATABASE_URL").is_ok() {
            eprintln!(
                "smix-server: DATABASE_URL is set but no longer used — stream \
                 sessions moved into the embedded store \
                 (SMIX_SERVER_STORE_ROOT). The database it points at can be \
                 retired."
            );
        }
        // Said out loud rather than ignored. A deployment that still
        // sets REDIS_URL is describing an architecture this server no
        // longer has, and silently accepting it would leave someone
        // maintaining a valkey nothing talks to.
        if std::env::var("REDIS_URL").is_ok() {
            eprintln!(
                "smix-server: REDIS_URL is set but no longer used — the capturing \
                 set moved into the embedded store (SMIX_SERVER_STORE_ROOT). \
                 The valkey it points at can be retired."
            );
        }
        let stream_root =
            std::env::var("SMIX_STREAM_ROOT").unwrap_or_else(|_| ".smix/stream".to_string());
        let store_root =
            std::env::var("SMIX_SERVER_STORE_ROOT").unwrap_or_else(|_| ".smix/server".to_string());
        Ok(Self {
            bind,
            stream_root,
            store_root,
        })
    }
}
