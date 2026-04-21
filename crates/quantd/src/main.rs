use api::AppState;
use config::AppConfig;
use domain::Venue;
use exec::{ExecutionAdapter, ExecutionRouter, PaperAdapter};
use ingest::{IngestRegistry, MockBarsAdapter};
use longbridge_adapters::{LongbridgeCandleIngest, LongbridgeClients, LongbridgeTradeAdapter};
use pipeline::RiskLimits;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{self, Duration};
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

async fn build_execution_router_from_db(
    database: &db::Db,
    env_lb: Option<&LongbridgeClients>,
) -> Result<ExecutionRouter, String> {
    let mut routes: HashMap<String, Arc<dyn ExecutionAdapter>> = HashMap::new();
    let paper = Arc::new(PaperAdapter::new(database.clone()));

    let profiles = db::load_execution_profiles(database.pool())
    .await
    .map_err(|error| format!("load execution profiles failed: {error}"))?;
    let accounts = db::load_accounts(database.pool())
        .await
        .map_err(|error| format!("load accounts failed: {error}"))?;
    let profiles_by_id = profiles
        .into_iter()
        .map(|profile| (profile.id.clone(), profile))
        .collect::<HashMap<_, _>>();

    for account in accounts {
        let profile = profiles_by_id.get(&account.execution_profile_id).ok_or_else(|| {
            format!(
                "account {} references missing execution profile {}",
                account.id, account.execution_profile_id
            )
        })?;
        let adapter: Arc<dyn ExecutionAdapter> = match profile.kind.as_str() {
            "paper_sim" => {
                if account.mode != "paper" {
                    return Err(format!(
                        "account {} has mode {} but uses paper_sim profile {}",
                        account.id, account.mode, profile.id
                    ));
                }
                paper.clone()
            }
            "longbridge_live" => {
                if account.mode != "live" {
                    return Err(format!(
                        "account {} has mode {} but uses longbridge_live profile {}",
                        account.id, account.mode, profile.id
                    ));
                }
                build_longbridge_adapter(database, profile, env_lb, "longbridge_live")?
            }
            "longbridge_paper" => {
                if account.mode != "paper" {
                    return Err(format!(
                        "account {} has mode {} but uses longbridge_paper profile {}",
                        account.id, account.mode, profile.id
                    ));
                }
                build_longbridge_adapter(database, profile, env_lb, "longbridge_paper")?
            }
            other => {
                return Err(format!(
                    "account {} uses unsupported execution profile kind {}",
                    account.id, other
                ));
            }
        };
        tracing::info!(
            channel = "quantd",
            account_id = %account.id,
            mode = %account.mode,
            execution_profile_id = %account.execution_profile_id,
            profile_kind = %profile.kind,
            "registered execution route from database"
        );
        routes.insert(account.id, adapter);
    }

    Ok(ExecutionRouter::new(routes))
}

fn build_longbridge_adapter(
    database: &db::Db,
    profile: &db::ExecutionProfileRow,
    env_lb: Option<&LongbridgeClients>,
    source: &str,
) -> Result<Arc<dyn ExecutionAdapter>, String> {
    let config_json = profile
        .config_json
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("execution profile {} ({source}) missing config_json", profile.id))?;
    let creds: serde_json::Value = serde_json::from_str(config_json)
        .map_err(|error| format!("execution profile {} invalid config_json: {error}", profile.id))?;
    let app_key = creds["app_key"].as_str().unwrap_or("");
    let app_secret = creds["app_secret"].as_str().unwrap_or("");
    let access_token = creds["access_token"].as_str().unwrap_or("");
    if app_key.is_empty() || app_secret.is_empty() || access_token.is_empty() {
        return Err(format!(
            "execution profile {} ({source}) has incomplete credentials",
            profile.id
        ));
    }
    if let Some(lb) = env_lb {
        tracing::info!(
            channel = "quantd",
            profile_id = %profile.id,
            profile_kind = %profile.kind,
            source,
            "using connected longbridge trade context"
        );
        return Ok(Arc::new(LongbridgeTradeAdapter::new(
            database.clone(),
            lb.trade.clone(),
        )));
    }
    let clients = LongbridgeClients::connect_with_credentials(app_key, app_secret, access_token)
        .map_err(|error| format!("execution profile {} connect failed: {error}", profile.id))?;
    tracing::info!(
        channel = "quantd",
        profile_id = %profile.id,
        profile_kind = %profile.kind,
        source,
        "connected longbridge trade context from database credentials"
    );
    Ok(Arc::new(LongbridgeTradeAdapter::new(
        database.clone(),
        clients.trade,
    )))
}

async fn build_strategy(database: &db::Db) -> Arc<dyn strategy::Strategy> {
    let account_id =
        std::env::var("QUANTD_ACCOUNT_ID").unwrap_or_else(|_| "acc_lb_paper".to_string());

    // Try loading from system_config DB first
    let cfg_key = format!("strategy.{}", account_id);
    if let Ok(Some(cfg_json)) = db::get_system_config(database.pool(), &cfg_key).await {
        if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&cfg_json) {
            if cfg["type"].as_str() == Some("lstm") {
                let service_url = db::get_system_config(database.pool(), "lstm.service_url")
                    .await
                    .unwrap_or_default()
                    .unwrap_or_else(|| "http://127.0.0.1:8000".to_string());
                let model_type = cfg["model_type"].as_str().unwrap_or("alstm").to_string();
                let lookback = cfg["lookback"].as_i64().unwrap_or(60);
                let buy_threshold = cfg["buy_threshold"].as_f64().unwrap_or(0.6);
                let sell_threshold = cfg["sell_threshold"].as_f64().unwrap_or(-0.6);
                let data_source_id = std::env::var("QUANTD_DATA_SOURCE_ID")
                    .unwrap_or_else(|_| "longbridge".to_string());
                tracing::info!(
                    channel = "quantd",
                    strategy = "lstm",
                    model_type = %model_type,
                    service_url = %service_url,
                    "loaded lstm strategy from system_config"
                );
                return Arc::new(strategy::LstmStrategy::new(
                    service_url,
                    model_type,
                    lookback,
                    buy_threshold,
                    sell_threshold,
                    database.clone(),
                    data_source_id,
                ));
            }
        }
    }

    // Fall back to env var
    match std::env::var("QUANTD_STRATEGY")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "always_long_one" | "mvp" => {
            tracing::info!(
                channel = "quantd",
                strategy = "always_long_one",
                "live strategy"
            );
            Arc::new(strategy::AlwaysLongOne)
        }
        _ => {
            tracing::info!(channel = "quantd", strategy = "noop", "live strategy");
            Arc::new(strategy::NoOpStrategy)
        }
    }
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
        registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::UsEquity)));
        registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::HkEquity)));
    }
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Crypto)));
    registry.register(Arc::new(MockBarsAdapter::paper_bars(Venue::Polymarket)));
    registry
}

fn parse_cycle_venue(value: &str) -> Result<Venue, String> {
    Venue::parse(value).ok_or_else(|| format!("invalid QUANTD_UNIVERSE_LOOP_VENUE: {value}"))
}

fn spawn_universe_loop(
    state: &AppState,
    app_config: &AppConfig,
) -> Result<Option<tokio::task::JoinHandle<()>>, String> {
    if !app_config.universe_loop_enabled {
        return Ok(None);
    }

    let venue = parse_cycle_venue(&app_config.universe_loop_venue)?;
    let database = state.database.clone();
    let execution_router = state.execution_router.clone();
    let ingest_registry = state.ingest_registry.clone();
    let risk_limits = state.risk_limits;
    let strategy = state.strategy.clone();
    let account_id = app_config.universe_loop_account_id.clone();
    let interval_secs = app_config.universe_loop_interval_secs;

    let handle = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(interval_secs));
        interval.tick().await;
        loop {
            interval.tick().await;
            let Some(adapter) = ingest_registry.adapter_for_venue(venue) else {
                tracing::warn!(
                    channel = "quantd",
                    venue = venue.as_str(),
                    error_code = "universe_loop_missing_adapter",
                    "background universe loop skipped"
                );
                continue;
            };
            drop(adapter);
            match quantd::run_background_universe_cycle_once(
                &database,
                &ingest_registry,
                &execution_router,
                strategy.as_ref(),
                risk_limits,
                venue,
                &account_id,
            )
            .await
            {
                Ok(Some(report)) => {
                    tracing::info!(
                        channel = "quantd",
                        venue = %report.venue,
                        account_id = %report.account_id,
                        mode = %report.mode,
                        accepted = report.accepted.len(),
                        placed = report.placed.len(),
                        "background universe cycle completed"
                    );
                }
                Ok(None) => {
                    tracing::warn!(
                        channel = "quantd",
                        venue = venue.as_str(),
                        error_code = "universe_loop_missing_adapter",
                        "background universe loop skipped"
                    );
                }
                Err(pipeline::PipelineError::UnsupportedStrategy) => {
                    tracing::warn!(
                        channel = "quantd",
                        venue = venue.as_str(),
                        error_code = "universe_loop_unsupported_strategy",
                        "background universe loop disabled by strategy capability"
                    );
                    break;
                }
                Err(pipeline::PipelineError::EmptyAllowlist) => {
                    tracing::info!(
                        channel = "quantd",
                        venue = venue.as_str(),
                        "background universe loop skipped (empty allowlist)"
                    );
                }
                Err(error) => {
                    if let Err(mode_err) = quantd::set_runtime_mode(&database, "observe_only").await
                    {
                        tracing::warn!(
                            channel = "quantd",
                            error_code = "runtime_mode_fallback_failed",
                            err = %mode_err,
                            "failed to switch runtime mode after universe loop error"
                        );
                    }
                    tracing::warn!(
                        channel = "quantd",
                        venue = venue.as_str(),
                        error_code = "universe_loop_failed",
                        err = %error,
                        runtime_mode = "observe_only",
                        "background universe loop failed"
                    );
                }
            }
        }
    });
    Ok(Some(handle))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_config = AppConfig::from_env()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&app_config.log_filter).unwrap_or_else(|_| EnvFilter::new("info")),
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
    quantd::init_runtime_defaults(&database).await?;

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
                quantd::set_runtime_mode(&database, "observe_only").await?;
                if let Err(snapshot_err) = quantd::record_reconciliation_failure(
                    &database,
                    "acc_lb_live",
                    "broker_connect_failed",
                )
                .await
                {
                    tracing::warn!(
                        channel = "quantd",
                        error_code = "reconciliation_snapshot_write_failed",
                        err = %snapshot_err,
                        "failed to persist reconciliation fallback"
                    );
                }
                tracing::warn!(
                    channel = "quantd",
                    error_code = "broker_connect_failed",
                    err = %err,
                    runtime_mode = "observe_only",
                    "longbridge: connect failed; US/HK ingest uses synthetic paper bars"
                );
                None
            }
        }
    } else {
        None
    };

    let us_lb = env_symbol("QUANTD_LB_US_SYMBOL", "AAPL.US");
    let hk_lb = env_symbol("QUANTD_LB_HK_SYMBOL", "700.HK");

    let strategy = build_strategy(&database).await;
    let execution_router = build_execution_router_from_db(&database, lb_clients.as_ref())
        .await
        .map_err(std::io::Error::other)?;
    let accounts = db::load_accounts(database.pool()).await.unwrap_or_default();
    for account in accounts {
        let route_status = if execution_router.resolve(&account.id).is_ok() {
            "registered"
        } else {
            "missing"
        };
        tracing::info!(
            channel = "quantd",
            account_id = %account.id,
            mode = %account.mode,
            execution_profile_id = %account.execution_profile_id,
            route_status,
            "account route summary"
        );
    }

    let ingest_registry = build_ingest_registry(lb_clients.as_ref(), &us_lb, &hk_lb);
    let risk_limits = RiskLimits::from_env();

    let state = AppState {
        database: database.clone(),
        events: event_tx.clone(),
        execution_router: execution_router.clone(),
        ingest_registry: ingest_registry.clone(),
        risk_limits,
        strategy,
        api_key: app_config.api_key.clone(),
    };
    let background_loop = spawn_universe_loop(&state, &app_config)?;

    let listener = tokio::net::TcpListener::bind(app_config.http_bind).await?;
    let addr: SocketAddr = app_config.http_bind;
    tracing::info!(channel = "quantd", %addr, "http listening");

    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, api::router(state)).await {
            tracing::error!(channel = "quantd", err = %err, "http server error");
        }
    });

    server.await?;
    if let Some(handle) = background_loop {
        handle.abort();
    }
    Ok(())
}

fn redact_url(url: &str) -> String {
    if url.starts_with("sqlite:") {
        return url.to_string();
    }
    "***".to_string()
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn build_execution_router_loads_paper_account_from_database_profile() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");

        let router = super::build_execution_router_from_db(&database, None)
            .await
            .expect("router");

        assert!(router.resolve("acc_mvp_paper").is_ok());
    }
}
