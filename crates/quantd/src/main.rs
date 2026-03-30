use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use api::AppState;
use config::AppConfig;
use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use ingest::{IngestRegistry, MockBarsAdapter};
use longbridge_adapters::{LongbridgeCandleIngest, LongbridgeClients, LongbridgeTradeAdapter};
use pipeline::{run_one_tick_for_venue, RiskLimits, VenueTickParams};
use strategy::AlwaysLongOne;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

fn env_symbol(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn longbridge_env_configured() -> bool {
    let k = std::env::var("LONGBRIDGE_APP_KEY").unwrap_or_default();
    let s = std::env::var("LONGBRIDGE_APP_SECRET").unwrap_or_default();
    let t = std::env::var("LONGBRIDGE_ACCESS_TOKEN").unwrap_or_default();
    !k.is_empty() && !s.is_empty() && !t.is_empty()
}

fn build_ingest_registry(
    lb: Option<&LongbridgeClients>,
    us_symbol: &str,
    hk_symbol: &str,
) -> IngestRegistry {
    let mut registry = IngestRegistry::default();
    if let Some(lb) = lb {
        registry.register(Arc::new(LongbridgeCandleIngest::new(
            lb.quote.clone(),
            Venue::UsEquity,
            us_symbol,
        )));
        registry.register(Arc::new(LongbridgeCandleIngest::new(
            lb.quote.clone(),
            Venue::HkEquity,
            hk_symbol,
        )));
    } else {
        registry.register(Arc::new(MockBarsAdapter::new(
            Venue::UsEquity,
            "mock_us",
        )));
        registry.register(Arc::new(MockBarsAdapter::new(
            Venue::HkEquity,
            "mock_hk",
        )));
    }
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
    let app_config = AppConfig::from_env()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&app_config.log_filter)
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    tracing::info!(
        channel = "quantd",
        database_url = %redact_url(&app_config.database_url),
        http_bind = %app_config.http_bind,
        "starting quantd"
    );

    let database = db::Db::connect(&app_config.database_url).await?;
    let is_prod = app_config.env.eq_ignore_ascii_case("prod");
    if !is_prod || app_config.allow_seed {
        db::ensure_mvp_seed(database.pool()).await?;
    }

    let (event_tx, _event_rx) = broadcast::channel::<api::StreamEvent>(64);

    let lb_clients: Option<LongbridgeClients> = if longbridge_env_configured() {
        match LongbridgeClients::connect() {
            Ok(c) => {
                tracing::info!(channel = "quantd", "longbridge: connected (quote+trade)");
                if let Err(err) = db::ensure_longbridge_live_account(database.pool()).await {
                    tracing::warn!(channel = "quantd", %err, "longbridge: ensure account/profile failed");
                }
                Some(c)
            }
            Err(err) => {
                tracing::warn!(
                    channel = "quantd",
                    %err,
                    "longbridge: connect failed; US/HK ingest falls back to mock"
                );
                None
            }
        }
    } else {
        None
    };

    let us_lb = env_symbol("QUANTD_LB_US_SYMBOL", "AAPL.US");
    let hk_lb = env_symbol("QUANTD_LB_HK_SYMBOL", "700.HK");

    let paper = Arc::new(PaperAdapter::new(database.clone()));
    let mut routes = HashMap::new();
    routes.insert("acc_mvp_paper".to_string(), paper as Arc<dyn ExecutionAdapter>);
    if let Some(ref lb) = lb_clients {
        routes.insert(
            "acc_lb_live".to_string(),
            Arc::new(LongbridgeTradeAdapter::new(lb.trade.clone())) as Arc<dyn ExecutionAdapter>,
        );
    }
    let execution_router = ExecutionRouter::new(routes);

    let ingest_registry = build_ingest_registry(lb_clients.as_ref(), &us_lb, &hk_lb);
    let risk_limits = RiskLimits::from_env();

    let state = AppState {
        database: database.clone(),
        events: event_tx.clone(),
        execution_router: execution_router.clone(),
        ingest_registry: ingest_registry.clone(),
        risk_limits,
        api_key: app_config.api_key.clone(),
    };

    let listener = tokio::net::TcpListener::bind(app_config.http_bind).await?;
    let addr: SocketAddr = app_config.http_bind;
    tracing::info!(channel = "quantd", %addr, "http listening");

    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, api::router(state)).await {
            tracing::error!(channel = "quantd", err = %err, "http server error");
        }
    });

    if !is_prod || app_config.allow_seed {
        run_bootstrap_tick(
            &database,
            &execution_router,
            &ingest_registry,
            risk_limits,
            &event_tx,
        )
        .await?;
    }

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
    risk_limits: RiskLimits,
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
        let ack = run_one_tick_for_venue(
            database,
            adapter.as_ref(),
            router,
            &strategy,
            risk_limits,
            &tick,
        )
        .await?;

        if let Some(ack) = ack {
            let _ = event_tx.send(api::StreamEvent::OrderCreated {
                order_id: ack.order_id,
                venue,
                symbol: tick.symbol.clone(),
            });
        }
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
