use anyhow::Context;
use smix_server::{app, config::Config, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env().context("load config")?;
    tracing::info!(
        bind = %cfg.bind,
        store = %cfg.store_root,
        stream_root = %cfg.stream_root,
        "starting smix-server"
    );

    // Everything the server remembers is in here: the capturing set and
    // the stream sessions. No database and no valkey to be up first.
    let store = std::sync::Arc::new(
        smix_store::Store::open(std::path::Path::new(&cfg.store_root))
            .context("open smix-server store")?,
    );

    let state = AppState {
        cfg: cfg.clone(),
        store,
        captures: Default::default(),
    };

    let listener = tokio::net::TcpListener::bind(cfg.bind)
        .await
        .context("bind listener")?;
    tracing::info!(bind = %cfg.bind, "listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
