use anyhow::Result;
use backtest::{BacktestRuntime, BacktestSettings};
use clap::{Parser, Subcommand};
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
    },
    Backtest {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Replay,
    Report,
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
        Command::ImportBars { config } => {
            let (app_config, _) = load_db(&config).await?;
            let bars = data::load_bars_from_csv(&app_config.data.path)?;
            println!("imported bars: {}", bars.len());
        }
        Command::Backtest { config } => {
            let (app_config, db) = load_db(&config).await?;
            db.migrate().await?;
            let bars = data::load_bars_from_csv(&app_config.data.path)?;
            let summary = BacktestRuntime::new(db, backtest_settings(&app_config)?)
                .run(bars)
                .await?;
            println!(
                "backtest completed: signals={} orders={}",
                summary.signals, summary.orders
            );
        }
        Command::Replay => println!("replay started"),
        Command::Report => println!("report generated"),
        Command::CheckConfig { config } => {
            config::AppConfig::from_toml_file(config)?;
            println!("config ok");
        }
    }
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
