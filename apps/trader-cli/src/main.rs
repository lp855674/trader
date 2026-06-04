use anyhow::{Context, Result, bail};
use backtest::{BacktestRuntime, BacktestSettings};
use broker::{
    BinanceLimitOrderRequest, BinanceOrderSide, BinanceSpotTestnetAdapter,
    BinanceSpotTestnetSettings, Broker,
};
use clap::{Parser, Subcommand, ValueEnum};
use metrics::{equity_returns, paper_summary};
use paper::{BinancePaperOrderExecutor, PaperRuntime, PaperSettings};
use replay::ReplayRuntime;
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "trader")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Migrate {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    ImportBars {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        output_parquet: Option<String>,
    },
    Backtest {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    PaperRun {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    PaperPreflight {
        #[arg(long, default_value = "configs/backtest/slow-paper.toml")]
        config: String,
    },
    BinancePaperReadonly {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
    },
    BinancePaperTinyOrder {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        side: String,
        #[arg(long)]
        qty: String,
        #[arg(long)]
        price: String,
        #[arg(long)]
        confirm_testnet_order: bool,
    },
    Replay {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Report {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        format: ReportFormat,
        #[arg(long)]
        output: Option<String>,
    },
    CheckConfig {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReportFormat {
    Text,
    Csv,
    Html,
}

struct ReportData {
    run_id: String,
    run_status: String,
    orders: usize,
    fills: usize,
    balances: usize,
    snapshots: usize,
    total_return: String,
    sharpe: String,
    sortino: String,
    max_drawdown: String,
    win_rate: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => println!("initialized"),
        Command::Migrate { config } => {
            let (_, db) = load_db(&config).await?;
            db.migrate().await?;
            println!("migrated");
        }
        Command::ImportBars {
            config,
            output_parquet,
        } => {
            let (app_config, _) = load_db(&config).await?;
            let bars = data::load_bars(&app_config.data.source, &app_config.data.path)?;
            if let Some(output_path) = output_parquet {
                data::write_bars_to_parquet(output_path, &bars)?;
                println!("wrote parquet bars: {}", bars.len());
            } else {
                println!("imported bars: {}", bars.len());
            }
        }
        Command::Backtest { config } => {
            let (app_config, db) = load_db(&config).await?;
            db.migrate().await?;
            insert_event(
                &db,
                &app_config.runtime.run_id,
                "backtest.started",
                &serde_json::json!({ "run_id": &app_config.runtime.run_id }).to_string(),
            )
            .await?;
            let bars = data::load_bars(&app_config.data.source, &app_config.data.path)?;
            let summary = BacktestRuntime::new(db, backtest_settings(&app_config)?)
                .run(bars)
                .await?;
            let (app_config, db) = load_db(&config).await?;
            let payload = serde_json::json!({
                "run_id": &app_config.runtime.run_id,
                "signals": summary.signals,
                "orders": summary.orders
            })
            .to_string();
            insert_event(
                &db,
                &app_config.runtime.run_id,
                "backtest.completed",
                &payload,
            )
            .await?;
            println!(
                "backtest completed: signals={} orders={}",
                summary.signals, summary.orders
            );
        }
        Command::PaperRun { config } => {
            let (app_config, db) = load_db(&config).await?;
            db.migrate().await?;
            let bars = data::load_bars(&app_config.data.source, &app_config.data.path)
                .with_context(|| format!("failed to load bars from {}", app_config.data.path))?;
            let settings = paper_settings(&app_config)?;
            let summary = paper_runtime(&app_config, db, settings)
                .await?
                .run_bars(bars)
                .await?;
            println!(
                "paper completed: signals={} orders={}",
                summary.signals, summary.orders
            );
        }
        Command::PaperPreflight { config } => {
            let (app_config, _) = load_db(&config).await?;
            let settings = paper_settings(&app_config)?;
            if app_config.runtime.mode != config::RuntimeMode::Paper {
                bail!("paper preflight requires runtime.mode = paper");
            }
            if app_config.broker.mode != config::BrokerMode::Paper {
                bail!("paper preflight requires broker.mode = paper");
            }
            let real_broker_connection = paper_real_broker_connection_ready(&app_config)?;
            let bars = data::load_bars(&app_config.data.source, &app_config.data.path)
                .with_context(|| format!("failed to load bars from {}", app_config.data.path))?;
            println!(
                "paper preflight ok: run_id={} strategy={} symbol={} bars={} database={} broker={} broker_mode={} account={} max_order_notional={} max_exposure={} trading_halted={} real_broker_connection={} order_submit_enabled={}",
                settings.run_id,
                settings.strategy_name,
                settings.symbol,
                bars.len(),
                app_config.database.url,
                broker_kind_slug(app_config.broker.kind),
                broker_mode_slug(app_config.broker.mode),
                settings.account_id,
                settings.max_order_notional,
                settings.max_exposure,
                settings.trading_halted,
                real_broker_connection,
                app_config.broker.order_submit_enabled
            );
        }
        Command::BinancePaperReadonly { config } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            if app_config.broker.kind != config::BrokerKind::Binance {
                bail!("binance paper readonly requires broker.kind = binance");
            }
            if app_config.broker.mode != config::BrokerMode::Paper {
                bail!("binance paper readonly requires broker.mode = paper");
            }
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(&app_config)?)?;
            let status = adapter.status().await?;
            let account = adapter
                .account_snapshot(&app_config.paper.account_id)
                .await?;
            println!(
                "binance paper readonly ok: connected={} trading_enabled={} account={} cash={} equity={} margin_used={}",
                status.connected,
                status.trading_enabled,
                account.account_id,
                account.cash,
                account.equity,
                account.margin_used
            );
        }
        Command::BinancePaperTinyOrder {
            config,
            symbol,
            side,
            qty,
            price,
            confirm_testnet_order,
        } => {
            if !confirm_testnet_order {
                bail!("refusing to submit Binance testnet order without --confirm-testnet-order");
            }
            let app_config = config::AppConfig::from_toml_file(&config)?;
            if app_config.broker.kind != config::BrokerKind::Binance {
                bail!("binance paper tiny order requires broker.kind = binance");
            }
            if app_config.broker.mode != config::BrokerMode::Paper {
                bail!("binance paper tiny order requires broker.mode = paper");
            }
            let (_, db) = load_db(&config).await?;
            db.migrate().await?;
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(&app_config)?)?;
            let client_order_id = format!(
                "trader-{}",
                uuid::Uuid::new_v4()
                    .simple()
                    .to_string()
                    .get(..24)
                    .unwrap_or("order")
            );
            let order = BinanceLimitOrderRequest {
                symbol: symbol.clone(),
                side: binance_order_side(&side)?,
                quantity: Decimal::from_str(&qty)?,
                price: Decimal::from_str(&price)?,
                client_order_id,
            };
            let started_at_ms = chrono::Utc::now().timestamp_millis();
            db.insert_strategy_run(storage::NewStrategyRun {
                id: app_config.runtime.run_id.clone(),
                name: "binance_paper_tiny_order".to_string(),
                mode: "paper".to_string(),
                status: "running".to_string(),
                started_at_ms,
                ended_at_ms: None,
                error: None,
                config_json: serde_json::json!({
                    "broker": "binance",
                    "testnet": true,
                    "symbol": symbol,
                    "side": side,
                    "qty": qty,
                    "price": price
                })
                .to_string(),
            })
            .await?;
            insert_event(
                &db,
                &app_config.runtime.run_id,
                "binance.testnet_order.started",
                &serde_json::json!({ "run_id": &app_config.runtime.run_id }).to_string(),
            )
            .await?;
            let placed = adapter.place_limit_order(&order).await?;
            let order_id = format!("{}-binance-{}", app_config.runtime.run_id, placed.order_id);
            let now_ms = chrono::Utc::now().timestamp_millis();
            db.insert_order(storage::NewOrder {
                id: order_id.clone(),
                run_id: app_config.runtime.run_id.clone(),
                client_order_id: order.client_order_id.clone(),
                broker_order_id: Some(placed.order_id.to_string()),
                account_id: app_config.paper.account_id.clone(),
                symbol: symbol.clone(),
                side: side.to_ascii_uppercase(),
                order_type: "LIMIT".to_string(),
                price: Some(price.clone()),
                qty: qty.clone(),
                filled_qty: "0".to_string(),
                status: placed.status.clone(),
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
            })
            .await?;
            let queried = adapter
                .query_binance_order(&symbol, placed.order_id)
                .await?;
            let (final_order_status, cancel_error) =
                match adapter.cancel_binance_order(&symbol, placed.order_id).await {
                    Ok(cancelled) => binance_cancel_outcome(cancelled.status, None),
                    Err(error) => {
                        let refreshed = adapter
                            .query_binance_order(&symbol, placed.order_id)
                            .await
                            .unwrap_or_else(|_| queried.clone());
                        binance_cancel_outcome(refreshed.status, Some(error.to_string()))
                    }
                };
            let trades = adapter.my_trades(&symbol, placed.order_id).await?;
            let trade_filled_qty = trades
                .iter()
                .fold(Decimal::ZERO, |total, trade| total + trade.qty);
            let filled_qty = if trade_filled_qty > Decimal::ZERO {
                trade_filled_qty
            } else {
                queried.executed_qty
            };
            let ended_at_ms = chrono::Utc::now().timestamp_millis();
            for trade in &trades {
                db.insert_fill(storage::NewFill {
                    id: format!(
                        "{}-binance-trade-{}",
                        app_config.runtime.run_id, trade.trade_id
                    ),
                    order_id: order_id.clone(),
                    run_id: app_config.runtime.run_id.clone(),
                    symbol: trade.symbol.clone(),
                    side: side.to_ascii_uppercase(),
                    price: trade.price.to_string(),
                    qty: trade.qty.to_string(),
                    fee: trade.fee.to_string(),
                    ts_ms: trade.ts_ms,
                })
                .await?;
            }
            if !trades.is_empty() {
                let account = adapter
                    .account_snapshot(&app_config.paper.account_id)
                    .await?;
                let all_fills = db.list_fills(&app_config.runtime.run_id).await?;
                let accounting = binance_accounting_records_from_fills(
                    &app_config.runtime.run_id,
                    &app_config.paper.account_id,
                    &app_config.portfolio.base_currency,
                    account.cash,
                    &all_fills,
                    ended_at_ms,
                )?;
                db.upsert_account_balance(accounting.balance).await?;
                if let Some(position) = accounting.position {
                    db.upsert_position(position).await?;
                }
                db.insert_portfolio_snapshot(accounting.snapshot).await?;
            }
            db.update_order_execution_by_broker_id(
                &app_config.runtime.run_id,
                &placed.order_id.to_string(),
                &final_order_status,
                &filled_qty.to_string(),
                ended_at_ms,
            )
            .await?;
            db.update_strategy_run_status(
                &app_config.runtime.run_id,
                "completed",
                Some(ended_at_ms),
                None,
            )
            .await?;
            insert_event(
                &db,
                &app_config.runtime.run_id,
                "binance.testnet_order.completed",
                &serde_json::json!({
                    "run_id": &app_config.runtime.run_id,
                    "order_id": placed.order_id,
                    "placed_status": placed.status,
                    "queried_status": queried.status,
                    "final_status": final_order_status,
                    "cancel_error": cancel_error,
                    "filled_qty": filled_qty.to_string(),
                    "trades": trades.len()
                })
                .to_string(),
            )
            .await?;
            println!(
                "binance paper tiny order ok: symbol={} order_id={} placed_status={} queried_status={} final_status={} filled_qty={} trades={} cancel_error={} client_order_id={}",
                symbol,
                placed.order_id,
                placed.status,
                queried.status,
                final_order_status,
                filled_qty,
                trades.len(),
                cancel_error.as_deref().unwrap_or("none"),
                placed.client_order_id
            );
        }
        Command::Replay { config } => {
            let (app_config, db) = load_db(&config).await?;
            db.migrate().await?;
            let started_at_ms = chrono::Utc::now().timestamp_millis();
            db.insert_strategy_run(storage::NewStrategyRun {
                id: app_config.runtime.run_id.clone(),
                name: app_config.strategy.name.clone(),
                mode: "replay".to_string(),
                status: "running".to_string(),
                started_at_ms,
                ended_at_ms: None,
                error: None,
                config_json: "{}".to_string(),
            })
            .await?;
            insert_event(&db, &app_config.runtime.run_id, "replay.started", "{}").await?;

            let bars = data::load_bars(&app_config.data.source, &app_config.data.path)?;
            let summary = ReplayRuntime::new(100_000).replay_bars(bars).await;
            let ended_at_ms = chrono::Utc::now().timestamp_millis();
            db.update_strategy_run_status(
                &app_config.runtime.run_id,
                "completed",
                Some(ended_at_ms),
                None,
            )
            .await?;
            let payload = serde_json::json!({
                "run_id": app_config.runtime.run_id,
                "bars": summary.bars,
                "speed": summary.speed
            })
            .to_string();
            insert_event(
                &db,
                &app_config.runtime.run_id,
                "replay.completed",
                &payload,
            )
            .await?;
            println!(
                "replay completed: bars={} speed={}",
                summary.bars, summary.speed
            );
        }
        Command::Report {
            config,
            format,
            output,
        } => {
            let (app_config, db) = load_db(&config).await?;
            db.migrate().await?;
            let run_id = &app_config.runtime.run_id;
            let run_status = db
                .get_strategy_run(run_id)
                .await?
                .map(|run| run.status)
                .unwrap_or_else(|| "missing".to_string());
            let orders = db.list_orders(run_id).await?;
            let fills = db.list_fills(run_id).await?;
            let balances = db.list_account_balances(run_id).await?;
            let snapshots = db.list_portfolio_snapshots(run_id).await?;
            let equity = snapshots
                .iter()
                .map(|snapshot| Decimal::from_str(&snapshot.equity))
                .collect::<Result<Vec<_>, _>>()?;
            let returns = equity_returns(&equity);
            let summary = paper_summary(orders.len(), fills.len(), &equity, &returns);
            let report = ReportData {
                run_id: run_id.clone(),
                run_status,
                orders: orders.len(),
                fills: fills.len(),
                balances: balances.len(),
                snapshots: snapshots.len(),
                total_return: summary.total_return,
                sharpe: summary.sharpe,
                sortino: summary.sortino,
                max_drawdown: summary.max_drawdown,
                win_rate: summary.win_rate,
            };
            let rendered = render_report(&report, format);
            if let Some(output_path) = output {
                std::fs::write(&output_path, rendered)?;
                println!("wrote report: {output_path}");
            } else {
                print!("{rendered}");
            }
        }
        Command::CheckConfig { config } => {
            config::AppConfig::from_toml_file(config)?;
            println!("config ok");
        }
    }
    Ok(())
}

fn render_report(report: &ReportData, format: ReportFormat) -> String {
    match format {
        ReportFormat::Text => format!(
            "report: run_id={} status={} orders={} fills={} balances={} snapshots={} total_return={} sharpe={} sortino={} max_drawdown={} win_rate={}\n",
            report.run_id,
            report.run_status,
            report.orders,
            report.fills,
            report.balances,
            report.snapshots,
            report.total_return,
            report.sharpe,
            report.sortino,
            report.max_drawdown,
            report.win_rate
        ),
        ReportFormat::Csv => format!(
            "run_id,status,orders,fills,balances,snapshots,total_return,sharpe,sortino,max_drawdown,win_rate\n{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(&report.run_id),
            csv_escape(&report.run_status),
            report.orders,
            report.fills,
            report.balances,
            report.snapshots,
            csv_escape(&report.total_return),
            csv_escape(&report.sharpe),
            csv_escape(&report.sortino),
            csv_escape(&report.max_drawdown),
            csv_escape(&report.win_rate)
        ),
        ReportFormat::Html => format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Trader Report</title></head><body><h1>Trader Report</h1><table><tbody><tr><th>Run ID</th><td>{}</td></tr><tr><th>Status</th><td>{}</td></tr><tr><th>Orders</th><td>{}</td></tr><tr><th>Fills</th><td>{}</td></tr><tr><th>Balances</th><td>{}</td></tr><tr><th>Snapshots</th><td>{}</td></tr><tr><th>Total Return</th><td>{}</td></tr><tr><th>Sharpe</th><td>{}</td></tr><tr><th>Sortino</th><td>{}</td></tr><tr><th>Max Drawdown</th><td>{}</td></tr><tr><th>Win Rate</th><td>{}</td></tr></tbody></table></body></html>\n",
            html_escape(&report.run_id),
            html_escape(&report.run_status),
            report.orders,
            report.fills,
            report.balances,
            report.snapshots,
            html_escape(&report.total_return),
            html_escape(&report.sharpe),
            html_escape(&report.sortino),
            html_escape(&report.max_drawdown),
            html_escape(&report.win_rate)
        ),
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn broker_kind_slug(kind: config::BrokerKind) -> &'static str {
    match kind {
        config::BrokerKind::Simulated => "simulated",
        config::BrokerKind::Futu => "futu",
        config::BrokerKind::Binance => "binance",
        config::BrokerKind::Okx => "okx",
        config::BrokerKind::InteractiveBrokers => "interactive_brokers",
    }
}

fn broker_mode_slug(mode: config::BrokerMode) -> &'static str {
    match mode {
        config::BrokerMode::Paper => "paper",
        config::BrokerMode::Live => "live",
    }
}

fn binance_testnet_settings(app_config: &config::AppConfig) -> Result<BinanceSpotTestnetSettings> {
    let api_key_env = app_config
        .broker
        .api_key_env
        .as_deref()
        .unwrap_or("BINANCE_TESTNET_API_KEY");
    let secret_key_env = app_config
        .broker
        .secret_key_env
        .as_deref()
        .unwrap_or("BINANCE_TESTNET_SECRET_KEY");
    let api_key = std::env::var(api_key_env)
        .with_context(|| format!("missing Binance testnet API key env {api_key_env}"))?;
    let secret_key = std::env::var(secret_key_env)
        .with_context(|| format!("missing Binance testnet secret key env {secret_key_env}"))?;

    Ok(BinanceSpotTestnetSettings {
        base_url: app_config
            .broker
            .base_url
            .clone()
            .unwrap_or_else(|| "https://testnet.binance.vision/api".to_string()),
        api_key,
        secret_key,
        recv_window_ms: app_config.broker.recv_window_ms.unwrap_or(5000),
    })
}

fn paper_real_broker_connection_ready(app_config: &config::AppConfig) -> Result<bool> {
    match app_config.broker.kind {
        config::BrokerKind::Simulated => Ok(false),
        config::BrokerKind::Binance => {
            let base_url = app_config.broker.base_url.as_deref().unwrap_or_default();
            if !base_url.contains("testnet.binance.vision") {
                bail!("Binance paper preflight requires Spot testnet base_url");
            }
            let api_key_env = app_config
                .broker
                .api_key_env
                .as_deref()
                .unwrap_or("BINANCE_TESTNET_API_KEY");
            let secret_key_env = app_config
                .broker
                .secret_key_env
                .as_deref()
                .unwrap_or("BINANCE_TESTNET_SECRET_KEY");
            std::env::var(api_key_env)
                .with_context(|| format!("missing Binance testnet API key env {api_key_env}"))?;
            std::env::var(secret_key_env).with_context(|| {
                format!("missing Binance testnet secret key env {secret_key_env}")
            })?;
            Ok(true)
        }
        config::BrokerKind::Futu
        | config::BrokerKind::Okx
        | config::BrokerKind::InteractiveBrokers => Ok(false),
    }
}

async fn paper_runtime(
    app_config: &config::AppConfig,
    db: storage::Db,
    settings: PaperSettings,
) -> Result<PaperRuntime> {
    if !app_config.broker.order_submit_enabled {
        return Ok(PaperRuntime::new(db, settings));
    }
    if app_config.runtime.mode != config::RuntimeMode::Paper {
        bail!("broker order submit requires runtime.mode = paper");
    }
    if app_config.broker.mode != config::BrokerMode::Paper {
        bail!("broker order submit requires broker.mode = paper");
    }
    match app_config.broker.kind {
        config::BrokerKind::Binance => {
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(app_config)?)?;
            let account = adapter
                .account_snapshot(&app_config.paper.account_id)
                .await?;
            Ok(PaperRuntime::new_with_executor(
                db,
                settings_with_broker_initial_cash(settings, account.cash),
                Box::new(BinancePaperOrderExecutor::new_with_client_order_prefix(
                    adapter,
                    app_config.runtime.run_id.clone(),
                )),
            ))
        }
        config::BrokerKind::Simulated
        | config::BrokerKind::Futu
        | config::BrokerKind::Okx
        | config::BrokerKind::InteractiveBrokers => {
            bail!("paper-run broker order submit only supports Binance Spot Testnet in this phase")
        }
    }
}

fn settings_with_broker_initial_cash(
    mut settings: PaperSettings,
    broker_cash: Decimal,
) -> PaperSettings {
    settings.initial_cash = broker_cash;
    settings
}

fn binance_order_side(input: &str) -> Result<BinanceOrderSide> {
    match input.to_ascii_lowercase().as_str() {
        "buy" => Ok(BinanceOrderSide::Buy),
        "sell" => Ok(BinanceOrderSide::Sell),
        other => bail!("unsupported Binance order side {other}; expected buy or sell"),
    }
}

fn binance_cancel_outcome(
    final_status: String,
    cancel_error: Option<String>,
) -> (String, Option<String>) {
    (final_status, cancel_error)
}

struct BinanceAccountingRecords {
    balance: storage::NewAccountBalance,
    position: Option<storage::NewPosition>,
    snapshot: storage::NewPortfolioSnapshot,
}

fn binance_accounting_records_from_fills(
    run_id: &str,
    account_id: &str,
    base_currency: &str,
    cash: Decimal,
    fills: &[storage::NewFill],
    updated_at_ms: i64,
) -> Result<BinanceAccountingRecords> {
    let mut symbol = String::new();
    let mut signed_qty = Decimal::ZERO;
    let mut notional = Decimal::ZERO;
    let mut abs_qty = Decimal::ZERO;

    for fill in fills {
        symbol = fill.symbol.clone();
        let qty = Decimal::from_str(&fill.qty)?;
        let price = Decimal::from_str(&fill.price)?;
        if fill.side.eq_ignore_ascii_case("buy") {
            signed_qty += qty;
        } else {
            signed_qty -= qty;
        }
        notional += qty * price;
        abs_qty += qty;
    }

    let avg_price = if abs_qty > Decimal::ZERO {
        notional / abs_qty
    } else {
        Decimal::ZERO
    };
    let market_value = signed_qty * avg_price;

    Ok(BinanceAccountingRecords {
        balance: storage::NewAccountBalance {
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            asset: base_currency.to_string(),
            total: cash.to_string(),
            available: cash.to_string(),
            frozen: Decimal::ZERO.to_string(),
            updated_at_ms,
        },
        position: (!fills.is_empty()).then(|| storage::NewPosition {
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            symbol: symbol.clone(),
            qty: signed_qty.to_string(),
            avg_price: avg_price.to_string(),
            updated_at_ms,
        }),
        snapshot: storage::NewPortfolioSnapshot {
            id: format!("{run_id}-binance-snapshot-{updated_at_ms}"),
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            ts_ms: updated_at_ms,
            cash: cash.to_string(),
            market_value: market_value.to_string(),
            equity: (cash + market_value).to_string(),
            realized_pnl: Decimal::ZERO.to_string(),
            unrealized_pnl: Decimal::ZERO.to_string(),
        },
    })
}

async fn insert_event(
    db: &storage::Db,
    source: &str,
    category: &str,
    payload_json: &str,
) -> Result<()> {
    db.insert_event(storage::NewEventRecord {
        event_id: uuid::Uuid::new_v4().to_string(),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        source: source.to_string(),
        category: category.to_string(),
        payload_json: payload_json.to_string(),
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        binance_accounting_records_from_fills, binance_cancel_outcome, binance_testnet_settings,
        settings_with_broker_initial_cash,
    };
    use rust_decimal_macros::dec;

    #[test]
    fn binance_cancel_error_preserves_refreshed_order_status() {
        let (status, error) = binance_cancel_outcome(
            "FILLED".to_string(),
            Some("broker rejected order: Unknown order sent".to_string()),
        );

        assert_eq!(status, "FILLED");
        assert_eq!(
            error.as_deref(),
            Some("broker rejected order: Unknown order sent")
        );
    }

    #[test]
    fn binance_buy_fills_create_accounting_records() {
        let records = binance_accounting_records_from_fills(
            "run-1",
            "binance-testnet",
            "USDT",
            dec!(9936.17961),
            &[storage::NewFill {
                id: "fill-1".to_string(),
                order_id: "order-1".to_string(),
                run_id: "run-1".to_string(),
                symbol: "BTCUSDT".to_string(),
                side: "BUY".to_string(),
                price: "63820.39".to_string(),
                qty: "0.001".to_string(),
                fee: "0".to_string(),
                ts_ms: 10,
            }],
            11,
        )
        .unwrap();

        assert_eq!(records.balance.total, "9936.17961");
        let position = records.position.unwrap();
        assert_eq!(position.symbol, "BTCUSDT");
        assert_eq!(position.qty, "0.001");
        assert_eq!(position.avg_price, "63820.39");
        assert_eq!(records.snapshot.market_value, "63.82039");
        assert_eq!(records.snapshot.equity, "10000.00000");
    }

    #[test]
    fn binance_accounting_records_accumulate_existing_fills() {
        let fills = vec![
            storage::NewFill {
                id: "fill-1".to_string(),
                order_id: "order-1".to_string(),
                run_id: "run-1".to_string(),
                symbol: "BTCUSDT".to_string(),
                side: "BUY".to_string(),
                price: "63820.39".to_string(),
                qty: "0.001".to_string(),
                fee: "0".to_string(),
                ts_ms: 10,
            },
            storage::NewFill {
                id: "fill-2".to_string(),
                order_id: "order-2".to_string(),
                run_id: "run-1".to_string(),
                symbol: "BTCUSDT".to_string(),
                side: "BUY".to_string(),
                price: "63960".to_string(),
                qty: "0.001".to_string(),
                fee: "0".to_string(),
                ts_ms: 11,
            },
        ];

        let records = binance_accounting_records_from_fills(
            "run-1",
            "binance-testnet",
            "USDT",
            dec!(9808.38961),
            &fills,
            12,
        )
        .unwrap();

        let position = records.position.unwrap();
        assert_eq!(position.qty, "0.002");
        assert_eq!(position.avg_price, "63890.1950");
        assert_eq!(records.snapshot.market_value, "127.7803900");
    }

    #[test]
    fn binance_submit_requires_testnet_credentials() {
        unsafe {
            std::env::remove_var("BINANCE_TESTNET_API_KEY");
            std::env::remove_var("BINANCE_TESTNET_SECRET_KEY");
        }
        let mut config = sample_app_config();
        config.broker.order_submit_enabled = true;

        let error = binance_testnet_settings(&config).unwrap_err();

        assert!(error.to_string().contains("BINANCE_TESTNET_API_KEY"));
    }

    #[test]
    fn binance_submit_uses_broker_cash_as_initial_cash() {
        let mut settings = paper::PaperSettings::sample();
        settings.initial_cash = dec!(100000);

        let settings = settings_with_broker_initial_cash(settings, dec!(9744.45561));

        assert_eq!(settings.initial_cash, dec!(9744.45561));
    }

    fn sample_app_config() -> config::AppConfig {
        config::AppConfig::from_toml_str(
            r#"
            [runtime]
            mode = "paper"
            run_id = "run-1"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "datasets/sample/aapl_1d.csv"

            [strategy]
            name = "moving_average_cross"
            symbols = ["CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT"]
            fast_window = 2
            slow_window = 3

            [portfolio]
            initial_cash = "100000"
            base_currency = "USDT"
            order_qty = "0.001"
            max_abs_qty = "1"

            [risk]
            max_order_notional = "50"
            min_cash_after_order = "10"
            max_exposure = "1000"
            max_drawdown = "0.2"
            max_leverage = "1"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "binance"
            mode = "paper"
            base_url = "https://testnet.binance.vision/api"
            order_submit_enabled = false

            [paper]
            account_id = "binance-testnet"
            slippage_bps = "5"
            fee_bps = "10"

            [live]
            enabled = false
            "#,
        )
        .unwrap()
    }
}

async fn load_db(config_path: &str) -> Result<(config::AppConfig, storage::Db)> {
    let app_config = config::AppConfig::from_toml_file(config_path)?;
    ensure_database_parent(&app_config.database.url)?;
    let db = storage::Db::connect(&app_config.database.url).await?;
    Ok((app_config, db))
}

fn ensure_database_parent(database_url: &str) -> Result<()> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn sqlite_file_path(database_url: &str) -> Option<&str> {
    if database_url == "sqlite::memory:" || database_url == "sqlite://:memory:" {
        return None;
    }
    database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
}

fn backtest_settings(app_config: &config::AppConfig) -> Result<BacktestSettings> {
    Ok(BacktestSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: "backtest".to_string(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        max_exposure: Decimal::from_str(&app_config.risk.max_exposure)?,
        max_drawdown: Decimal::from_str(&app_config.risk.max_drawdown)?,
        max_leverage: Decimal::from_str(&app_config.risk.max_leverage)?,
        max_margin_used: Decimal::from_str(&app_config.risk.max_margin_used)?,
        trading_halted: app_config.risk.trading_halted,
        initial_equity: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
    })
}

fn paper_settings(app_config: &config::AppConfig) -> Result<PaperSettings> {
    Ok(PaperSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        symbol: app_config
            .strategy
            .symbols
            .first()
            .cloned()
            .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string()),
        account_id: app_config.paper.account_id.clone(),
        order_qty: Decimal::from_str(&app_config.portfolio.order_qty)?,
        max_abs_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        max_order_qty: Decimal::from_str(&app_config.portfolio.max_abs_qty)?,
        max_order_notional: Decimal::from_str(&app_config.risk.max_order_notional)?,
        min_cash_after_order: Decimal::from_str(&app_config.risk.min_cash_after_order)?,
        max_exposure: Decimal::from_str(&app_config.risk.max_exposure)?,
        max_drawdown: Decimal::from_str(&app_config.risk.max_drawdown)?,
        max_leverage: Decimal::from_str(&app_config.risk.max_leverage)?,
        max_margin_used: Decimal::from_str(&app_config.risk.max_margin_used)?,
        trading_halted: app_config.risk.trading_halted,
        initial_cash: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        base_currency: app_config.portfolio.base_currency.clone(),
        slippage_bps: Decimal::from_str(&app_config.paper.slippage_bps)?,
        fee_bps: Decimal::from_str(&app_config.paper.fee_bps)?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
        bar_delay_ms: app_config.paper.bar_delay_ms.unwrap_or(0),
    })
}
