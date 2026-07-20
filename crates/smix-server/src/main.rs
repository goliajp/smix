use anyhow::Context;
use smix_server::{app, config::Config, db, state::AppState};

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
        db = %cfg.database_url_redacted(),
        stream_root = %cfg.stream_root,
        "starting smix-server"
    );

    let pg = db::connect(&cfg.database_url)
        .await
        .context("connect postgres")?;
    db::run_migrations(&pg).await.context("run migrations")?;

    // The capturing set lives in the embedded store — no valkey to
    // connect to, and nothing to be up before this process can start.
    let store = std::sync::Arc::new(
        smix_store::Store::open(std::path::Path::new(&cfg.store_root))
            .context("open smix-server store")?,
    );

    let state = AppState {
        cfg: cfg.clone(),
        pg,
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
