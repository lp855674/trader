use anyhow::Result;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let config_path =
        std::env::var("TRADER_CONFIG").unwrap_or_else(|_| "configs/backtest/ma_cross.toml".into());
    let app_config = config::AppConfig::from_toml_file(&config_path)?;
    ensure_database_parent(&app_config.database.url)?;
    let db = storage::Db::connect(&app_config.database.url).await?;
    db.migrate().await?;
    let state = api::AppState::new(db, config_path);
    let address = SocketAddr::from(([127, 0, 0, 1], 8080));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "trader-server listening");
    axum::serve(listener, api::router_with_state(state)).await?;
    Ok(())
}

fn ensure_database_parent(database_url: &str) -> Result<()> {
    let Some(path) = database_url.strip_prefix("sqlite:") else {
        return Ok(());
    };
    if path == ":memory:" || path.starts_with(':') {
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
