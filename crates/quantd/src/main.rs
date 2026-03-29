use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use api::AppState;
use config::AppConfig;
use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use ingest::{IngestRegistry, MockBarsAdapter};
use pipeline::{run_one_tick_for_venue, VenueTickParams};
use strategy::AlwaysLongOne;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

fn build_ingest_registry() -> IngestRegistry {
    let mut registry = IngestRegistry::default();
    registry.register(Arc::new(MockBarsAdapter::new(
        Venue::UsEquity,
        "mock_us",
    )));
    registry.register(Arc::new(MockBarsAdapter::new(
        Venue::HkEquity,
        "mock_hk",
    )));
    registry.register(Arc::new(MockBarsAdapter::new(
        Venue::Crypto,
        "mock_crypto",
    )));
    registry.register(Arc::new(MockBarsAdapter::new(
        Venue::Polymarket,
        "mock_poly",
    )));
    registry
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let app_config = AppConfig::from_env()?;
    tracing::info!(
        channel = "quantd",
        database_url = %redact_url(&app_config.database_url),
        http_bind = %app_config.http_bind,
        "starting quantd"
    );

    let database = db::Db::connect(&app_config.database_url).await?;
    db::ensure_mvp_seed(database.pool()).await?;

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(64);

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert("acc_mvp_paper".to_string(), paper as Arc<dyn ExecutionAdapter>);
    let execution_router = ExecutionRouter::new(routes);

    let ingest_registry = build_ingest_registry();

    let state = AppState {
        database: database.clone(),
        events: event_tx.clone(),
        execution_router: execution_router.clone(),
        ingest_registry: ingest_registry.clone(),
    };

    let listener = tokio::net::TcpListener::bind(app_config.http_bind).await?;
    let addr: SocketAddr = app_config.http_bind;
    tracing::info!(channel = "quantd", %addr, "http listening");

    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, api::router(state)).await {
            tracing::error!(channel = "quantd", err = %err, "http server error");
        }
    });

    run_bootstrap_tick(
        &database,
        &execution_router,
        &ingest_registry,
        &event_tx,
    )
    .await?;

    server.await?;
    Ok(())
}

fn redact_url(url: &str) -> String {
    if url.starts_with("sqlite:") {
        return "sqlite:***".to_string();
    }
    "***".to_string()
}

async fn run_bootstrap_tick(
    database: &db::Db,
    router: &ExecutionRouter,
    registry: &IngestRegistry,
    event_tx: &broadcast::Sender<api::StreamEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let strategy = AlwaysLongOne;
    let ts_ms = chrono_like_now_ms();

    for venue in [
        Venue::UsEquity,
        Venue::HkEquity,
        Venue::Crypto,
        Venue::Polymarket,
    ] {
        let adapter = registry.adapter_for_venue(venue).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing ingest adapter for venue",
            )
        })?;
        let tick = VenueTickParams {
            account_id: "acc_mvp_paper".to_string(),
            venue,
            symbol: "MVP".to_string(),
            ts_ms,
        };
        run_one_tick_for_venue(
            database,
            adapter.as_ref(),
            router,
            &strategy,
            &tick,
        )
        .await?;

        let _ = event_tx.send(api::StreamEvent::OrderCycleDone { venue });
    }

    Ok(())
}

fn chrono_like_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
