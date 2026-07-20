use anyhow::{Context, Result};
use std::net::SocketAddr;

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
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
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
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
            database_url,
            stream_root,
            store_root,
        })
    }

    pub fn database_url_redacted(&self) -> String {
        url_strip_password(&self.database_url).unwrap_or_else(|| self.database_url.clone())
    }
}

fn url_strip_password(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let (creds, host) = rest.split_once('@')?;
    let user = creds.split_once(':').map(|(u, _)| u).unwrap_or(creds);
    Some(format!("{scheme}://{user}@{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_hides_password() {
        assert_eq!(
            url_strip_password("postgres://u:p@h:5432/d").unwrap(),
            "postgres://u@h:5432/d"
        );
    }
}
