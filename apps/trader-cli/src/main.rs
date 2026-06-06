use anyhow::{Context, Result, bail};
use backtest::{BacktestRuntime, BacktestSettings};
use broker::{
    BinanceAssetBalance, BinanceLimitOrderRequest, BinanceOpenOrder, BinanceOrderSide,
    BinanceSpotTestnetAdapter, BinanceSpotTestnetSettings, Broker, IbkrPaperGatewayAdapter,
    IbkrPaperGatewaySettings,
};
use clap::{Parser, Subcommand, ValueEnum};
use metrics::{equity_returns, paper_summary};
use paper::{BinancePaperOrderExecutor, PaperRuntime, PaperSettings};
use replay::ReplayRuntime;
use rust_decimal::Decimal;
use std::{io::Write, path::Path, str::FromStr, time::Duration};

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
    IbkrPaperReadonly {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
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
    BinancePaperRecover {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
    },
    BinancePaperOpenOrders {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
    },
    BinancePaperReconcile {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
    },
    BinancePaperCancelOpenOrders {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        confirm_testnet_cancel: bool,
    },
    BinancePaperKlines {
        #[arg(long, default_value = "configs/paper/binance_testnet.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
        #[arg(long, default_value = "1m")]
        interval: String,
        #[arg(long, default_value_t = 100)]
        limit: u16,
        #[arg(long, value_enum, default_value_t = KlineOutputFormat::Parquet)]
        format: KlineOutputFormat,
        #[arg(long)]
        output: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum KlineOutputFormat {
    Parquet,
    Csv,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BinancePaperReconciliation {
    symbol: String,
    local_orders: usize,
    local_fills: usize,
    matched_orders: usize,
    local_only_orders: usize,
    remote_open_orders: usize,
    remote_open_matched: usize,
    remote_open_unmatched: usize,
    local_cash: Decimal,
    remote_cash: Decimal,
    cash_delta: Decimal,
    local_base_qty: Decimal,
    remote_base_total: Decimal,
    base_delta: Decimal,
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
        Command::IbkrPaperReadonly { config } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper readonly")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let status = adapter.status().await?;
            let settings = adapter.settings();
            println!(
                "ibkr paper readonly ok: host={} port={} client_id={} connected={} order_submit_enabled={}",
                settings.host,
                settings.port,
                settings.client_id,
                status.connected,
                app_config.broker.order_submit_enabled
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
        Command::BinancePaperRecover { config } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_binance_paper_config(&app_config, "binance paper recover")?;
            let (_, db) = load_db(&config).await?;
            db.migrate().await?;
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(&app_config)?)?;
            let recovered = recover_binance_paper_orders(&app_config, &db, &adapter).await?;
            println!(
                "binance paper recover ok: scanned={} recovered={} missing={} remaining={} trades={} run_status_updated={}",
                recovered.scanned,
                recovered.recovered,
                recovered.missing,
                recovered.remaining,
                recovered.trades,
                recovered.run_status_updated
            );
        }
        Command::BinancePaperOpenOrders { config, symbol } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_binance_paper_config(&app_config, "binance paper open orders")?;
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(&app_config)?)?;
            let orders = adapter.open_orders(&symbol).await?;
            println!(
                "binance paper open orders ok: symbol={} open_orders={}",
                symbol,
                orders.len()
            );
            for order in orders {
                println!(
                    "open_order: order_id={} client_order_id={} side={} status={} price={} orig_qty={} executed_qty={}",
                    order.order_id,
                    order.client_order_id,
                    order.side,
                    order.status,
                    order.price,
                    order.orig_qty,
                    order.executed_qty
                );
            }
        }
        Command::BinancePaperReconcile { config, symbol } => {
            let (app_config, db) = load_db(&config).await?;
            ensure_binance_paper_config(&app_config, "binance paper reconcile")?;
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(&app_config)?)?;
            let report = reconcile_binance_paper(&app_config, &db, &adapter, &symbol).await?;
            println!(
                "binance paper reconcile ok: symbol={} local_orders={} local_fills={} matched_orders={} local_only_orders={} remote_open_orders={} remote_open_matched={} remote_open_unmatched={} local_cash={} remote_cash={} cash_delta={} local_base_qty={} remote_base_total={} base_delta={}",
                report.symbol,
                report.local_orders,
                report.local_fills,
                report.matched_orders,
                report.local_only_orders,
                report.remote_open_orders,
                report.remote_open_matched,
                report.remote_open_unmatched,
                report.local_cash,
                report.remote_cash,
                report.cash_delta,
                report.local_base_qty,
                report.remote_base_total,
                report.base_delta
            );
        }
        Command::BinancePaperCancelOpenOrders {
            config,
            symbol,
            confirm_testnet_cancel,
        } => {
            if !confirm_testnet_cancel {
                bail!("refusing to cancel Binance testnet orders without --confirm-testnet-cancel");
            }
            let (app_config, db) = load_db(&config).await?;
            ensure_binance_paper_config(&app_config, "binance paper cancel open orders")?;
            db.migrate().await?;
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(&app_config)?)?;
            let orders = adapter.open_orders(&symbol).await?;
            let mut cancelled = 0usize;
            let mut local_synced = 0u64;
            for order in &orders {
                let cancelled_order = adapter
                    .cancel_binance_order(&symbol, order.order_id)
                    .await?;
                cancelled += 1;
                local_synced += db
                    .update_order_status_by_client_order_id(
                        &app_config.runtime.run_id,
                        &order.client_order_id,
                        &cancelled_order.order_id.to_string(),
                        &cancelled_order.status,
                        chrono::Utc::now().timestamp_millis(),
                    )
                    .await?;
            }
            println!(
                "binance paper cancel open orders ok: symbol={} scanned={} cancelled={} local_synced={}",
                symbol,
                orders.len(),
                cancelled,
                local_synced
            );
        }
        Command::BinancePaperKlines {
            config,
            symbol,
            interval,
            limit,
            format,
            output,
        } => {
            if limit == 0 || limit > 1000 {
                bail!("limit must be between 1 and 1000");
            }
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_binance_paper_config(&app_config, "binance paper klines")?;
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_public_testnet_settings(&app_config)?)?;
            let bars = adapter.klines(&symbol, &interval, limit).await?;
            write_binance_klines(&output, &bars, format)?;
            println!(
                "binance paper klines ok: symbol={} interval={} bars={} format={} output={}",
                symbol,
                interval,
                bars.len(),
                kline_output_format_slug(format),
                output
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
        config::BrokerKind::InteractiveBrokers => "ibkr",
    }
}

fn broker_mode_slug(mode: config::BrokerMode) -> &'static str {
    match mode {
        config::BrokerMode::Paper => "paper",
        config::BrokerMode::Live => "live",
    }
}

fn kline_output_format_slug(format: KlineOutputFormat) -> &'static str {
    match format {
        KlineOutputFormat::Parquet => "parquet",
        KlineOutputFormat::Csv => "csv",
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

fn binance_public_testnet_settings(
    app_config: &config::AppConfig,
) -> Result<BinanceSpotTestnetSettings> {
    Ok(BinanceSpotTestnetSettings {
        base_url: app_config
            .broker
            .base_url
            .clone()
            .unwrap_or_else(|| "https://testnet.binance.vision/api".to_string()),
        api_key: String::new(),
        secret_key: String::new(),
        recv_window_ms: app_config.broker.recv_window_ms.unwrap_or(5000),
    })
}

fn ibkr_paper_gateway_settings(app_config: &config::AppConfig) -> Result<IbkrPaperGatewaySettings> {
    Ok(IbkrPaperGatewaySettings {
        host: app_config
            .broker
            .host
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        port: app_config.broker.port.unwrap_or(7497),
        client_id: app_config.broker.client_id.unwrap_or(1),
        connect_timeout: Duration::from_secs(2),
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
        config::BrokerKind::InteractiveBrokers => {
            if app_config.broker.order_submit_enabled {
                bail!(
                    "IBKR paper order submit is not implemented; run ibkr-paper-readonly first and keep order_submit_enabled=false"
                );
            }
            Ok(false)
        }
        config::BrokerKind::Futu | config::BrokerKind::Okx => Ok(false),
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

fn ensure_binance_paper_config(app_config: &config::AppConfig, command_name: &str) -> Result<()> {
    if app_config.broker.kind != config::BrokerKind::Binance {
        bail!("{command_name} requires broker.kind = binance");
    }
    if app_config.broker.mode != config::BrokerMode::Paper {
        bail!("{command_name} requires broker.mode = paper");
    }
    Ok(())
}

fn ensure_ibkr_paper_config(app_config: &config::AppConfig, command_name: &str) -> Result<()> {
    if app_config.broker.kind != config::BrokerKind::InteractiveBrokers {
        bail!("{command_name} requires broker.kind = ibkr");
    }
    if app_config.broker.mode != config::BrokerMode::Paper {
        bail!("{command_name} requires broker.mode = paper");
    }
    Ok(())
}

fn binance_cancel_outcome(
    final_status: String,
    cancel_error: Option<String>,
) -> (String, Option<String>) {
    (final_status, cancel_error)
}

fn write_binance_klines(
    output: impl AsRef<Path>,
    bars: &[broker::BinanceKlineBar],
    format: KlineOutputFormat,
) -> Result<()> {
    let output = output.as_ref();
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    match format {
        KlineOutputFormat::Parquet => {
            let bars = bars
                .iter()
                .map(|bar| {
                    data::Bar::new(
                        bar.ts_ms, bar.open, bar.high, bar.low, bar.close, bar.volume,
                    )
                })
                .collect::<Vec<_>>();
            data::write_bars_to_parquet(output, &bars)?;
        }
        KlineOutputFormat::Csv => write_binance_klines_csv(output, bars)?,
    }
    Ok(())
}

fn write_binance_klines_csv(
    output: impl AsRef<Path>,
    bars: &[broker::BinanceKlineBar],
) -> Result<()> {
    let mut file = std::fs::File::create(output)?;
    writeln!(file, "ts_ms,open,high,low,close,volume")?;
    for bar in bars {
        writeln!(
            file,
            "{},{},{},{},{},{}",
            bar.ts_ms, bar.open, bar.high, bar.low, bar.close, bar.volume
        )?;
    }
    Ok(())
}

struct BinanceAccountingRecords {
    balance: storage::NewAccountBalance,
    position: Option<storage::NewPosition>,
    snapshot: storage::NewPortfolioSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BinancePaperRecoverSummary {
    scanned: usize,
    recovered: usize,
    missing: usize,
    remaining: usize,
    trades: usize,
    run_status_updated: bool,
}

async fn recover_binance_paper_orders(
    app_config: &config::AppConfig,
    db: &storage::Db,
    adapter: &BinanceSpotTestnetAdapter,
) -> Result<BinancePaperRecoverSummary> {
    let run_id = &app_config.runtime.run_id;
    let orders = db.list_recoverable_orders(run_id).await?;
    let mut summary = BinancePaperRecoverSummary {
        scanned: orders.len(),
        recovered: 0,
        missing: 0,
        remaining: 0,
        trades: 0,
        run_status_updated: false,
    };

    for order in orders {
        let symbol = paper::binance_spot_symbol(&order.symbol)?;
        let queried = match adapter
            .query_binance_order_by_client_order_id(&symbol, &order.client_order_id)
            .await
        {
            Ok(order) => order,
            Err(broker::BrokerError::Rejected(message)) if message.contains("code=-2013") => {
                summary.missing += 1;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let trades = adapter.my_trades(&symbol, queried.order_id).await?;
        let filled_qty = binance_filled_qty(&trades, queried.executed_qty);
        let ended_at_ms = chrono::Utc::now().timestamp_millis();
        for trade in &trades {
            db.insert_fill(storage::NewFill {
                id: format!("{run_id}-binance-trade-{}", trade.trade_id),
                order_id: order.id.clone(),
                run_id: run_id.clone(),
                symbol: trade.symbol.clone(),
                side: order.side.clone(),
                price: trade.price.to_string(),
                qty: trade.qty.to_string(),
                fee: trade.fee.to_string(),
                ts_ms: trade.ts_ms,
            })
            .await?;
        }
        db.update_order_execution_by_client_order_id(
            &order.client_order_id,
            &queried.order_id.to_string(),
            &queried.status,
            &filled_qty.to_string(),
            ended_at_ms,
        )
        .await?;
        if !trades.is_empty() {
            let account = adapter
                .account_snapshot(&app_config.paper.account_id)
                .await?;
            let all_fills = db.list_fills(run_id).await?;
            let accounting = binance_accounting_records_from_fills(
                run_id,
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
        summary.recovered += 1;
        summary.trades += trades.len();
    }

    summary.remaining = db.list_recoverable_orders(run_id).await?.len();
    if summary.scanned > 0 && summary.missing == 0 && summary.remaining == 0 {
        if let Some(run) = db.get_strategy_run(run_id).await? {
            if run.status != "completed" {
                db.update_strategy_run_status(
                    run_id,
                    "recovered",
                    Some(chrono::Utc::now().timestamp_millis()),
                    None,
                )
                .await?;
                summary.run_status_updated = true;
            }
        }
    }
    Ok(summary)
}

async fn reconcile_binance_paper(
    app_config: &config::AppConfig,
    db: &storage::Db,
    adapter: &BinanceSpotTestnetAdapter,
    symbol: &str,
) -> Result<BinancePaperReconciliation> {
    let run_id = &app_config.runtime.run_id;
    let exchange_symbol = paper::binance_spot_symbol(symbol)?;
    let local_orders = db.list_orders(run_id).await?;
    let local_fills = db.list_fills(run_id).await?;
    let local_balances = db.list_account_balances(run_id).await?;
    let local_positions = db.list_positions(run_id).await?;
    let remote_open_orders = adapter.open_orders(&exchange_symbol).await?;
    let remote_balances = adapter.account_balances().await?;

    let matched_orders = local_orders
        .iter()
        .filter(|order| binance_local_order_matches_remote_open(order, &remote_open_orders))
        .count();
    let local_only_orders = local_orders.len() - matched_orders;
    let remote_open_matched = remote_open_orders
        .iter()
        .filter(|remote| {
            local_orders.iter().any(|local| {
                local.broker_order_id.as_deref() == Some(&remote.order_id.to_string())
                    || local.client_order_id == remote.client_order_id
            })
        })
        .count();
    let remote_open_unmatched = remote_open_orders.len() - remote_open_matched;
    let local_cash = local_balances
        .iter()
        .find(|balance| balance.asset == app_config.portfolio.base_currency)
        .and_then(|balance| Decimal::from_str(&balance.total).ok())
        .unwrap_or(Decimal::ZERO);
    let remote_cash = binance_balance_total(&remote_balances, &app_config.portfolio.base_currency);
    let strategy_symbol = app_config
        .strategy
        .symbols
        .first()
        .context("strategy must contain at least one symbol")?;
    let local_base_qty = local_positions
        .iter()
        .find(|position| position.symbol == *strategy_symbol)
        .and_then(|position| Decimal::from_str(&position.qty).ok())
        .unwrap_or(Decimal::ZERO);
    let remote_base_total = binance_balance_total(&remote_balances, &binance_base_asset(symbol)?);

    Ok(BinancePaperReconciliation {
        symbol: exchange_symbol,
        local_orders: local_orders.len(),
        local_fills: local_fills.len(),
        matched_orders,
        local_only_orders,
        remote_open_orders: remote_open_orders.len(),
        remote_open_matched,
        remote_open_unmatched,
        local_cash,
        remote_cash,
        cash_delta: remote_cash - local_cash,
        local_base_qty,
        remote_base_total,
        base_delta: remote_base_total - local_base_qty,
    })
}

fn binance_local_order_matches_remote_open(
    local: &storage::NewOrder,
    remote_open_orders: &[BinanceOpenOrder],
) -> bool {
    remote_open_orders.iter().any(|remote| {
        local.broker_order_id.as_deref() == Some(&remote.order_id.to_string())
            || local.client_order_id == remote.client_order_id
    })
}

fn binance_balance_total(remote_balances: &[BinanceAssetBalance], asset: &str) -> Decimal {
    remote_balances
        .iter()
        .find(|balance| balance.asset == asset)
        .map(BinanceAssetBalance::total)
        .unwrap_or(Decimal::ZERO)
}

fn binance_base_asset(symbol: &str) -> Result<String> {
    let exchange_symbol = paper::binance_spot_symbol(symbol)?;
    exchange_symbol
        .strip_suffix("USDT")
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("unsupported Binance quote asset for {symbol}"))
}

fn binance_filled_qty(trades: &[broker::BinanceTrade], queried_qty: Decimal) -> Decimal {
    let trade_qty = trades
        .iter()
        .fold(Decimal::ZERO, |total, trade| total + trade.qty);
    if trade_qty > Decimal::ZERO {
        trade_qty
    } else {
        queried_qty
    }
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
        binance_accounting_records_from_fills, binance_balance_total, binance_base_asset,
        binance_cancel_outcome, binance_local_order_matches_remote_open, binance_testnet_settings,
        settings_with_broker_initial_cash,
    };
    use broker::{BinanceAssetBalance, BinanceOpenOrder};
    use rust_decimal::Decimal;
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
    fn binance_reconcile_matches_local_order_by_client_or_broker_id() {
        let remote_orders = vec![
            BinanceOpenOrder {
                order_id: 42,
                client_order_id: "client-42".to_string(),
                symbol: "BTCUSDT".to_string(),
                status: "NEW".to_string(),
                side: "BUY".to_string(),
                price: dec!(100000),
                orig_qty: dec!(0.001),
                executed_qty: dec!(0),
            },
            BinanceOpenOrder {
                order_id: 77,
                client_order_id: "client-77".to_string(),
                symbol: "BTCUSDT".to_string(),
                status: "NEW".to_string(),
                side: "SELL".to_string(),
                price: dec!(100100),
                orig_qty: dec!(0.001),
                executed_qty: dec!(0),
            },
        ];
        let by_client = storage::NewOrder {
            id: "order-1".to_string(),
            run_id: "run-1".to_string(),
            client_order_id: "client-42".to_string(),
            broker_order_id: None,
            account_id: "paper".to_string(),
            symbol: "BTCUSDT".to_string(),
            side: "BUY".to_string(),
            order_type: "MARKET".to_string(),
            price: None,
            qty: "0.001".to_string(),
            filled_qty: "0".to_string(),
            status: "NEW".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let mut by_broker = by_client.clone();
        by_broker.client_order_id = "other-client".to_string();
        by_broker.broker_order_id = Some("77".to_string());

        assert!(binance_local_order_matches_remote_open(
            &by_client,
            &remote_orders
        ));
        assert!(binance_local_order_matches_remote_open(
            &by_broker,
            &remote_orders
        ));
    }

    #[test]
    fn binance_reconcile_sums_remote_balances_and_extracts_base_asset() {
        let balances = vec![BinanceAssetBalance {
            asset: "BTC".to_string(),
            free: dec!(0.001),
            locked: dec!(0.0002),
        }];

        assert_eq!(binance_balance_total(&balances, "BTC"), dec!(0.0012));
        assert_eq!(binance_balance_total(&balances, "USDT"), Decimal::ZERO);
        assert_eq!(binance_base_asset("BTCUSDT").unwrap(), "BTC");
        assert_eq!(
            binance_base_asset("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT").unwrap(),
            "BTC"
        );
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
