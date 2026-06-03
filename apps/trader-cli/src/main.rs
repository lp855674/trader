use anyhow::Result;
use backtest::{BacktestRuntime, BacktestSettings};
use clap::{Parser, Subcommand};
use metrics::{equity_returns, paper_summary};
use paper::{PaperRuntime, PaperSettings};
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
    Replay {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Report {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    CheckConfig {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
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
            let bars = data::load_bars(&app_config.data.source, &app_config.data.path)?;
            let summary = PaperRuntime::new(db, paper_settings(&app_config)?)
                .run_bars(bars)
                .await?;
            println!(
                "paper completed: signals={} orders={}",
                summary.signals, summary.orders
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
        Command::Report { config } => {
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
            println!(
                "report: run_id={} status={} orders={} fills={} balances={} snapshots={} total_return={} sharpe={} sortino={} max_drawdown={} win_rate={}",
                run_id,
                run_status,
                orders.len(),
                fills.len(),
                balances.len(),
                snapshots.len(),
                summary.total_return,
                summary.sharpe,
                summary.sortino,
                summary.max_drawdown,
                summary.win_rate
            );
        }
        Command::CheckConfig { config } => {
            config::AppConfig::from_toml_file(config)?;
            println!("config ok");
        }
    }
    Ok(())
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
        max_order_notional: Decimal::from(1_000_000),
        min_cash_after_order: Decimal::ZERO,
        initial_cash: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        base_currency: app_config.portfolio.base_currency.clone(),
        slippage_bps: Decimal::from_str(&app_config.paper.slippage_bps)?,
        fee_bps: Decimal::from_str(&app_config.paper.fee_bps)?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
        bar_delay_ms: app_config.paper.bar_delay_ms.unwrap_or(0),
    })
}
