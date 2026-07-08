use anyhow::{Context, Result};
use std::net::SocketAddr;

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub valkey_url: String,
    pub stream_root: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind: SocketAddr = std::env::var("SMIX_SERVER_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8787".to_string())
            .parse()
            .context("parse SMIX_SERVER_BIND")?;
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
        let valkey_url = std::env::var("REDIS_URL").context("REDIS_URL required")?;
        let stream_root =
            std::env::var("SMIX_STREAM_ROOT").unwrap_or_else(|_| ".smix/stream".to_string());
        Ok(Self {
            bind,
            database_url,
            valkey_url,
            stream_root,
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
