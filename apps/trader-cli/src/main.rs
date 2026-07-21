use anyhow::{Context, Result, bail};
use backtest::{BacktestRuntime, BacktestSettings};
use broker::{
    BinanceAssetBalance, BinanceLimitOrderRequest, BinanceOpenOrder, BinanceOrderSide,
    BinanceSpotTestnetAdapter, BinanceSpotTestnetSettings, Broker, FakeBrokerAdapter,
    IbkrLimitOrderRequest, IbkrOpenOrder, IbkrOrderSide, IbkrPaperGatewayAdapter,
    IbkrPaperGatewaySettings,
};
use clap::{Parser, Subcommand, ValueEnum};
use events::LogWriterSettings;
use hmac::{Hmac, Mac};
use market_data::{
    IbkrMarketDataProvider, LongbridgeMarketDataProvider, LongbridgeMarketDataSettings,
    MarketDataProvider,
};
use metrics::{equity_returns, paper_summary};
use paper::{
    BinancePaperOrderExecutor, IbkrPaperGatewayOrderClient, IbkrPaperOrderExecutor, PaperRuntime,
    PaperSettings,
};
use replay::ReplayRuntime;
use rust_decimal::Decimal;
use sha2::Sha256;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

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
    FeatureManifest {
        #[arg(long)]
        parquet: String,
        #[arg(long)]
        output: String,
    },
    FeatureBuildSma {
        #[arg(long)]
        source: String,
        #[arg(long)]
        input: String,
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        feature_name: String,
        #[arg(long)]
        period: usize,
        #[arg(long)]
        version: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        manifest_output: String,
    },
    FeatureBuildIndicator {
        #[arg(long)]
        indicator: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        input: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        inputs_config: Option<String>,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        feature_name: String,
        #[arg(long)]
        period: usize,
        #[arg(long)]
        version: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        manifest_output: String,
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
    MarketDataProbe {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_longbridge.toml")]
        config: String,
        #[arg(long = "symbol")]
        symbols: Vec<String>,
    },
    IbkrPaperMarketData {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
        #[arg(long = "symbol")]
        symbols: Vec<String>,
        #[arg(long)]
        delayed: bool,
    },
    IbkrPaperOpenOrders {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
    },
    IbkrPaperExecutions {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
        #[arg(long, default_value_t = 1)]
        request_id: i64,
        #[arg(long)]
        symbol: Option<String>,
    },
    IbkrPaperReconcile {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long, default_value_t = 1)]
        request_id: i64,
    },
    IbkrPaperNextOrderId {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
    },
    IbkrPaperCancelOrder {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
        #[arg(long)]
        order_id: i64,
        #[arg(long)]
        confirm_ibkr_paper_cancel: bool,
    },
    IbkrPaperTinyOrder {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        side: String,
        #[arg(long)]
        qty: String,
        #[arg(long)]
        price: String,
        #[arg(long, default_value_t = 30)]
        observe_seconds: u64,
        #[arg(long)]
        confirm_ibkr_paper_order: bool,
    },
    IbkrPaperRecover {
        #[arg(long, default_value = "configs/paper/ibkr_aapl_1d_parquet.toml")]
        config: String,
        #[arg(long, default_value_t = 1)]
        request_id: i64,
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
    RiskKillSwitch {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        cancel_open_orders: bool,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        confirm_kill_switch: bool,
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
        #[arg(long)]
        run_id: String,
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        format: ReportFormat,
        #[arg(long)]
        output: Option<String>,
    },
    Positions {
        #[command(subcommand)]
        command: PositionsCommand,
    },
    Snapshots {
        #[command(subcommand)]
        command: SnapshotsCommand,
    },
    Configs {
        #[command(subcommand)]
        command: ConfigsCommand,
    },
    MarketRules {
        #[command(subcommand)]
        command: MarketRulesCommand,
    },
    Runs {
        #[command(subcommand)]
        command: RunsCommand,
    },
    Logs {
        #[command(subcommand)]
        command: LogsCommand,
    },
    Reconciliation {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: String,
    },
    ReconciliationGate {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long = "account")]
        accounts: Vec<String>,
        #[arg(long)]
        min_successful_audits: Option<usize>,
        #[arg(long)]
        max_audit_age_ms: Option<i64>,
    },
    OrderEvents {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        order_id: Option<String>,
        #[arg(long)]
        client_order_id: Option<String>,
        #[arg(long)]
        broker_order_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        event_type: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    RiskEvents {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        risk_type: Option<String>,
        #[arg(long)]
        decision: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationDrifts {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationAlertsSummary {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationGateAlertsSummary {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationAlertsExport {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationAlertDeliveriesSummary {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        alert_message: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationAlertDeliveriesExport {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    ReconciliationAlertRedeliver {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        webhook_url: String,
        #[arg(long)]
        auth_token: Option<String>,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
    Funding {
        #[command(subcommand)]
        command: FundingCommand,
    },
    Ingest {
        #[command(subcommand)]
        command: IngestCommand,
    },
    CheckConfig {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    LiveWorker {
        #[arg(long)]
        launch_file: String,
    },
}

#[derive(Subcommand)]
enum PositionsCommand {
    List {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        exchange: Option<String>,
    },
}

#[derive(Subcommand)]
enum SnapshotsCommand {
    Cash {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
    },
    Positions {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        position_side: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
    },
}

#[derive(Subcommand)]
enum ConfigsCommand {
    Create {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        file: String,
        #[arg(long, default_value = "local")]
        created_by: String,
        #[arg(long)]
        parent_version: Option<u32>,
        #[arg(long)]
        target_env: Option<String>,
        #[arg(long)]
        rollout: Option<String>,
        #[arg(long)]
        ts_ms: Option<i64>,
    },
    List {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
    },
    Show {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: Option<u32>,
        #[arg(long)]
        published: bool,
    },
    Diff {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        v1: u32,
        #[arg(long)]
        v2: u32,
    },
    Rollback {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: u32,
        #[arg(long, default_value = "local")]
        actor: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        ts_ms: Option<i64>,
    },
    SubmitReview {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: u32,
        #[arg(long, default_value = "local")]
        changed_by: String,
        #[arg(long)]
        actor_role: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        ts_ms: Option<i64>,
    },
    Approve {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: u32,
        #[arg(long, default_value = "local")]
        changed_by: String,
        #[arg(long)]
        actor_role: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        ts_ms: Option<i64>,
    },
    Publish {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: u32,
        #[arg(long, default_value = "local")]
        changed_by: String,
        #[arg(long)]
        actor_role: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        ts_ms: Option<i64>,
    },
    Archive {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: u32,
        #[arg(long, default_value = "local")]
        changed_by: String,
        #[arg(long)]
        actor_role: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        ts_ms: Option<i64>,
    },
    PendingApprovals {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        target_env: Option<String>,
    },
    GovernancePolicy {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Releases {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        config_id: String,
    },
    Audits {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        config_id: String,
    },
}

#[derive(Subcommand)]
enum MarketRulesCommand {
    Effective {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        market: String,
        #[arg(long)]
        exchange: String,
        #[arg(long)]
        asset_class: String,
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        trading_day: String,
        #[arg(long)]
        at_ms: Option<i64>,
    },
    Audits {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        rule_type: Option<String>,
        #[arg(long)]
        rule_id: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        limit: Option<i64>,
    },
}

#[derive(Subcommand)]
enum RunsCommand {
    ConfigVersion {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: String,
    },
}

#[derive(Subcommand)]
enum LogsCommand {
    List {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long, default_value_t = 0)]
        offset: i64,
    },
    Count {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        search: Option<String>,
    },
    Metrics {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
    },
    Tail {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 1000)]
        poll_interval_ms: u64,
        #[arg(long, default_value_t = 10)]
        max_polls: u32,
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
    Export {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long, default_value_t = 0)]
        offset: i64,
    },
    Ship {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        collector_url: String,
        #[arg(long)]
        bearer_token: Option<String>,
        #[arg(long)]
        signature_secret_env: Option<String>,
        #[arg(long, default_value_t = 0)]
        max_retries: u32,
        #[arg(long, default_value_t = 250)]
        retry_backoff_ms: u64,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long, default_value_t = 0)]
        offset: i64,
    },
    Purge {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long = "before")]
        before_ms: i64,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        run_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum FundingCommand {
    List {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        exchange: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long = "from")]
        from_ms: Option<i64>,
        #[arg(long = "to")]
        to_ms: Option<i64>,
    },
}

#[derive(Subcommand)]
enum IngestCommand {
    BinanceMeta {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long, default_value = "binance")]
        exchange: String,
    },
    FundingRates {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long, default_value = "binance")]
        exchange: String,
        #[arg(long)]
        symbol: String,
    },
    CorporateActions {
        #[arg(long, default_value = "configs/backtest/ma_cross.toml")]
        config: String,
        #[arg(long)]
        symbol: String,
    },
    Status {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeatureIndicatorKind {
    Sma,
    Ema,
    Rsi,
}

impl FeatureIndicatorKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "sma" => Ok(Self::Sma),
            "ema" => Ok(Self::Ema),
            "rsi" => Ok(Self::Rsi),
            other => bail!("unsupported indicator {other}; expected sma, ema or rsi"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sma => "sma",
            Self::Ema => "ema",
            Self::Rsi => "rsi",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct IbkrPaperReconciliation {
    symbol: String,
    local_orders: usize,
    local_fills: usize,
    matched_orders: usize,
    local_only_orders: usize,
    remote_open_orders: usize,
    remote_open_matched: usize,
    remote_open_unmatched: usize,
    remote_executions: usize,
    remote_execution_matched: usize,
    remote_execution_matched_orders: usize,
    remote_execution_max_per_order: usize,
    remote_execution_unmatched: usize,
    remote_execution_field_drifts: usize,
    remote_execution_order_ids: Vec<String>,
    remote_execution_client_order_ids: Vec<String>,
    remote_execution_trade_ids: Vec<String>,
    local_fully_filled_orders: usize,
    local_partially_filled_orders: usize,
    local_fill_qty: Decimal,
    remote_execution_qty: Decimal,
    qty_delta: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IbkrExecutionMatchSummary {
    matched: usize,
    matched_orders: usize,
    max_per_order: usize,
    field_drifts: usize,
    matched_qty: Decimal,
    order_ids: Vec<String>,
    client_order_ids: Vec<String>,
    trade_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalOrder {
    id: String,
    client_order_id: String,
    broker_order_id: Option<String>,
    account_id: String,
    symbol: String,
    qty: String,
    filled_qty: String,
    status: String,
}

impl From<storage::StoredOrder> for LocalOrder {
    fn from(order: storage::StoredOrder) -> Self {
        Self {
            id: order.id,
            client_order_id: order.client_order_id,
            broker_order_id: order.broker_order_id,
            account_id: order.account_id,
            symbol: order.symbol,
            qty: order.qty,
            filled_qty: order.filled_qty,
            status: order.status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalFill {
    id: String,
    order_id: String,
    symbol: String,
    side: String,
    price: String,
    qty: String,
    fee: String,
}

impl From<storage::StoredFill> for LocalFill {
    fn from(fill: storage::StoredFill) -> Self {
        Self {
            id: fill.id,
            order_id: fill.order_id,
            symbol: fill.symbol,
            side: fill.side,
            price: fill.price,
            qty: fill.qty,
            fee: fill.fee,
        }
    }
}

fn local_orders_from_storage(orders: Vec<storage::StoredOrder>) -> Vec<LocalOrder> {
    orders.into_iter().map(LocalOrder::from).collect()
}

fn local_fills_from_storage(fills: Vec<storage::StoredFill>) -> Vec<LocalFill> {
    fills.into_iter().map(LocalFill::from).collect()
}

fn main() -> Result<()> {
    std::thread::Builder::new()
        .name("trader-cli".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(run_cli)?
        .join()
        .map_err(|_| anyhow::anyhow!("trader CLI thread panicked"))?
}

fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()?;
    runtime.block_on(run_command(cli.command))
}

async fn run_command(command: Command) -> Result<()> {
    match command {
        Command::Init => println!("initialized"),
        Command::LiveWorker { launch_file } => run_live_worker(&launch_file).await?,
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
        Command::FeatureManifest { parquet, output } => {
            let records = feature_store::load_feature_records_from_parquet(&parquet)?;
            let manifest = feature_store::build_feature_manifest(&parquet, &records);
            ensure_file_parent(&output)?;
            feature_store::write_feature_manifest(&output, &manifest)?;
            println!(
                "wrote feature manifest: records={} output={}",
                manifest.record_count, output
            );
        }
        Command::FeatureBuildSma {
            source,
            input,
            symbol,
            run_id,
            feature_name,
            period,
            version,
            output,
            manifest_output,
        } => {
            let record_count = build_indicator_features(
                FeatureIndicatorKind::Sma,
                IndicatorBuild {
                    builder: "feature-build-sma".to_string(),
                    inputs: vec![data::BarInput::new(symbol, source, input)],
                    run_id,
                    feature_name,
                    period,
                    version,
                    output: output.clone(),
                    manifest_output: manifest_output.clone(),
                },
            )?;
            println!(
                "wrote sma features: records={} output={} manifest={}",
                record_count, output, manifest_output
            );
        }
        Command::FeatureBuildIndicator {
            indicator,
            source,
            input,
            symbol,
            inputs_config,
            run_id,
            feature_name,
            period,
            version,
            output,
            manifest_output,
        } => {
            let indicator = FeatureIndicatorKind::parse(&indicator)?;
            let inputs = indicator_inputs(source, input, symbol, inputs_config)?;
            let record_count = build_indicator_features(
                indicator,
                IndicatorBuild {
                    builder: "feature-build-indicator".to_string(),
                    inputs,
                    run_id,
                    feature_name,
                    period,
                    version,
                    output: output.clone(),
                    manifest_output: manifest_output.clone(),
                },
            )?;
            println!(
                "wrote {} features: records={} output={} manifest={}",
                indicator.label(),
                record_count,
                output,
                manifest_output
            );
        }
        Command::Backtest { config } => {
            let (app_config, db) = load_db(&config).await?;
            let run_spec = runtime::RunSpec::from(&app_config);
            db.migrate().await?;
            run_configured_log_retention(&db, &app_config).await?;
            persist_cli_run_config_snapshot(&db, &run_spec, &config).await?;
            insert_event(
                &db,
                &app_config.runtime.run_id,
                "backtest.started",
                &serde_json::json!({ "run_id": &app_config.runtime.run_id }).to_string(),
            )
            .await?;
            let market_slices = load_configured_market_slices(&app_config)?;
            let summary = BacktestRuntime::new(db, backtest_settings(&app_config)?)
                .run_market_slices(market_slices)
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
            let run_spec = runtime::RunSpec::from(&app_config);
            db.migrate().await?;
            run_configured_log_retention(&db, &app_config).await?;
            persist_cli_run_config_snapshot(&db, &run_spec, &config).await?;
            let market_slices = load_configured_market_slices(&app_config).with_context(|| {
                format!(
                    "failed to load market data from {}",
                    data_source_description(&app_config)
                )
            })?;
            let settings = paper_settings(&app_config)?;
            let summary = paper_runtime(&app_config, db, settings)
                .await?
                .run_market_slices(market_slices)
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
            let real_broker_connection = paper_real_broker_connection_ready(&app_config).await?;
            let market_slices = load_configured_market_slices(&app_config).with_context(|| {
                format!(
                    "failed to load market data from {}",
                    data_source_description(&app_config)
                )
            })?;
            println!(
                "paper preflight ok: run_id={} strategy={} symbol={} bars={} database={} broker={} broker_mode={} account={} max_order_notional={} max_exposure={} trading_halted={} real_broker_connection={} order_submit_enabled={}",
                settings.run_id,
                settings.strategy_name,
                settings.symbol,
                market_slices.len(),
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
            let accounts = adapter
                .validate_paper_account(&app_config.paper.account_id)
                .await?;
            let settings = adapter.settings();
            println!(
                "ibkr paper readonly ok: host={} port={} client_id={} connected=true account={} accounts={} order_submit_enabled={}",
                settings.host,
                settings.port,
                settings.client_id,
                app_config.paper.account_id,
                accounts.len(),
                app_config.broker.order_submit_enabled
            );
        }
        Command::MarketDataProbe { config, symbols } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            let provider = market_data_provider_for_probe(&app_config)?;
            let snapshot_count =
                run_market_data_probe(provider.as_ref(), &app_config, &symbols, true).await?;
            println!(
                "market data probe ok: provider={:?} snapshots={snapshot_count}",
                app_config.market_data.provider
            );
        }
        Command::IbkrPaperMarketData {
            config,
            symbols,
            delayed,
        } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper market data")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let snapshot_count =
                run_ibkr_market_data_probe(&adapter, &app_config, &symbols, delayed, true).await?;
            println!("ibkr paper market data ok: snapshots={snapshot_count}");
        }
        Command::IbkrPaperOpenOrders { config } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper open orders")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let orders = adapter.open_orders().await?;
            let first_order = orders.first();
            println!(
                "ibkr paper open orders ok: open_orders={} order_id={} symbol={} status={}",
                orders.len(),
                first_order
                    .map(|order| order.order_id.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                first_order
                    .map(|order| order.symbol.as_str())
                    .unwrap_or("none"),
                first_order
                    .map(|order| order.status.as_str())
                    .unwrap_or("none")
            );
        }
        Command::IbkrPaperExecutions {
            config,
            request_id,
            symbol,
        } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper executions")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let symbol = symbol
                .as_deref()
                .map(paper::ibkr_stock_symbol)
                .unwrap_or_else(|| {
                    let configured_symbol = app_config
                        .strategy
                        .symbols
                        .first()
                        .map(String::as_str)
                        .unwrap_or("AAPL");
                    paper::ibkr_stock_symbol(configured_symbol)
                })?;
            let executions = adapter
                .executions(request_id, &app_config.paper.account_id, &symbol)
                .await?;
            let first_execution = executions.first();
            println!(
                "ibkr paper executions ok: request_id={} account={} symbol={} executions={} order_id={} client_order_id={} trade_id={}",
                request_id,
                app_config.paper.account_id,
                symbol,
                executions.len(),
                first_execution
                    .map(|execution| execution.order_id.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                first_execution
                    .map(|execution| execution.client_order_id.as_str())
                    .unwrap_or("none"),
                first_execution
                    .map(|execution| execution.trade_id.as_str())
                    .unwrap_or("none")
            );
        }
        Command::IbkrPaperReconcile {
            config,
            symbol,
            request_id,
        } => {
            let (app_config, db) = load_db(&config).await?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper reconcile")?;
            db.migrate().await?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let symbol = symbol
                .as_deref()
                .map(paper::ibkr_stock_symbol)
                .unwrap_or_else(|| {
                    let configured_symbol = app_config
                        .strategy
                        .symbols
                        .first()
                        .map(String::as_str)
                        .unwrap_or("AAPL");
                    paper::ibkr_stock_symbol(configured_symbol)
                })?;
            let report =
                reconcile_ibkr_paper(&app_config, &db, &adapter, request_id, &symbol).await?;
            println!(
                "ibkr paper reconcile ok: symbol={} local_orders={} local_fills={} matched_orders={} local_only_orders={} remote_open_orders={} remote_open_matched={} remote_open_unmatched={} remote_executions={} remote_execution_matched={} remote_execution_matched_orders={} remote_execution_max_per_order={} remote_execution_unmatched={} remote_execution_field_drifts={} remote_execution_order_ids={} remote_execution_client_order_ids={} remote_execution_trade_ids={} local_fully_filled_orders={} local_partially_filled_orders={} local_fill_qty={} remote_execution_qty={} qty_delta={}",
                report.symbol,
                report.local_orders,
                report.local_fills,
                report.matched_orders,
                report.local_only_orders,
                report.remote_open_orders,
                report.remote_open_matched,
                report.remote_open_unmatched,
                report.remote_executions,
                report.remote_execution_matched,
                report.remote_execution_matched_orders,
                report.remote_execution_max_per_order,
                report.remote_execution_unmatched,
                report.remote_execution_field_drifts,
                joined_output_values(&report.remote_execution_order_ids),
                joined_output_values(&report.remote_execution_client_order_ids),
                joined_output_values(&report.remote_execution_trade_ids),
                report.local_fully_filled_orders,
                report.local_partially_filled_orders,
                report.local_fill_qty,
                report.remote_execution_qty,
                report.qty_delta
            );
        }
        Command::IbkrPaperNextOrderId { config } => {
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper next order id")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let order_id = adapter.next_order_id().await?;
            println!("ibkr paper next order id ok: next_order_id={order_id}");
        }
        Command::IbkrPaperCancelOrder {
            config,
            order_id,
            confirm_ibkr_paper_cancel,
        } => {
            if !confirm_ibkr_paper_cancel {
                bail!("refusing to cancel IBKR paper order without --confirm-ibkr-paper-cancel");
            }
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper cancel order")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let status = adapter.cancel_ibkr_order(order_id).await?;
            println!(
                "ibkr paper cancel order ok: order_id={} status={} filled_qty={} remaining_qty={} avg_fill_price={}",
                status.order_id,
                status.status,
                status.filled_qty,
                status.remaining_qty,
                status.avg_fill_price
            );
        }
        Command::IbkrPaperTinyOrder {
            config,
            symbol,
            side,
            qty,
            price,
            observe_seconds,
            confirm_ibkr_paper_order,
        } => {
            if !confirm_ibkr_paper_order {
                bail!("refusing to submit IBKR paper order without --confirm-ibkr-paper-order");
            }
            let app_config = config::AppConfig::from_toml_file(&config)?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper tiny order")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let client_order_id = format!(
                "trader-{}",
                uuid::Uuid::new_v4()
                    .simple()
                    .to_string()
                    .get(..24)
                    .unwrap_or("ibkrorder")
            );
            let order = IbkrLimitOrderRequest {
                symbol: paper::ibkr_stock_symbol(&symbol)?,
                side: ibkr_order_side(&side)?,
                quantity: Decimal::from_str(&qty)?,
                price: Decimal::from_str(&price)?,
                outside_rth: true,
                route_exchange: app_config.broker.ibkr_route_exchange.clone(),
                override_percentage_constraints: app_config
                    .broker
                    .ibkr_override_percentage_constraints,
                client_order_id,
            };
            let diagnostic = adapter
                .diagnose_limit_order(
                    &app_config.paper.account_id,
                    &order,
                    Duration::from_secs(observe_seconds),
                )
                .await?;
            for event in &diagnostic.events {
                println!("ibkr paper order event: {}", serde_json::to_string(event)?);
            }
            println!(
                "ibkr paper tiny order ok: symbol={} order_id={} status={} filled_qty={} client_order_id={} completion_reason={} terminal_status={} observed_for_ms={} events={}",
                order.symbol,
                diagnostic.order_id,
                diagnostic.latest_status.as_deref().unwrap_or("none"),
                diagnostic.filled_qty,
                diagnostic.client_order_id,
                diagnostic.completion_reason,
                diagnostic.terminal_status.as_deref().unwrap_or("none"),
                diagnostic.observed_for_ms,
                diagnostic.events.len()
            );
        }
        Command::IbkrPaperRecover { config, request_id } => {
            let (app_config, db) = load_db(&config).await?;
            ensure_ibkr_paper_config(&app_config, "ibkr paper recover")?;
            db.migrate().await?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(&app_config)?)?;
            let recovered =
                recover_ibkr_paper_orders(&app_config, &db, &adapter, request_id).await?;
            println!(
                "ibkr paper recover ok: scanned={} recovered={} missing={} remaining={} trades={} run_status_updated={}",
                recovered.scanned,
                recovered.recovered,
                recovered.missing,
                recovered.remaining,
                recovered.trades,
                recovered.run_status_updated
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
            db.start_strategy_run(storage::StrategyRunStartCommand {
                run_id: app_config.runtime.run_id.clone(),
                name: "binance_paper_tiny_order".to_string(),
                mode: "paper".to_string(),
                started_at_ms,
                config: serde_json::json!({
                    "broker": "binance",
                    "testnet": true,
                    "symbol": symbol,
                    "side": side,
                    "qty": qty,
                    "price": price
                }),
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
            db.record_external_order(storage::ExternalOrderCommand {
                order_id: order_id.clone(),
                run_id: app_config.runtime.run_id.clone(),
                client_order_id: order.client_order_id.clone(),
                broker_order_id: Some(placed.order_id.to_string()),
                account_id: app_config.paper.account_id.clone(),
                symbol: symbol.clone(),
                side: side.to_ascii_uppercase(),
                order_type: "LIMIT".to_string(),
                price: Some(Decimal::from_str(&price)?),
                qty: Decimal::from_str(&qty)?,
                filled_qty: Decimal::ZERO,
                status: placed.status.clone(),
                ts_ms: now_ms,
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
                db.record_external_fill(storage::ExternalFillCommand {
                    id: format!(
                        "{}-binance-trade-{}",
                        app_config.runtime.run_id, trade.trade_id
                    ),
                    order_id: order_id.clone(),
                    run_id: app_config.runtime.run_id.clone(),
                    symbol: trade.symbol.clone(),
                    side: side.to_ascii_uppercase(),
                    price: trade.price,
                    qty: trade.qty,
                    fee: trade.fee,
                    ts_ms: trade.ts_ms,
                })
                .await?;
            }
            if !trades.is_empty() {
                let account = adapter
                    .account_snapshot(&app_config.paper.account_id)
                    .await?;
                let all_fills =
                    local_fills_from_storage(db.list_fills(&app_config.runtime.run_id).await?);
                let accounting = binance_accounting_records_from_fills(
                    &app_config.runtime.run_id,
                    &app_config.paper.account_id,
                    &app_config.portfolio.base_currency,
                    account.cash,
                    &all_fills,
                    ended_at_ms,
                )?;
                db.record_account_balance(accounting.balance).await?;
                if let Some(position) = accounting.position {
                    db.record_position(position).await?;
                }
                db.record_portfolio_snapshot(accounting.snapshot).await?;
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
        Command::RiskKillSwitch {
            config,
            run_id,
            cancel_open_orders,
            symbol,
            confirm_kill_switch,
        } => {
            if !confirm_kill_switch {
                bail!("refusing to activate kill switch without --confirm-kill-switch");
            }
            let (app_config, db) = load_db(&config).await?;
            db.migrate().await?;
            let account_id = app_config.paper.account_id.clone();
            db.record_runtime_event(storage::RuntimeEventCommand {
                ts_ms: chrono::Utc::now().timestamp_millis(),
                source: run_id.clone(),
                category: "algorithm.risk.rejected".to_string(),
                payload: serde_json::json!({
                    "run_id": run_id,
                    "account_id": account_id,
                    "symbol": symbol,
                    "risk_type": "operator_kill_switch",
                    "decision": "rejected",
                    "reason": "operator activated kill switch",
                }),
            })
            .await?;
            let mut cancelled = Vec::new();
            let mut local_synced = 0u64;
            if cancel_open_orders {
                let broker = operational_broker(&app_config)?;
                cancelled = broker::cancel_open_orders_for_account_symbol(
                    broker.as_ref(),
                    &account_id,
                    symbol.as_deref(),
                )
                .await?;
                local_synced = sync_cancelled_open_orders(&db, &run_id, &cancelled).await?;
            }
            println!(
                "risk kill switch ok: account_id={} cancel_open_orders={} cancelled={} local_synced={} symbol={}",
                account_id,
                cancel_open_orders,
                cancelled.len(),
                local_synced,
                symbol.as_deref().unwrap_or("*")
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
            let run_spec = runtime::RunSpec::from(&app_config);
            db.migrate().await?;
            persist_cli_run_config_snapshot(&db, &run_spec, &config).await?;
            let started_at_ms = chrono::Utc::now().timestamp_millis();
            db.start_strategy_run(storage::StrategyRunStartCommand {
                run_id: app_config.runtime.run_id.clone(),
                name: app_config.strategy.name.clone(),
                mode: "replay".to_string(),
                started_at_ms,
                config: serde_json::json!({}),
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
            run_id,
            format,
            output,
        } => {
            let (_, db) = load_db(&config).await?;
            db.migrate().await?;
            let run_status = db
                .get_strategy_run(&run_id)
                .await?
                .map(|run| run.status)
                .unwrap_or_else(|| "missing".to_string());
            let orders = local_orders_from_storage(db.list_orders(&run_id).await?);
            let fills = local_fills_from_storage(db.list_fills(&run_id).await?);
            let balances = db.list_account_balances(&run_id).await?;
            let snapshots = db.list_portfolio_snapshots(&run_id).await?;
            let equity = snapshots
                .iter()
                .map(|snapshot| Decimal::from_str(&snapshot.equity))
                .collect::<Result<Vec<_>, _>>()?;
            let returns = equity_returns(&equity);
            let summary = paper_summary(orders.len(), fills.len(), &equity, &returns);
            let report = ReportData {
                run_id,
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
        Command::Positions { command } => match command {
            PositionsCommand::List {
                config,
                run_id,
                account,
                exchange,
            } => {
                let (_, db) = load_db(&config).await?;
                let positions = db.list_crypto_positions(&run_id).await?;
                for position in positions.into_iter().filter(|position| {
                    account
                        .as_deref()
                        .is_none_or(|account| position.account_id == account)
                        && exchange
                            .as_deref()
                            .is_none_or(|exchange| position.exchange == exchange)
                }) {
                    println!(
                        "crypto_position: run_id={} account={} exchange={} symbol={} asset_class={} margin_mode={} side={} leverage={} qty={} avg_price={} margin_used={} funding_fee={} realized_pnl={} unrealized_pnl={} updated_at_ms={}",
                        position.run_id,
                        position.account_id,
                        position.exchange,
                        position.symbol,
                        position.asset_class,
                        position.margin_mode,
                        position.position_side,
                        position.leverage,
                        position.qty,
                        position.avg_price,
                        position.margin_used,
                        position.funding_fee,
                        position.realized_pnl,
                        position.unrealized_pnl,
                        position.updated_at_ms
                    );
                }
            }
        },
        Command::Snapshots { command } => match command {
            SnapshotsCommand::Cash {
                config,
                run_id,
                currency,
                from_ms,
                to_ms,
            } => {
                let (_, db) = load_db(&config).await?;
                let snapshots = db
                    .list_cash_snapshots_filtered(&run_id, currency.as_deref(), from_ms, to_ms)
                    .await?;
                for snapshot in snapshots {
                    println!(
                        "cash_snapshot: run_id={} ts_ms={} currency={} cash={} available_cash={} frozen_cash={} created_at_ms={}",
                        snapshot.run_id,
                        snapshot.ts_ms,
                        snapshot.currency,
                        snapshot.cash,
                        snapshot.available_cash,
                        snapshot.frozen_cash,
                        snapshot.created_at_ms
                    );
                }
            }
            SnapshotsCommand::Positions {
                config,
                run_id,
                symbol,
                position_side,
                from_ms,
                to_ms,
            } => {
                let (_, db) = load_db(&config).await?;
                let snapshots = db
                    .list_position_snapshots_filtered(
                        &run_id,
                        symbol.as_deref(),
                        position_side.as_deref(),
                        from_ms,
                        to_ms,
                    )
                    .await?;
                for snapshot in snapshots {
                    println!(
                        "position_snapshot: run_id={} ts_ms={} market={} exchange={} symbol={} asset_class={} side={} qty={} available_qty={} avg_price={} mark_price={} market_value={} unrealized_pnl={} realized_pnl={} currency={} created_at_ms={}",
                        snapshot.run_id,
                        snapshot.ts_ms,
                        snapshot.market,
                        snapshot.exchange,
                        snapshot.symbol,
                        snapshot.asset_class,
                        snapshot.position_side.as_deref().unwrap_or(""),
                        snapshot.qty,
                        snapshot.available_qty,
                        snapshot.avg_price.as_deref().unwrap_or(""),
                        snapshot.mark_price.as_deref().unwrap_or(""),
                        snapshot.market_value.as_deref().unwrap_or(""),
                        snapshot.unrealized_pnl.as_deref().unwrap_or(""),
                        snapshot.realized_pnl.as_deref().unwrap_or(""),
                        snapshot.currency,
                        snapshot.created_at_ms
                    );
                }
            }
        },
        Command::Configs { command } => match command {
            ConfigsCommand::Create {
                config,
                name,
                file,
                created_by,
                parent_version,
                target_env,
                rollout,
                ts_ms,
            } => {
                let (_, db) = load_db(&config).await?;
                let content_json = compact_json_file(&file)?;
                let version = db
                    .create_config_version(storage::NewConfigVersion {
                        name: name.clone(),
                        content_json,
                        created_by,
                        parent_version,
                        target_env,
                        rollout,
                        ts_ms: ts_ms.unwrap_or_else(now_ms),
                    })
                    .await?;
                let config_version = db.get_config(&name, version).await?.ok_or_else(|| {
                    anyhow::anyhow!("created config version {name}:{version} was not found")
                })?;
                print_config_version(&config_version);
            }
            ConfigsCommand::List { config, name } => {
                let (_, db) = load_db(&config).await?;
                for config_version in db.list_config_versions(&name).await? {
                    print_config_version(&config_version);
                }
            }
            ConfigsCommand::Show {
                config,
                name,
                version,
                published,
            } => {
                if published && version.is_some() {
                    bail!("--published cannot be combined with --version");
                }
                let (_, db) = load_db(&config).await?;
                let config_version = if published {
                    db.get_published_config(&name).await?
                } else if let Some(version) = version {
                    db.get_config(&name, version).await?
                } else {
                    db.get_latest_config(&name).await?
                }
                .ok_or_else(|| anyhow::anyhow!("config {name} was not found"))?;
                print_config_version(&config_version);
                println!("config_content: {}", config_version.content_json);
            }
            ConfigsCommand::Diff {
                config,
                name,
                v1,
                v2,
            } => {
                let (_, db) = load_db(&config).await?;
                let diff = db.diff_configs(&name, v1, v2).await?;
                print_config_diff(&diff);
            }
            ConfigsCommand::Rollback {
                config,
                name,
                version,
                actor,
                reason,
                ts_ms,
            } => {
                let (_, db) = load_db(&config).await?;
                let rollback_version = db
                    .rollback_config_version(
                        &name,
                        version,
                        &actor,
                        reason.as_deref(),
                        ts_ms.unwrap_or_else(now_ms),
                    )
                    .await?;
                let config_version =
                    db.get_config(&name, rollback_version)
                        .await?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "rollback config version {name}:{rollback_version} was not found"
                            )
                        })?;
                print_config_version(&config_version);
            }
            ConfigsCommand::SubmitReview {
                config,
                name,
                version,
                changed_by,
                actor_role,
                reason,
                ts_ms,
            } => {
                transition_config_state(
                    &config,
                    &name,
                    version,
                    storage::ConfigState::PendingReview,
                    &changed_by,
                    actor_role.as_deref(),
                    reason.as_deref(),
                    ts_ms,
                )
                .await?;
            }
            ConfigsCommand::Approve {
                config,
                name,
                version,
                changed_by,
                actor_role,
                reason,
                ts_ms,
            } => {
                transition_config_state(
                    &config,
                    &name,
                    version,
                    storage::ConfigState::Approved,
                    &changed_by,
                    actor_role.as_deref(),
                    reason.as_deref(),
                    ts_ms,
                )
                .await?;
            }
            ConfigsCommand::Publish {
                config,
                name,
                version,
                changed_by,
                actor_role,
                reason,
                ts_ms,
            } => {
                transition_config_state(
                    &config,
                    &name,
                    version,
                    storage::ConfigState::Published,
                    &changed_by,
                    actor_role.as_deref(),
                    reason.as_deref(),
                    ts_ms,
                )
                .await?;
            }
            ConfigsCommand::Archive {
                config,
                name,
                version,
                changed_by,
                actor_role,
                reason,
                ts_ms,
            } => {
                transition_config_state(
                    &config,
                    &name,
                    version,
                    storage::ConfigState::Archived,
                    &changed_by,
                    actor_role.as_deref(),
                    reason.as_deref(),
                    ts_ms,
                )
                .await?;
            }
            ConfigsCommand::PendingApprovals { config, target_env } => {
                let (_, db) = load_db(&config).await?;
                for approval in db.list_config_approval_queue(target_env.as_deref()).await? {
                    println!(
                        "config_approval: name={} version={} state={} target_env={} rollout={} required_role={} required_approvals={} approval_count={} remaining_approvals={} changed_by={} changed_at_ms={}",
                        approval.config.name,
                        approval.config.version,
                        approval.config.state.as_str(),
                        approval.config.target_env.as_deref().unwrap_or(""),
                        approval.config.rollout.as_deref().unwrap_or(""),
                        approval.required_role,
                        approval.required_approvals,
                        approval.approval_count,
                        approval.remaining_approvals,
                        approval.config.state_changed_by,
                        approval.config.state_changed_at_ms
                    );
                }
            }
            ConfigsCommand::GovernancePolicy { config } => {
                let (_, db) = load_db(&config).await?;
                for rule in db.list_config_governance_policy().rules {
                    println!(
                        "config_governance_policy: target_env={} transition_to={} required_role={} required_approvals={} requires_independent_actor={}",
                        rule.target_env,
                        rule.transition_to.as_str(),
                        rule.required_role,
                        rule.required_approvals,
                        rule.requires_independent_actor
                    );
                }
            }
            ConfigsCommand::Releases { config, config_id } => {
                let (_, db) = load_db(&config).await?;
                let releases = db.list_config_releases(&config_id).await?;
                for release in releases {
                    println!(
                        "config_release: config_id={} version={} status={} released_by={} notes={} created_at_ms={} updated_at_ms={}",
                        release.config_id,
                        release.version,
                        release.status,
                        release.released_by.as_deref().unwrap_or(""),
                        release.notes.as_deref().unwrap_or(""),
                        release.created_at_ms,
                        release.updated_at_ms
                    );
                }
            }
            ConfigsCommand::Audits { config, config_id } => {
                let (_, db) = load_db(&config).await?;
                let audits = db.list_config_audits(&config_id).await?;
                for audit in audits {
                    println!(
                        "config_audit: config_id={} version={} action={} actor={} reason={} ts_ms={}",
                        audit.config_id,
                        audit.version.as_deref().unwrap_or(""),
                        audit.action,
                        audit.actor.as_deref().unwrap_or(""),
                        audit.reason.as_deref().unwrap_or(""),
                        audit.ts_ms
                    );
                }
            }
        },
        Command::MarketRules { command } => match command {
            MarketRulesCommand::Effective {
                config,
                market,
                exchange,
                asset_class,
                symbol,
                trading_day,
                at_ms,
            } => {
                let (_, db) = load_db(&config).await?;
                let at_ms = at_ms.unwrap_or_else(now_ms);
                println!(
                    "market_rule_effective: market={} exchange={} asset_class={} symbol={} trading_day={} at_ms={}",
                    market, exchange, asset_class, symbol, trading_day, at_ms
                );
                if let Some(rule) = db
                    .find_lot_size_rule(&market, &exchange, &asset_class, &symbol, at_ms)
                    .await?
                {
                    println!(
                        "market_rule_lot_size: id={} symbol={} lot_size={} min_qty={} min_notional={} effective_from_ms={} effective_to_ms={}",
                        rule.id,
                        rule.symbol.as_deref().unwrap_or(""),
                        rule.lot_size,
                        rule.min_qty,
                        rule.min_notional,
                        rule.effective_from_ms,
                        rule.effective_to_ms
                            .map(|value| value.to_string())
                            .unwrap_or_default()
                    );
                }
                if let Some(rule) = db
                    .find_price_limit_rule(&market, &exchange, &asset_class, &symbol, at_ms)
                    .await?
                {
                    println!(
                        "market_rule_price_limit: id={} symbol={} tick_size={} limit_up_bps={} limit_down_bps={} effective_from_ms={} effective_to_ms={}",
                        rule.id,
                        rule.symbol.as_deref().unwrap_or(""),
                        rule.tick_size,
                        rule.limit_up_bps.as_deref().unwrap_or(""),
                        rule.limit_down_bps.as_deref().unwrap_or(""),
                        rule.effective_from_ms,
                        rule.effective_to_ms
                            .map(|value| value.to_string())
                            .unwrap_or_default()
                    );
                }
                if let Some(rule) = db
                    .find_fee_rule_with_tiers(
                        &market,
                        &exchange,
                        &asset_class,
                        Some(&symbol),
                        at_ms,
                    )
                    .await?
                {
                    println!(
                        "market_rule_fee: id={} symbol={} volume_window={} maker_bps={} taker_bps={} minimum_fee={} tax_bps={} exchange_fee_bps={} tiers={} effective_from_ms={} effective_to_ms={}",
                        rule.rule.id,
                        rule.rule.symbol.as_deref().unwrap_or(""),
                        rule.rule.volume_window,
                        rule.rule.maker_bps,
                        rule.rule.taker_bps,
                        rule.rule.minimum_fee.as_deref().unwrap_or(""),
                        rule.rule.tax_bps.as_deref().unwrap_or(""),
                        rule.rule.exchange_fee_bps.as_deref().unwrap_or(""),
                        rule.tiers.len(),
                        rule.rule.effective_from_ms,
                        rule.rule
                            .effective_to_ms
                            .map(|value| value.to_string())
                            .unwrap_or_default()
                    );
                    for tier in rule.tiers {
                        println!(
                            "market_rule_fee_tier: id={} fee_rule_id={} volume_from={} volume_to={} maker_bps={} taker_bps={}",
                            tier.id,
                            tier.fee_rule_id,
                            tier.volume_from,
                            tier.volume_to.as_deref().unwrap_or(""),
                            tier.maker_bps,
                            tier.taker_bps
                        );
                    }
                }
                if let Some(calendar) = db.find_market_calendar(&market, &trading_day).await? {
                    println!(
                        "market_rule_calendar: id={} market={} trading_day={} is_open={} session_template={}",
                        calendar.id,
                        calendar.market,
                        calendar.trading_day,
                        calendar.is_open,
                        calendar.session_template.as_deref().unwrap_or("")
                    );
                }
                for session in db.list_trading_session_rules(&market, &trading_day).await? {
                    println!(
                        "market_rule_trading_session: id={} market={} trading_day={} session_name={} open_time={} close_time={} timezone={}",
                        session.id,
                        session.market,
                        session.trading_day,
                        session.session_name,
                        session.open_time,
                        session.close_time,
                        session.timezone
                    );
                }
            }
            MarketRulesCommand::Audits {
                config,
                rule_type,
                rule_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let (_, db) = load_db(&config).await?;
                let events = db
                    .list_market_rule_audit_events(storage::MarketRuleAuditFilter {
                        rule_type,
                        rule_id,
                        from_ms,
                        to_ms,
                        limit,
                    })
                    .await?;
                for event in events {
                    println!(
                        "market_rule_audit: source={} category={} ts_ms={} event_id={} payload={}",
                        event.source,
                        event.category,
                        event.ts_ms,
                        event.event_id,
                        event.payload_json
                    );
                }
            }
        },
        Command::Runs { command } => match command {
            RunsCommand::ConfigVersion { config, run_id } => {
                let (_, db) = load_db(&config).await?;
                if let Some(binding) = db.get_run_config_version_binding(&run_id).await? {
                    println!(
                        "run_config_version: run_id={} config_id={} version={} bound_at_ms={}",
                        binding.run_id, binding.config_id, binding.version, binding.bound_at_ms
                    );
                } else {
                    println!("run_config_version: run_id={} status=missing", run_id);
                }
            }
        },
        Command::Logs { command } => match command {
            LogsCommand::List {
                config,
                run_id,
                level,
                target,
                from_ms,
                to_ms,
                search,
                limit,
                offset,
            } => {
                let (_, db) = load_db(&config).await?;
                let logs = db
                    .list_system_logs_filtered(build_system_log_filter(
                        run_id,
                        level,
                        target,
                        from_ms,
                        to_ms,
                        search,
                        limit,
                        Some(offset),
                    ))
                    .await?;
                for log in logs {
                    print_system_log(&log);
                }
            }
            LogsCommand::Count {
                config,
                run_id,
                level,
                target,
                from_ms,
                to_ms,
                search,
            } => {
                let (_, db) = load_db(&config).await?;
                let count = db
                    .count_system_logs(build_system_log_filter(
                        run_id, level, target, from_ms, to_ms, search, None, None,
                    ))
                    .await?;
                println!("system_logs_count: count={count}");
            }
            LogsCommand::Metrics { config } => {
                let app_config = config::AppConfig::from_toml_file(&config)?;
                let settings = log_writer_settings(&app_config);
                let categories = if settings.categories.is_empty() {
                    "all".to_string()
                } else {
                    settings.categories.join(",")
                };
                println!(
                    "logging_metrics: dropped_logs={} enabled={} level={} categories={} buffer_size={} batch_size={} flush_interval_ms={}",
                    settings.metrics.dropped_logs(),
                    settings.enabled,
                    settings.min_level,
                    categories,
                    settings.buffer_size,
                    settings.batch_size,
                    settings.flush_interval_ms
                );
            }
            LogsCommand::Tail {
                config,
                run_id,
                level,
                target,
                from_ms,
                to_ms,
                search,
                poll_interval_ms,
                max_polls,
                limit,
            } => {
                let (_, db) = load_db(&config).await?;
                let mut next_from_ms = from_ms;
                let mut seen_ids = BTreeSet::new();
                for poll_index in 0..max_polls {
                    let logs = db
                        .list_system_logs_filtered(build_system_log_filter(
                            run_id.clone(),
                            level.clone(),
                            target.clone(),
                            next_from_ms,
                            to_ms,
                            search.clone(),
                            Some(limit),
                            Some(0),
                        ))
                        .await?;
                    let mut latest_ts_ms = next_from_ms.unwrap_or_default();
                    for log in logs {
                        latest_ts_ms = latest_ts_ms.max(log.ts_ms);
                        if seen_ids.insert(log.id.clone()) {
                            print_system_log(&log);
                        }
                    }
                    next_from_ms = Some(latest_ts_ms.saturating_add(1));
                    if poll_index + 1 < max_polls {
                        tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;
                    }
                }
            }
            LogsCommand::Export {
                config,
                output,
                run_id,
                level,
                target,
                from_ms,
                to_ms,
                search,
                limit,
                offset,
            } => {
                let (_, db) = load_db(&config).await?;
                let logs = db
                    .list_system_logs_filtered(build_system_log_filter(
                        run_id,
                        level,
                        target,
                        from_ms,
                        to_ms,
                        search,
                        limit,
                        Some(offset),
                    ))
                    .await?;
                let mut file = std::fs::File::create(&output)
                    .with_context(|| format!("failed to create log export file {output}"))?;
                for log in &logs {
                    writeln!(file, "{}", system_log_json(log))?;
                }
                println!("system_logs_exported: count={} path={output}", logs.len());
            }
            LogsCommand::Ship {
                config,
                collector_url,
                bearer_token,
                signature_secret_env,
                max_retries,
                retry_backoff_ms,
                run_id,
                level,
                target,
                from_ms,
                to_ms,
                search,
                limit,
                offset,
            } => {
                let (_, db) = load_db(&config).await?;
                let logs = db
                    .list_system_logs_filtered(build_system_log_filter(
                        run_id,
                        level,
                        target,
                        from_ms,
                        to_ms,
                        search,
                        limit,
                        Some(offset),
                    ))
                    .await?;
                let mut body = String::new();
                for log in &logs {
                    body.push_str(&system_log_json(log).to_string());
                    body.push('\n');
                }
                let signature_secret = signature_secret_env
                    .as_deref()
                    .map(|env_name| {
                        std::env::var(env_name).with_context(|| {
                            format!("failed to read signature secret env {env_name}")
                        })
                    })
                    .transpose()?;
                let (status, attempts) = ship_system_logs_with_retry(
                    &collector_url,
                    bearer_token.as_deref(),
                    signature_secret.as_deref(),
                    body,
                    max_retries,
                    retry_backoff_ms,
                )
                .await?;
                println!(
                    "system_logs_shipped: count={} status={} attempts={}",
                    logs.len(),
                    status.as_u16(),
                    attempts
                );
            }
            LogsCommand::Purge {
                config,
                before_ms,
                target,
                run_id,
            } => {
                let (_, db) = load_db(&config).await?;
                let purged = db
                    .purge_system_logs(storage::SystemLogRetentionCommand {
                        before_ms,
                        target,
                        run_id,
                    })
                    .await?;
                println!("system_logs_purged: count={purged}");
            }
        },
        Command::Reconciliation { config, run_id } => {
            let (_, db) = load_db(&config).await?;
            let cash_snapshots = db.list_cash_snapshots(&run_id).await?;
            let position_snapshots = db.list_position_snapshots(&run_id).await?;
            let reconciliation_audits = db.list_reconciliation_audits(&run_id).await?;
            let latest_audit = reconciliation_audits.last();
            let drift_events = db
                .list_risk_events(&run_id)
                .await?
                .into_iter()
                .filter(|event| event.risk_type == "reconciliation_drift")
                .collect::<Vec<_>>();
            let status = if drift_events.is_empty() {
                "ok"
            } else {
                "drift"
            };
            println!(
                "reconciliation: run_id={} status={} cash_snapshots={} position_snapshots={} reconciliation_audits={} latest_audit_broker={} latest_audit_account={} latest_audit_severity={} drift_events={}",
                run_id,
                status,
                cash_snapshots.len(),
                position_snapshots.len(),
                reconciliation_audits.len(),
                latest_audit
                    .map(|audit| audit.broker_kind.as_str())
                    .unwrap_or(""),
                latest_audit
                    .map(|audit| audit.account_id.as_str())
                    .unwrap_or(""),
                latest_audit
                    .map(|audit| audit.severity.as_str())
                    .unwrap_or(""),
                drift_events.len()
            );
            for event in drift_events {
                println!(
                    "reconciliation_drift: ts_ms={} account={} symbol={} decision={} reason={} threshold={} observed_value={}",
                    event.ts_ms,
                    event.account_id.as_deref().unwrap_or(""),
                    event.symbol.as_deref().unwrap_or(""),
                    event.decision,
                    event.reason.as_deref().unwrap_or(""),
                    event.threshold.as_deref().unwrap_or(""),
                    event.observed_value.as_deref().unwrap_or("")
                );
            }
        }
        Command::ReconciliationGate {
            config,
            accounts,
            min_successful_audits,
            max_audit_age_ms,
        } => {
            run_reconciliation_gate(&config, accounts, min_successful_audits, max_audit_age_ms)
                .await?
        }
        Command::OrderEvents {
            config,
            run_id,
            order_id,
            client_order_id,
            broker_order_id,
            account_id,
            symbol,
            status,
            event_type,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let events = db
                .list_order_events_filtered(storage::OrderEventFilter {
                    run_id,
                    order_id,
                    client_order_id,
                    broker_order_id,
                    account_id,
                    symbol,
                    status,
                    event_type,
                    from_ms,
                    to_ms,
                    limit,
                })
                .await?;
            for event in events {
                println!(
                    "order_event: run_id={} ts_ms={} order_id={} client_order_id={} broker_order_id={} account={} symbol={} status={} event_type={} message={}",
                    event.run_id,
                    event.ts_ms,
                    event.order_id.as_deref().unwrap_or(""),
                    event.client_order_id.as_deref().unwrap_or(""),
                    event.broker_order_id.as_deref().unwrap_or(""),
                    event.account_id.as_deref().unwrap_or(""),
                    event.symbol.as_deref().unwrap_or(""),
                    event.status,
                    event.event_type,
                    event.message.as_deref().unwrap_or("")
                );
            }
        }
        Command::RiskEvents {
            config,
            run_id,
            risk_type,
            decision,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let events = db
                .list_risk_events_filtered(storage::RiskEventFilter {
                    run_id,
                    risk_type,
                    decision,
                    account_id,
                    symbol,
                    from_ms,
                    to_ms,
                    limit,
                })
                .await?;
            for event in events {
                println!(
                    "risk_event: run_id={} ts_ms={} account={} symbol={} risk_type={} decision={} reason={} threshold={} observed_value={}",
                    event.run_id,
                    event.ts_ms,
                    event.account_id.as_deref().unwrap_or(""),
                    event.symbol.as_deref().unwrap_or(""),
                    event.risk_type,
                    event.decision,
                    event.reason.as_deref().unwrap_or(""),
                    event.threshold.as_deref().unwrap_or(""),
                    event.observed_value.as_deref().unwrap_or("")
                );
            }
        }
        Command::ReconciliationDrifts {
            config,
            run_id,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let drift_events = db
                .list_risk_events_filtered(storage::RiskEventFilter {
                    run_id,
                    risk_type: Some("reconciliation_drift".to_string()),
                    decision: None,
                    account_id,
                    symbol,
                    from_ms,
                    to_ms,
                    limit,
                })
                .await?;
            for event in drift_events {
                println!(
                    "reconciliation_drift: run_id={} ts_ms={} account={} symbol={} decision={} reason={} threshold={} observed_value={}",
                    event.run_id,
                    event.ts_ms,
                    event.account_id.as_deref().unwrap_or(""),
                    event.symbol.as_deref().unwrap_or(""),
                    event.decision,
                    event.reason.as_deref().unwrap_or(""),
                    event.threshold.as_deref().unwrap_or(""),
                    event.observed_value.as_deref().unwrap_or("")
                );
            }
        }
        Command::ReconciliationAlertsSummary {
            config,
            run_id,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id: run_id.clone(),
                    level: None,
                    target: Some("runtime.alert".to_string()),
                    from_ms,
                    to_ms,
                    search: None,
                    limit,
                    offset: None,
                })
                .await?;
            let mut alert_count = 0usize;
            let mut latest_alert_ts_ms = None;
            let mut runs = BTreeSet::new();
            let mut accounts = BTreeSet::new();
            let mut symbols = BTreeSet::new();
            let mut reasons = BTreeSet::new();
            for log in logs {
                if log.message != "reconciliation_drift.alert" {
                    continue;
                }
                let fields = log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                let log_account_id = fields
                    .get("account_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let log_symbol = fields
                    .get("symbol")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let log_reason = fields
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                if account_id
                    .as_deref()
                    .is_some_and(|expected| log_account_id.as_deref() != Some(expected))
                {
                    continue;
                }
                if symbol
                    .as_deref()
                    .is_some_and(|expected| log_symbol.as_deref() != Some(expected))
                {
                    continue;
                }
                alert_count += 1;
                latest_alert_ts_ms = Some(
                    latest_alert_ts_ms.map_or(log.ts_ms, |current: i64| current.max(log.ts_ms)),
                );
                if let Some(run_id) = log.run_id {
                    runs.insert(run_id);
                }
                if let Some(account_id) = log_account_id {
                    accounts.insert(account_id);
                }
                if let Some(symbol) = log_symbol {
                    symbols.insert(symbol);
                }
                if let Some(reason) = log_reason {
                    reasons.insert(reason);
                }
            }
            println!(
                "reconciliation_alert_summary: run_id={} alert_count={} latest_alert_ts_ms={} runs={} accounts={} symbols={} reasons={}",
                run_id.as_deref().unwrap_or("*"),
                alert_count,
                latest_alert_ts_ms
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                runs.into_iter().collect::<Vec<_>>().join(","),
                accounts.into_iter().collect::<Vec<_>>().join(","),
                symbols.into_iter().collect::<Vec<_>>().join(","),
                reasons.into_iter().collect::<Vec<_>>().join(","),
            );
        }
        Command::ReconciliationGateAlertsSummary {
            config,
            run_id,
            account_id,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id: run_id.clone(),
                    level: Some("ERROR".to_string()),
                    target: Some("runtime.alert".to_string()),
                    from_ms,
                    to_ms,
                    search: Some("reconciliation_gate.block.alert".to_string()),
                    limit,
                    offset: None,
                })
                .await?;
            let mut block_count = 0usize;
            let mut latest_block_ts_ms = None;
            let mut runs = BTreeSet::new();
            let mut accounts = BTreeSet::new();
            let mut brokers = BTreeSet::new();
            let mut reasons = BTreeSet::new();
            for log in logs {
                if log.message != "reconciliation_gate.block.alert" {
                    continue;
                }
                let fields = log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                let failure_account_matches = gate_failure_values(&fields, "account_id")
                    .into_iter()
                    .any(|value| account_id.as_deref() == Some(value.as_str()));
                if account_id
                    .as_deref()
                    .is_some_and(|_| !failure_account_matches)
                {
                    continue;
                }
                block_count += 1;
                latest_block_ts_ms = Some(
                    latest_block_ts_ms.map_or(log.ts_ms, |current: i64| current.max(log.ts_ms)),
                );
                if let Some(run_id) = log.run_id {
                    runs.insert(run_id);
                }
                for account in gate_failure_values(&fields, "account_id") {
                    accounts.insert(account);
                }
                for broker in gate_failure_values(&fields, "broker") {
                    brokers.insert(broker);
                }
                for reason in gate_failure_values(&fields, "reason") {
                    reasons.insert(reason);
                }
            }
            println!(
                "reconciliation_gate_alert_summary: run_id={} block_count={} latest_block_ts_ms={} runs={} accounts={} brokers={} reasons={}",
                run_id.as_deref().unwrap_or("*"),
                block_count,
                latest_block_ts_ms
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                runs.into_iter().collect::<Vec<_>>().join(","),
                accounts.into_iter().collect::<Vec<_>>().join(","),
                brokers.into_iter().collect::<Vec<_>>().join(","),
                reasons.into_iter().collect::<Vec<_>>().join(","),
            );
        }
        Command::ReconciliationAlertsExport {
            config,
            output,
            run_id,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id: run_id.clone(),
                    level: None,
                    target: Some("runtime.alert".to_string()),
                    from_ms,
                    to_ms,
                    search: None,
                    limit,
                    offset: None,
                })
                .await?;
            let mut file = std::fs::File::create(&output)
                .with_context(|| format!("failed to create alert export file {output}"))?;
            let mut exported = 0usize;
            for log in logs {
                if log.message != "reconciliation_drift.alert" {
                    continue;
                }
                let fields = log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                let log_account_id = fields.get("account_id").and_then(serde_json::Value::as_str);
                let log_symbol = fields.get("symbol").and_then(serde_json::Value::as_str);
                if account_id
                    .as_deref()
                    .is_some_and(|expected| log_account_id != Some(expected))
                {
                    continue;
                }
                if symbol
                    .as_deref()
                    .is_some_and(|expected| log_symbol != Some(expected))
                {
                    continue;
                }
                let dedup_key = format!(
                    "{}|{}|{}|{}|{}",
                    log.message,
                    log.run_id.as_deref().unwrap_or(""),
                    log_account_id.unwrap_or(""),
                    log_symbol.unwrap_or(""),
                    fields
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                );
                writeln!(
                    file,
                    "{}",
                    serde_json::json!({
                        "run_id": log.run_id,
                        "ts_ms": log.ts_ms,
                        "level": log.level,
                        "target": log.target,
                        "message": log.message,
                        "account_id": log_account_id,
                        "symbol": log_symbol,
                        "reason": fields.get("reason").and_then(serde_json::Value::as_str),
                        "dedup_key": dedup_key,
                        "fields": fields,
                        "created_at_ms": log.created_at_ms,
                    })
                )?;
                exported += 1;
            }
            println!(
                "reconciliation_alerts_exported: count={} path={output}",
                exported
            );
        }
        Command::ReconciliationAlertDeliveriesSummary {
            config,
            run_id,
            alert_message,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id: run_id.clone(),
                    level: None,
                    target: Some("runtime.alert_delivery".to_string()),
                    from_ms,
                    to_ms,
                    search: alert_message.clone(),
                    limit,
                    offset: None,
                })
                .await?;
            let mut delivery_count = 0usize;
            let mut latest_delivery_ts_ms = None;
            let mut sent_count = 0usize;
            let mut failed_count = 0usize;
            let mut statuses = BTreeSet::new();
            let mut sinks = BTreeSet::new();
            let mut alert_messages = BTreeSet::new();
            for log in logs {
                if log.message != "alert.delivery" {
                    continue;
                }
                let fields = log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                let log_account_id = fields
                    .get("account_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let log_symbol = fields
                    .get("symbol")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let log_alert_message = fields
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                if alert_message
                    .as_deref()
                    .is_some_and(|expected| log_alert_message.as_deref() != Some(expected))
                {
                    continue;
                }
                if account_id
                    .as_deref()
                    .is_some_and(|expected| log_account_id.as_deref() != Some(expected))
                {
                    continue;
                }
                if symbol
                    .as_deref()
                    .is_some_and(|expected| log_symbol.as_deref() != Some(expected))
                {
                    continue;
                }
                delivery_count += 1;
                latest_delivery_ts_ms = Some(
                    latest_delivery_ts_ms.map_or(log.ts_ms, |current: i64| current.max(log.ts_ms)),
                );
                if let Some(status) = fields.get("status").and_then(serde_json::Value::as_str) {
                    if status == "sent" {
                        sent_count += 1;
                    }
                    if status == "failed" {
                        failed_count += 1;
                    }
                    statuses.insert(status.to_string());
                }
                if let Some(sink) = fields.get("sink").and_then(serde_json::Value::as_str) {
                    sinks.insert(sink.to_string());
                }
                if let Some(message) = log_alert_message {
                    alert_messages.insert(message);
                }
            }
            println!(
                "reconciliation_alert_delivery_summary: run_id={} alert_message={} delivery_count={} latest_delivery_ts_ms={} sent_count={} failed_count={} sinks={} statuses={} alert_messages={}",
                run_id.as_deref().unwrap_or("*"),
                alert_message.as_deref().unwrap_or("*"),
                delivery_count,
                latest_delivery_ts_ms
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                sent_count,
                failed_count,
                sinks.into_iter().collect::<Vec<_>>().join(","),
                statuses.into_iter().collect::<Vec<_>>().join(","),
                alert_messages.into_iter().collect::<Vec<_>>().join(","),
            );
        }
        Command::ReconciliationAlertDeliveriesExport {
            config,
            output,
            run_id,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id: run_id.clone(),
                    level: None,
                    target: Some("runtime.alert_delivery".to_string()),
                    from_ms,
                    to_ms,
                    search: None,
                    limit,
                    offset: None,
                })
                .await?;
            let mut file = std::fs::File::create(&output)
                .with_context(|| format!("failed to create alert delivery export file {output}"))?;
            let mut exported = 0usize;
            for log in logs {
                if log.message != "alert.delivery" {
                    continue;
                }
                let fields = log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                let log_account_id = fields.get("account_id").and_then(serde_json::Value::as_str);
                let log_symbol = fields.get("symbol").and_then(serde_json::Value::as_str);
                if account_id
                    .as_deref()
                    .is_some_and(|expected| log_account_id != Some(expected))
                {
                    continue;
                }
                if symbol
                    .as_deref()
                    .is_some_and(|expected| log_symbol != Some(expected))
                {
                    continue;
                }
                writeln!(
                    file,
                    "{}",
                    serde_json::json!({
                        "run_id": log.run_id,
                        "ts_ms": log.ts_ms,
                        "level": log.level,
                        "target": log.target,
                        "message": log.message,
                        "account_id": log_account_id,
                        "symbol": log_symbol,
                        "sink": fields.get("sink").and_then(serde_json::Value::as_str),
                        "status": fields.get("status").and_then(serde_json::Value::as_str),
                        "attempts": fields.get("attempts").and_then(serde_json::Value::as_u64),
                        "http_status": fields.get("http_status").and_then(serde_json::Value::as_u64),
                        "error": fields.get("error").and_then(serde_json::Value::as_str),
                        "dedup_key": fields.get("dedup_key").and_then(serde_json::Value::as_str),
                        "fields": fields,
                        "created_at_ms": log.created_at_ms,
                    })
                )?;
                exported += 1;
            }
            println!(
                "reconciliation_alert_deliveries_exported: count={} path={output}",
                exported
            );
        }
        Command::ReconciliationAlertRedeliver {
            config,
            webhook_url,
            auth_token,
            run_id,
            account_id,
            symbol,
            from_ms,
            to_ms,
            limit,
        } => {
            let (_, db) = load_db(&config).await?;
            let delivery_logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id: run_id.clone(),
                    level: None,
                    target: Some("runtime.alert_delivery".to_string()),
                    from_ms,
                    to_ms,
                    search: None,
                    limit,
                    offset: None,
                })
                .await?;
            let alert_logs = db
                .list_system_logs_filtered(storage::SystemLogFilter {
                    run_id,
                    level: None,
                    target: Some("runtime.alert".to_string()),
                    from_ms,
                    to_ms,
                    search: None,
                    limit: None,
                    offset: None,
                })
                .await?;
            let client = reqwest::Client::new();
            let mut redelivered = 0usize;
            for delivery_log in delivery_logs {
                if delivery_log.message != "alert.delivery" {
                    continue;
                }
                let delivery_fields = delivery_log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                if delivery_fields
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    != Some("failed")
                {
                    continue;
                }
                let delivery_account_id = delivery_fields
                    .get("account_id")
                    .and_then(serde_json::Value::as_str);
                let delivery_symbol = delivery_fields
                    .get("symbol")
                    .and_then(serde_json::Value::as_str);
                if account_id
                    .as_deref()
                    .is_some_and(|expected| delivery_account_id != Some(expected))
                {
                    continue;
                }
                if symbol
                    .as_deref()
                    .is_some_and(|expected| delivery_symbol != Some(expected))
                {
                    continue;
                }
                let Some(delivery_dedup_key) = delivery_fields
                    .get("dedup_key")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(alert_log) = alert_logs.iter().find(|log| {
                    if log.message != "reconciliation_drift.alert" {
                        return false;
                    }
                    let alert_fields = log
                        .fields_json
                        .as_deref()
                        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                        .unwrap_or(serde_json::Value::Null);
                    alert_dedup_key_for_cli(
                        &log.message,
                        log.run_id.as_deref().unwrap_or(""),
                        &alert_fields,
                    ) == delivery_dedup_key
                }) else {
                    continue;
                };
                let alert_fields = alert_log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                let payload = serde_json::json!({
                    "ts_ms": alert_log.ts_ms,
                    "run_id": alert_log.run_id,
                    "target": alert_log.target,
                    "message": alert_log.message,
                    "dedup_key": delivery_dedup_key,
                    "fields": alert_fields,
                });
                let mut request = client.post(&webhook_url).json(&payload);
                if let Some(token) = auth_token.as_deref() {
                    request = request.bearer_auth(token);
                }
                request.send().await?.error_for_status()?;
                redelivered += 1;
            }
            println!(
                "reconciliation_alerts_redelivered: count={redelivered} webhook_url={webhook_url}"
            );
        }
        Command::Funding { command } => match command {
            FundingCommand::List {
                config,
                exchange,
                symbol,
                from_ms,
                to_ms,
            } => {
                let (_, db) = load_db(&config).await?;
                let rates = db
                    .list_funding_rates(&exchange, symbol.as_deref(), from_ms, to_ms)
                    .await?;
                for rate in rates {
                    println!(
                        "funding_rate: exchange={} symbol={} funding_time_ms={} funding_rate={} mark_price={} source={} id={}",
                        rate.exchange,
                        rate.symbol,
                        rate.funding_time_ms,
                        rate.funding_rate,
                        rate.mark_price.as_deref().unwrap_or(""),
                        rate.source,
                        rate.id
                    );
                }
            }
        },
        Command::Ingest { command } => match command {
            IngestCommand::BinanceMeta { config, exchange } => {
                ensure_supported_ingestion_exchange(&exchange)?;
                let (_, db) = load_db(&config).await?;
                db.migrate().await?;
                let client = reqwest::Client::new();
                let started = Instant::now();
                let result =
                    data::ingestion::binance_meta::ingest_binance_market_meta(&db, &client).await?;
                log_and_print_ingestion(&db, result, started).await?;
            }
            IngestCommand::FundingRates {
                config,
                exchange,
                symbol,
            } => {
                ensure_supported_ingestion_exchange(&exchange)?;
                let (_, db) = load_db(&config).await?;
                db.migrate().await?;
                let client = reqwest::Client::new();
                let started = Instant::now();
                let result = data::ingestion::binance_funding::ingest_binance_funding_rates(
                    &db, &client, &symbol,
                )
                .await?;
                log_and_print_ingestion(&db, result, started).await?;
            }
            IngestCommand::CorporateActions { config, symbol } => {
                let (_, db) = load_db(&config).await?;
                db.migrate().await?;
                let client = reqwest::Client::new();
                let started = Instant::now();
                let result = data::ingestion::corporate_actions::ingest_yahoo_corporate_actions(
                    &db, &client, &symbol,
                )
                .await?;
                log_and_print_ingestion(&db, result, started).await?;
            }
            IngestCommand::Status { config } => {
                let (_, db) = load_db(&config).await?;
                db.migrate().await?;
                let statuses = data::ingestion::tracker::last_ingestions_with_staleness(
                    &db,
                    chrono::Utc::now().timestamp_millis(),
                )
                .await?;
                for status in statuses {
                    println!(
                        "ingestion_status: source={} table={} ts_ms={} age_ms={} stale_after_ms={} is_stale={} rows_fetched={} rows_upserted={} duration_ms={}",
                        status.source,
                        status.table,
                        status.ts_ms,
                        status.age_ms,
                        status.stale_after_ms,
                        status.is_stale,
                        status.rows_fetched,
                        status.rows_upserted,
                        status.duration_ms
                    );
                }
            }
        },
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
        port: app_config.broker.port.unwrap_or(4002),
        client_id: app_config.broker.client_id.unwrap_or(1),
        connect_timeout: Duration::from_millis(
            app_config.broker.connect_timeout_ms.unwrap_or(15_000),
        ),
    })
}

async fn paper_real_broker_connection_ready(app_config: &config::AppConfig) -> Result<bool> {
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
            if !app_config.broker.order_submit_enabled {
                return Ok(false);
            }
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(app_config)?)?;
            adapter
                .validate_paper_account(&app_config.paper.account_id)
                .await?;
            let market_data_provider =
                configured_market_data_provider(app_config, adapter.clone())?;
            run_realtime_market_data_gate(market_data_provider.as_ref(), app_config).await?;
            Ok(true)
        }
        config::BrokerKind::Futu | config::BrokerKind::Okx => Ok(false),
    }
}

async fn run_ibkr_market_data_probe(
    adapter: &IbkrPaperGatewayAdapter,
    app_config: &config::AppConfig,
    requested_symbols: &[String],
    delayed: bool,
    emit_snapshots: bool,
) -> Result<usize> {
    let symbols = ibkr_market_data_probe_symbols(app_config, requested_symbols)?;
    let expected_market_data_type = if delayed { "delayed" } else { "realtime" };
    let route_exchange = app_config.broker.ibkr_route_exchange.as_deref();
    let mut failures = Vec::new();

    for symbol in &symbols {
        let snapshot_result = if delayed {
            adapter
                .delayed_market_data_snapshot(symbol, route_exchange)
                .await
        } else {
            adapter.market_data_snapshot(symbol, route_exchange).await
        };
        let snapshot = match snapshot_result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!(
                    "IBKR paper market data probe failed: symbol={} type={} error={}",
                    symbol, expected_market_data_type, error
                );
                failures.push(format!("{symbol}: {error}"));
                continue;
            }
        };
        if emit_snapshots {
            println!("{}", serde_json::to_string(&snapshot)?);
        }
        if let Err(error) = paper::validate_ibkr_market_data_snapshot(
            &snapshot,
            expected_market_data_type,
            now_ms(),
        ) {
            eprintln!(
                "IBKR paper market data probe rejected: symbol={} type={} error={}",
                symbol, expected_market_data_type, error
            );
            failures.push(format!("{symbol}: {error}"));
        }
    }

    if !failures.is_empty() {
        bail!(
            "IBKR paper market data probe failed for {} {} snapshot(s): {}",
            failures.len(),
            expected_market_data_type,
            failures.join("; ")
        );
    }
    Ok(symbols.len())
}

async fn run_market_data_probe(
    provider: &dyn MarketDataProvider,
    app_config: &config::AppConfig,
    requested_symbols: &[String],
    emit_snapshots: bool,
) -> Result<usize> {
    let symbols = ibkr_market_data_probe_symbols(app_config, requested_symbols)?;
    let mut failures = Vec::new();

    for symbol in &symbols {
        let quote = match provider.snapshot(symbol).await {
            Ok(quote) => quote,
            Err(error) => {
                eprintln!("market data probe failed: symbol={symbol} error={error}");
                failures.push(format!("{symbol}: {error}"));
                continue;
            }
        };
        if emit_snapshots {
            println!("{}", serde_json::to_string(&quote)?);
        }
        if let Err(error) =
            paper::validate_market_data_quote(&quote, &data::MarketDataKind::Realtime, now_ms())
        {
            eprintln!("market data probe rejected: symbol={symbol} error={error}");
            failures.push(format!("{symbol}: {error}"));
        }
    }

    if !failures.is_empty() {
        bail!(
            "market data probe failed for {} realtime snapshot(s): {}",
            failures.len(),
            failures.join("; ")
        );
    }
    Ok(symbols.len())
}

fn ibkr_market_data_probe_symbols(
    app_config: &config::AppConfig,
    requested_symbols: &[String],
) -> Result<Vec<String>> {
    let source = if requested_symbols.is_empty() {
        &app_config.strategy.symbols
    } else {
        requested_symbols
    };
    let mut seen = BTreeSet::new();
    let mut symbols = Vec::new();
    for symbol in source {
        let symbol = paper::ibkr_stock_symbol(symbol)?;
        if seen.insert(symbol.clone()) {
            symbols.push(symbol);
        }
    }
    if symbols.is_empty() {
        bail!("IBKR paper market data probe requires at least one strategy or --symbol value");
    }
    Ok(symbols)
}

fn market_data_provider_for_probe(
    app_config: &config::AppConfig,
) -> Result<Box<dyn MarketDataProvider>> {
    match app_config.market_data.provider {
        config::MarketDataProviderKind::Ibkr => {
            ensure_ibkr_paper_config(app_config, "market data probe")?;
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(app_config)?)?;
            configured_market_data_provider(app_config, adapter)
        }
        config::MarketDataProviderKind::Longbridge => {
            let settings = LongbridgeMarketDataSettings::new(
                &app_config.market_data.longbridge_app_key_env,
                &app_config.market_data.longbridge_app_secret_env,
                &app_config.market_data.longbridge_access_token_env,
            );
            Ok(Box::new(LongbridgeMarketDataProvider::from_env(&settings)?))
        }
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
        config::BrokerKind::InteractiveBrokers => {
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(app_config)?)?;
            adapter
                .validate_paper_account(&app_config.paper.account_id)
                .await?;
            let market_data_provider =
                configured_market_data_provider(app_config, adapter.clone())?;
            run_realtime_market_data_gate(market_data_provider.as_ref(), app_config).await?;
            Ok(PaperRuntime::new_with_executor(
                db,
                settings,
                Box::new(
                    IbkrPaperOrderExecutor::new_with_client_order_prefix(
                        market_data_provider,
                        IbkrPaperGatewayOrderClient::new(
                            adapter,
                            app_config.paper.account_id.clone(),
                        ),
                        app_config.runtime.run_id.clone(),
                    )
                    .with_route_exchange(app_config.broker.ibkr_route_exchange.clone())
                    .with_override_percentage_constraints(
                        app_config.broker.ibkr_override_percentage_constraints,
                    ),
                ),
            ))
        }
        config::BrokerKind::Simulated | config::BrokerKind::Futu | config::BrokerKind::Okx => {
            bail!(
                "paper-run broker order submit only supports Binance Spot Testnet and IBKR paper in this phase"
            )
        }
    }
}

fn configured_market_data_provider(
    app_config: &config::AppConfig,
    ibkr_adapter: IbkrPaperGatewayAdapter,
) -> Result<Box<dyn MarketDataProvider>> {
    match app_config.market_data.provider {
        config::MarketDataProviderKind::Ibkr => Ok(Box::new(IbkrMarketDataProvider::new(
            ibkr_adapter,
            app_config.broker.ibkr_route_exchange.clone(),
        ))),
        config::MarketDataProviderKind::Longbridge => {
            let settings = LongbridgeMarketDataSettings::new(
                &app_config.market_data.longbridge_app_key_env,
                &app_config.market_data.longbridge_app_secret_env,
                &app_config.market_data.longbridge_access_token_env,
            );
            Ok(Box::new(LongbridgeMarketDataProvider::from_env(&settings)?))
        }
    }
}

async fn run_realtime_market_data_gate(
    provider: &dyn MarketDataProvider,
    app_config: &config::AppConfig,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();

    for configured_symbol in &app_config.strategy.symbols {
        let symbol = paper::ibkr_stock_symbol(configured_symbol)?;
        if !seen.insert(symbol.clone()) {
            continue;
        }
        let quote = match provider.snapshot(&symbol).await {
            Ok(quote) => quote,
            Err(error) => {
                failures.push(format!("{symbol}: {error}"));
                continue;
            }
        };
        if let Err(error) =
            paper::validate_market_data_quote(&quote, &data::MarketDataKind::Realtime, now_ms())
        {
            failures.push(format!("{symbol}: {error}"));
        }
    }

    if seen.is_empty() {
        bail!("paper market data gate requires at least one configured strategy symbol");
    }
    if !failures.is_empty() {
        bail!(
            "paper market data gate failed for {} realtime snapshot(s): {}",
            failures.len(),
            failures.join("; ")
        );
    }
    Ok(())
}

fn settings_with_broker_initial_cash(
    mut settings: PaperSettings,
    broker_cash: Decimal,
) -> PaperSettings {
    settings.initial_cash = broker_cash;
    settings
}

async fn sync_cancelled_open_orders(
    db: &storage::Db,
    run_id: &str,
    cancelled: &[broker::CancelledOpenOrder],
) -> Result<u64> {
    let mut local_synced = 0u64;
    let updated_at_ms = chrono::Utc::now().timestamp_millis();
    for cancellation in cancelled {
        let status = broker_order_status_label(cancellation.cancelled_order.status);
        let broker_order_id = &cancellation.cancelled_order.broker_order_id;
        let client_order_id = cancellation.open_order.client_order_id.trim();
        let updated = if client_order_id.is_empty() {
            0
        } else {
            db.update_order_status_by_client_order_id(
                run_id,
                client_order_id,
                broker_order_id,
                status,
                updated_at_ms,
            )
            .await?
        };
        local_synced += if updated > 0 {
            updated
        } else {
            db.update_order_status_by_broker_id(run_id, broker_order_id, status, updated_at_ms)
                .await?
        };
    }
    Ok(local_synced)
}

fn broker_order_status_label(status: broker::BrokerOrderStatus) -> &'static str {
    match status {
        broker::BrokerOrderStatus::Accepted => "ACCEPTED",
        broker::BrokerOrderStatus::Cancelled => "CANCELLED",
    }
}

fn operational_broker(app_config: &config::AppConfig) -> Result<Box<dyn Broker>> {
    match app_config.broker.kind {
        config::BrokerKind::Simulated => Ok(Box::new(FakeBrokerAdapter::new(
            broker::BrokerKind::Simulated,
        ))),
        config::BrokerKind::Futu => Ok(Box::new(FakeBrokerAdapter::futu())),
        config::BrokerKind::Okx => Ok(Box::new(FakeBrokerAdapter::okx())),
        config::BrokerKind::Binance => Ok(Box::new(BinanceSpotTestnetAdapter::try_new(
            binance_testnet_settings(app_config)?,
        )?)),
        config::BrokerKind::InteractiveBrokers => Ok(Box::new(IbkrPaperGatewayAdapter::try_new(
            ibkr_paper_gateway_settings(app_config)?,
        )?)),
    }
}

fn binance_order_side(input: &str) -> Result<BinanceOrderSide> {
    match input.to_ascii_lowercase().as_str() {
        "buy" => Ok(BinanceOrderSide::Buy),
        "sell" => Ok(BinanceOrderSide::Sell),
        other => bail!("unsupported Binance order side {other}; expected buy or sell"),
    }
}

fn ibkr_order_side(input: &str) -> Result<IbkrOrderSide> {
    match input.to_ascii_lowercase().as_str() {
        "buy" => Ok(IbkrOrderSide::Buy),
        "sell" => Ok(IbkrOrderSide::Sell),
        other => bail!("unsupported IBKR order side {other}; expected buy or sell"),
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
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
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
    balance: storage::AccountBalanceCommand,
    position: Option<storage::PositionCommand>,
    snapshot: storage::PortfolioSnapshotCommand,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IbkrPaperRecoverSummary {
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
            db.record_external_fill(storage::ExternalFillCommand {
                id: format!("{run_id}-binance-trade-{}", trade.trade_id),
                order_id: order.id.clone(),
                run_id: run_id.clone(),
                symbol: trade.symbol.clone(),
                side: order.side.clone(),
                price: trade.price,
                qty: trade.qty,
                fee: trade.fee,
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
            let all_fills = local_fills_from_storage(db.list_fills(run_id).await?);
            let accounting = binance_accounting_records_from_fills(
                run_id,
                &app_config.paper.account_id,
                &app_config.portfolio.base_currency,
                account.cash,
                &all_fills,
                ended_at_ms,
            )?;
            db.record_account_balance(accounting.balance).await?;
            if let Some(position) = accounting.position {
                db.record_position(position).await?;
            }
            db.record_portfolio_snapshot(accounting.snapshot).await?;
        }
        summary.recovered += 1;
        summary.trades += trades.len();
    }

    summary.remaining = db.list_recoverable_orders(run_id).await?.len();
    if summary.scanned > 0
        && summary.missing == 0
        && summary.remaining == 0
        && let Some(run) = db.get_strategy_run(run_id).await?
        && run.status != "completed"
    {
        db.update_strategy_run_status(
            run_id,
            "recovered",
            Some(chrono::Utc::now().timestamp_millis()),
            None,
        )
        .await?;
        summary.run_status_updated = true;
    }
    Ok(summary)
}

async fn recover_ibkr_paper_orders(
    app_config: &config::AppConfig,
    db: &storage::Db,
    adapter: &IbkrPaperGatewayAdapter,
    request_id: i64,
) -> Result<IbkrPaperRecoverSummary> {
    let run_id = &app_config.runtime.run_id;
    let orders = db.list_recoverable_orders(run_id).await?;
    let mut summary = IbkrPaperRecoverSummary {
        scanned: orders.len(),
        recovered: 0,
        missing: 0,
        remaining: 0,
        trades: 0,
        run_status_updated: false,
    };
    if orders.is_empty() {
        return Ok(summary);
    }

    let open_orders = adapter.open_orders().await?;
    for order in orders {
        let local_order = LocalOrder::from(order.clone());
        let symbol = paper::ibkr_stock_symbol(&order.symbol)?;
        let open_order = open_orders
            .iter()
            .find(|remote| ibkr_local_order_matches_single_remote_open(&local_order, remote));
        let broker_order_id = order
            .broker_order_id
            .clone()
            .or_else(|| open_order.map(|remote| remote.order_id.to_string()));
        let executions = adapter
            .executions(request_id, &app_config.paper.account_id, &symbol)
            .await?;
        let matched_executions = broker_order_id
            .as_deref()
            .map(|broker_order_id| {
                executions
                    .iter()
                    .filter(|execution| execution.order_id.to_string() == broker_order_id)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if open_order.is_none() && matched_executions.is_empty() {
            summary.missing += 1;
            continue;
        }

        let filled_qty = matched_executions
            .iter()
            .fold(Decimal::ZERO, |total, execution| total + execution.qty);
        let status = ibkr_recovered_order_status(&local_order, open_order, filled_qty)?;
        let ended_at_ms = chrono::Utc::now().timestamp_millis();
        for execution in &matched_executions {
            db.record_external_fill(storage::ExternalFillCommand {
                id: format!("{run_id}-ibkr-trade-{}", execution.trade_id),
                order_id: order.id.clone(),
                run_id: run_id.clone(),
                symbol: execution.symbol.clone(),
                side: execution.side.clone(),
                price: execution.price,
                qty: execution.qty,
                fee: execution.fee,
                ts_ms: ended_at_ms,
            })
            .await?;
        }
        if let Some(broker_order_id) = broker_order_id {
            db.update_order_execution_by_client_order_id(
                &order.client_order_id,
                &broker_order_id,
                &status,
                &filled_qty.to_string(),
                ended_at_ms,
            )
            .await?;
        }
        summary.recovered += 1;
        summary.trades += matched_executions.len();
    }

    summary.remaining = db.list_recoverable_orders(run_id).await?.len();
    if summary.scanned > 0
        && summary.missing == 0
        && summary.remaining == 0
        && let Some(run) = db.get_strategy_run(run_id).await?
        && run.status != "completed"
    {
        db.update_strategy_run_status(
            run_id,
            "recovered",
            Some(chrono::Utc::now().timestamp_millis()),
            None,
        )
        .await?;
        summary.run_status_updated = true;
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
    let local_orders = local_orders_from_storage(db.list_orders(run_id).await?);
    let local_fills = local_fills_from_storage(db.list_fills(run_id).await?);
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
    local: &LocalOrder,
    remote_open_orders: &[BinanceOpenOrder],
) -> bool {
    remote_open_orders.iter().any(|remote| {
        local.broker_order_id.as_deref() == Some(&remote.order_id.to_string())
            || local.client_order_id == remote.client_order_id
    })
}

async fn reconcile_ibkr_paper(
    app_config: &config::AppConfig,
    db: &storage::Db,
    adapter: &IbkrPaperGatewayAdapter,
    request_id: i64,
    symbol: &str,
) -> Result<IbkrPaperReconciliation> {
    let run_id = &app_config.runtime.run_id;
    let account_id = &app_config.paper.account_id;
    let local_orders = local_orders_from_storage(db.list_orders(run_id).await?)
        .into_iter()
        .filter(|order| ibkr_local_order_in_scope(order, account_id, symbol))
        .collect::<Vec<_>>();
    let local_order_ids = local_orders
        .iter()
        .map(|order| order.id.as_str())
        .collect::<BTreeSet<_>>();
    let local_fills = local_fills_from_storage(db.list_fills(run_id).await?)
        .into_iter()
        .filter(|fill| {
            local_order_ids.contains(fill.order_id.as_str())
                && ibkr_symbol_matches(&fill.symbol, symbol)
        })
        .collect::<Vec<_>>();
    let remote_open_orders = adapter
        .open_orders()
        .await?
        .into_iter()
        .filter(|order| {
            order.account_id == *account_id && ibkr_symbol_matches(&order.symbol, symbol)
        })
        .collect::<Vec<_>>();
    let remote_executions = adapter.executions(request_id, account_id, symbol).await?;
    let local_open_orders = local_orders
        .iter()
        .filter(|order| ibkr_local_order_expects_remote_open(order))
        .collect::<Vec<_>>();

    let matched_orders = local_open_orders
        .iter()
        .filter(|order| ibkr_local_order_matches_remote_open(order, &remote_open_orders))
        .count();
    let remote_open_matched = remote_open_orders
        .iter()
        .filter(|remote| {
            local_open_orders.iter().any(|local| {
                local.broker_order_id.as_deref() == Some(&remote.order_id.to_string())
                    || (!remote.client_order_id.is_empty()
                        && local.client_order_id == remote.client_order_id)
            })
        })
        .count();
    let execution_summary =
        ibkr_execution_match_summary(&remote_executions, &local_orders, &local_fills)?;
    let (local_fully_filled_orders, local_partially_filled_orders) =
        ibkr_local_fill_state_counts(&local_orders)?;
    let local_fill_qty = local_fills.iter().try_fold(Decimal::ZERO, |total, fill| {
        Decimal::from_str(&fill.qty).map(|qty| total + qty)
    })?;
    let remote_execution_qty = execution_summary.matched_qty;

    Ok(IbkrPaperReconciliation {
        symbol: symbol.to_string(),
        local_orders: local_orders.len(),
        local_fills: local_fills.len(),
        matched_orders,
        local_only_orders: local_open_orders.len() - matched_orders,
        remote_open_orders: remote_open_orders.len(),
        remote_open_matched,
        remote_open_unmatched: remote_open_orders.len() - remote_open_matched,
        remote_executions: remote_executions.len(),
        remote_execution_matched: execution_summary.matched,
        remote_execution_matched_orders: execution_summary.matched_orders,
        remote_execution_max_per_order: execution_summary.max_per_order,
        remote_execution_unmatched: remote_executions.len() - execution_summary.matched,
        remote_execution_field_drifts: execution_summary.field_drifts,
        remote_execution_order_ids: execution_summary.order_ids,
        remote_execution_client_order_ids: execution_summary.client_order_ids,
        remote_execution_trade_ids: execution_summary.trade_ids,
        local_fully_filled_orders,
        local_partially_filled_orders,
        local_fill_qty,
        remote_execution_qty,
        qty_delta: remote_execution_qty - local_fill_qty,
    })
}

fn ibkr_symbol_matches(local_symbol: &str, symbol: &str) -> bool {
    matches!(
        paper::ibkr_stock_symbol(local_symbol).as_deref(),
        Ok(local_symbol) if local_symbol == symbol
    )
}

fn ibkr_local_order_in_scope(local: &LocalOrder, account_id: &str, symbol: &str) -> bool {
    local.account_id == account_id && ibkr_symbol_matches(&local.symbol, symbol)
}

fn ibkr_local_fill_state_counts(local_orders: &[LocalOrder]) -> Result<(usize, usize)> {
    let mut fully_filled = 0;
    let mut partially_filled = 0;
    for order in local_orders {
        let qty = Decimal::from_str(&order.qty)?;
        let filled_qty = Decimal::from_str(&order.filled_qty)?;
        if qty > Decimal::ZERO && filled_qty >= qty {
            fully_filled += 1;
        } else if filled_qty > Decimal::ZERO {
            partially_filled += 1;
        }
    }
    Ok((fully_filled, partially_filled))
}

fn ibkr_local_order_expects_remote_open(local: &LocalOrder) -> bool {
    let is_fully_filled = Decimal::from_str(&local.filled_qty)
        .ok()
        .zip(Decimal::from_str(&local.qty).ok())
        .is_some_and(|(filled_qty, qty)| filled_qty >= qty);
    if is_fully_filled {
        return false;
    }

    matches!(
        local
            .status
            .replace(['_', '-', ' '], "")
            .to_uppercase()
            .as_str(),
        "SUBMITTED" | "NEW" | "PARTIALLYFILLED" | "PENDINGSUBMIT" | "PRESUBMITTED" | "APIPENDING"
    )
}

fn ibkr_local_order_matches_remote_open(
    local: &LocalOrder,
    remote_open_orders: &[IbkrOpenOrder],
) -> bool {
    remote_open_orders
        .iter()
        .any(|remote| ibkr_local_order_matches_single_remote_open(local, remote))
}

fn ibkr_local_order_matches_single_remote_open(local: &LocalOrder, remote: &IbkrOpenOrder) -> bool {
    local.broker_order_id.as_deref() == Some(&remote.order_id.to_string())
        || (!remote.client_order_id.is_empty() && local.client_order_id == remote.client_order_id)
}

fn ibkr_recovered_order_status(
    local: &LocalOrder,
    open_order: Option<&IbkrOpenOrder>,
    filled_qty: Decimal,
) -> Result<String> {
    if let Some(open_order) = open_order {
        return Ok(open_order.status.clone());
    }
    let order_qty = Decimal::from_str(&local.qty)?;
    if filled_qty >= order_qty {
        Ok("FILLED".to_string())
    } else if filled_qty > Decimal::ZERO {
        Ok("PARTIALLY_FILLED".to_string())
    } else {
        Ok(local.status.clone())
    }
}

fn ibkr_execution_match_summary(
    executions: &[broker::IbkrExecution],
    local_orders: &[LocalOrder],
    local_fills: &[LocalFill],
) -> Result<IbkrExecutionMatchSummary> {
    let mut executions_by_order = BTreeMap::<i64, Vec<&broker::IbkrExecution>>::new();
    for execution in executions {
        executions_by_order
            .entry(execution.order_id)
            .or_default()
            .push(execution);
    }

    let mut matched = 0;
    let mut matched_orders = 0;
    let mut max_per_order = 0;
    let mut field_drifts = 0;
    let mut matched_qty = Decimal::ZERO;
    let mut order_ids = BTreeSet::new();
    let mut client_order_ids = BTreeSet::new();
    let mut trade_ids = BTreeSet::new();
    for (broker_order_id, remote) in executions_by_order {
        let broker_order_id = broker_order_id.to_string();
        let local_order_id = local_orders
            .iter()
            .find(|order| {
                order.broker_order_id.as_deref() == Some(broker_order_id.as_str())
                    || remote.iter().any(|execution| {
                        !execution.client_order_id.is_empty()
                            && execution.client_order_id == order.client_order_id
                    })
            })
            .map(|order| order.id.as_str())
            .or_else(|| {
                local_fills
                    .iter()
                    .find(|fill| {
                        remote
                            .iter()
                            .any(|execution| fill.id.ends_with(&execution.trade_id))
                    })
                    .map(|fill| fill.order_id.as_str())
            });
        let Some(local_order_id) = local_order_id else {
            continue;
        };
        let local = local_fills
            .iter()
            .filter(|fill| fill.order_id == local_order_id)
            .collect::<Vec<_>>();
        if local.is_empty() {
            continue;
        }

        matched += remote.len();
        matched_orders += 1;
        max_per_order = max_per_order.max(remote.len());
        order_ids.insert(broker_order_id);
        for execution in &remote {
            if !execution.client_order_id.is_empty() {
                client_order_ids.insert(execution.client_order_id.clone());
            }
            if !execution.trade_id.is_empty() {
                trade_ids.insert(execution.trade_id.clone());
            }
        }
        matched_qty += remote
            .iter()
            .fold(Decimal::ZERO, |total, execution| total + execution.qty);
        if ibkr_execution_group_has_field_drift(&remote, &local)? {
            field_drifts += 1;
        }
    }

    Ok(IbkrExecutionMatchSummary {
        matched,
        matched_orders,
        max_per_order,
        field_drifts,
        matched_qty,
        order_ids: order_ids.into_iter().collect(),
        client_order_ids: client_order_ids.into_iter().collect(),
        trade_ids: trade_ids.into_iter().collect(),
    })
}

fn joined_output_values(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}

fn ibkr_execution_group_has_field_drift(
    remote: &[&broker::IbkrExecution],
    local: &[&LocalFill],
) -> Result<bool> {
    let remote_qty = remote
        .iter()
        .fold(Decimal::ZERO, |total, execution| total + execution.qty);
    let remote_notional = remote.iter().fold(Decimal::ZERO, |total, execution| {
        total + execution.price * execution.qty
    });
    let remote_fee = remote
        .iter()
        .fold(Decimal::ZERO, |total, execution| total + execution.fee);
    let local_qty = local.iter().try_fold(Decimal::ZERO, |total, fill| {
        Decimal::from_str(&fill.qty).map(|qty| total + qty)
    })?;
    let local_notional = local.iter().try_fold(Decimal::ZERO, |total, fill| {
        let price = Decimal::from_str(&fill.price)?;
        let qty = Decimal::from_str(&fill.qty)?;
        Ok::<Decimal, rust_decimal::Error>(total + price * qty)
    })?;
    let local_fee = local.iter().try_fold(Decimal::ZERO, |total, fill| {
        Decimal::from_str(&fill.fee).map(|fee| total + fee)
    })?;
    let remote_price = weighted_execution_price(remote_notional, remote_qty);
    let local_price = weighted_execution_price(local_notional, local_qty);
    let remote_symbol = remote.first().map(|execution| execution.symbol.as_str());
    let remote_side = remote
        .first()
        .and_then(|execution| normalized_ibkr_side(&execution.side));
    let local_symbol = local
        .first()
        .map(|fill| paper::ibkr_stock_symbol(&fill.symbol))
        .transpose()?;
    let local_side = local
        .first()
        .and_then(|fill| normalized_ibkr_side(&fill.side));
    let remote_fields_consistent = remote_side.is_some()
        && remote.iter().all(|execution| {
            Some(execution.symbol.as_str()) == remote_symbol
                && normalized_ibkr_side(&execution.side) == remote_side
        });
    let local_fields_consistent = local_side.is_some()
        && local.iter().all(|fill| {
            paper::ibkr_stock_symbol(&fill.symbol).ok().as_deref() == local_symbol.as_deref()
                && normalized_ibkr_side(&fill.side) == local_side
        });

    Ok(!remote_fields_consistent
        || !local_fields_consistent
        || remote_symbol != local_symbol.as_deref()
        || remote_side != local_side
        || remote_price != local_price
        || remote_qty != local_qty
        || remote_fee != local_fee)
}

fn weighted_execution_price(notional: Decimal, qty: Decimal) -> Decimal {
    if qty == Decimal::ZERO {
        Decimal::ZERO
    } else {
        notional / qty
    }
}

fn normalized_ibkr_side(side: &str) -> Option<&'static str> {
    match side.to_ascii_uppercase().as_str() {
        "BUY" | "BOT" => Some("BUY"),
        "SELL" | "SLD" => Some("SELL"),
        _ => None,
    }
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
    fills: &[LocalFill],
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
        balance: storage::AccountBalanceCommand {
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            asset: base_currency.to_string(),
            total: cash,
            available: cash,
            frozen: Decimal::ZERO,
            updated_at_ms,
        },
        position: (!fills.is_empty()).then(|| storage::PositionCommand {
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            symbol: symbol.clone(),
            qty: signed_qty,
            avg_price,
            updated_at_ms,
        }),
        snapshot: storage::PortfolioSnapshotCommand {
            id: format!("{run_id}-binance-snapshot-{updated_at_ms}"),
            run_id: run_id.to_string(),
            account_id: account_id.to_string(),
            ts_ms: updated_at_ms,
            cash,
            market_value,
            equity: cash + market_value,
            realized_pnl: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
        },
    })
}

async fn insert_event(
    db: &storage::Db,
    source: &str,
    category: &str,
    payload_json: &str,
) -> Result<()> {
    let payload = serde_json::from_str(payload_json)
        .unwrap_or_else(|_| serde_json::Value::String(payload_json.to_string()));
    db.record_runtime_event(storage::RuntimeEventCommand {
        ts_ms: chrono::Utc::now().timestamp_millis(),
        source: source.to_string(),
        category: category.to_string(),
        payload,
    })
    .await?;
    Ok(())
}

async fn log_and_print_ingestion(
    db: &storage::Db,
    result: data::ingestion::IngestionResult,
    started: Instant,
) -> Result<()> {
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    data::ingestion::tracker::IngestionTracker::log_ingestion(db, &result, duration_ms).await?;
    println!(
        "ingestion_completed: source={} table={} rows_fetched={} rows_upserted={} duration_ms={}",
        result.source, result.table, result.rows_fetched, result.rows_upserted, duration_ms
    );
    Ok(())
}

fn ensure_supported_ingestion_exchange(exchange: &str) -> Result<()> {
    if exchange.eq_ignore_ascii_case("binance") {
        Ok(())
    } else {
        bail!("unsupported ingestion exchange {exchange}; expected binance")
    }
}

fn compact_json_file(path: &str) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config content from {path}"))?;
    let value = serde_json::from_str::<serde_json::Value>(&content)
        .with_context(|| format!("failed to parse config JSON from {path}"))?;
    Ok(serde_json::to_string(&value)?)
}

async fn transition_config_state(
    config_path: &str,
    name: &str,
    version: u32,
    new_state: storage::ConfigState,
    changed_by: &str,
    actor_role: Option<&str>,
    reason: Option<&str>,
    ts_ms: Option<i64>,
) -> Result<()> {
    let (_, db) = load_db(config_path).await?;
    let ts_ms = ts_ms.unwrap_or_else(now_ms);
    if let Some(actor_role) = actor_role {
        db.update_config_state_with_policy(
            name, version, new_state, changed_by, actor_role, reason, ts_ms,
        )
        .await?;
    } else {
        db.update_config_state(name, version, new_state, changed_by, reason, ts_ms)
            .await?;
    }
    let config_version = db
        .get_config(name, version)
        .await?
        .ok_or_else(|| anyhow::anyhow!("config version {name}:{version} was not found"))?;
    print_config_version(&config_version);
    Ok(())
}

fn print_config_version(config: &storage::ConfigVersion) {
    println!(
        "config_version: name={} version={} state={} parent_version={} created_by={} created_at_ms={} state_changed_by={} state_changed_at_ms={} reason={} id={}",
        config.name,
        config.version,
        config.state.as_str(),
        optional_u32(config.parent_version),
        config.created_by,
        config.created_at_ms,
        config.state_changed_by,
        config.state_changed_at_ms,
        config.state_change_reason.as_deref().unwrap_or(""),
        config.id
    );
    if config.target_env.is_some()
        || config.rollout.is_some()
        || config.approved_by.is_some()
        || config.published_by.is_some()
    {
        println!(
            "config_governance: name={} version={} target_env={} rollout={} approved_by={} published_by={}",
            config.name,
            config.version,
            config.target_env.as_deref().unwrap_or(""),
            config.rollout.as_deref().unwrap_or(""),
            config.approved_by.as_deref().unwrap_or(""),
            config.published_by.as_deref().unwrap_or("")
        );
    }
}

fn print_config_diff(diff: &storage::ConfigDiff) {
    println!(
        "config_diff: name={} v1={} v2={} added={} removed={} changed={}",
        diff.name,
        diff.version_a,
        diff.version_b,
        diff.added.len(),
        diff.removed.len(),
        diff.changed.len()
    );
    for path in &diff.added {
        println!("config_diff_added: path={path}");
    }
    for path in &diff.removed {
        println!("config_diff_removed: path={path}");
    }
    for entry in &diff.changed {
        println!(
            "config_diff_changed: path={} before={} after={}",
            entry.path,
            json_literal(&entry.before),
            json_literal(&entry.after)
        );
    }
}

fn optional_u32(value: Option<u32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn json_literal(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn build_system_log_filter(
    run_id: Option<String>,
    level: Option<String>,
    target: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> storage::SystemLogFilter {
    storage::SystemLogFilter {
        run_id,
        level,
        target,
        from_ms,
        to_ms,
        search,
        limit,
        offset,
    }
}

fn print_system_log(log: &storage::StoredSystemLog) {
    println!(
        "system_log: run_id={} ts_ms={} level={} target={} message={} fields={} created_at_ms={}",
        log.run_id.as_deref().unwrap_or(""),
        log.ts_ms,
        log.level,
        log.target,
        log.message,
        log.fields_json.as_deref().unwrap_or("{}"),
        log.created_at_ms
    );
}

fn system_log_json(log: &storage::StoredSystemLog) -> serde_json::Value {
    let fields = log
        .fields_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "run_id": log.run_id,
        "ts_ms": log.ts_ms,
        "level": log.level,
        "target": log.target,
        "message": log.message,
        "fields": fields,
        "created_at_ms": log.created_at_ms,
    })
}

async fn ship_system_logs_with_retry(
    collector_url: &str,
    bearer_token: Option<&str>,
    signature_secret: Option<&str>,
    body: String,
    max_retries: u32,
    retry_backoff_ms: u64,
) -> Result<(reqwest::StatusCode, u32)> {
    let client = reqwest::Client::new();
    let max_attempts = max_retries.saturating_add(1);
    let mut last_error = None;
    let signature = signature_secret.map(|secret| {
        let timestamp_ms = chrono::Utc::now().timestamp_millis().to_string();
        let signature = log_ship_signature(secret, &timestamp_ms, &body);
        (timestamp_ms, signature)
    });

    for attempt in 1..=max_attempts {
        let mut request = client
            .post(collector_url)
            .header(reqwest::header::CONTENT_TYPE, "application/x-ndjson")
            .body(body.clone());
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some((timestamp_ms, signature)) = &signature {
            request = request
                .header("X-Trader-Log-Timestamp", timestamp_ms)
                .header("X-Trader-Log-Signature", format!("v1={signature}"));
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return Ok((status, attempt));
                }
                if !is_retryable_ship_status(status) || attempt == max_attempts {
                    bail!(
                        "system log collector returned non-success status {} after {} attempt(s)",
                        status.as_u16(),
                        attempt
                    );
                }
                last_error = Some(format!("collector status {}", status.as_u16()));
            }
            Err(error) => {
                if attempt == max_attempts {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to ship system logs to {collector_url} after {attempt} attempt(s)"
                        )
                    });
                }
                last_error = Some(error.to_string());
            }
        }

        let backoff = retry_backoff_ms.saturating_mul(attempt as u64);
        if backoff > 0 {
            tokio::time::sleep(Duration::from_millis(backoff)).await;
        }
    }

    bail!(
        "failed to ship system logs to {} after {} attempt(s): {}",
        collector_url,
        max_attempts,
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )
}

fn is_retryable_ship_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn log_ship_signature(secret: &str, timestamp_ms: &str, body: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(timestamp_ms.as_bytes());
    mac.update(b".");
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

async fn run_reconciliation_gate(
    config: &str,
    accounts: Vec<String>,
    min_successful_audits: Option<usize>,
    max_audit_age_ms: Option<i64>,
) -> Result<()> {
    let (app_config, db) = load_db(config).await?;
    let decision = runtime::evaluate_reconciliation_gate_from_storage(
        &app_config,
        &db,
        &accounts,
        min_successful_audits,
        max_audit_age_ms,
    )
    .await?;
    if let Err(error) = runtime::record_reconciliation_gate_decision(
        &db,
        &app_config,
        &decision,
        runtime::ReconciliationGateAuditLogContext {
            run_id: Some(app_config.runtime.run_id.clone()),
            source: "cli.reconciliation_gate".to_string(),
            config_path: Some(config.to_string()),
            config_format: Some("TOML".to_string()),
            config_checksum: Some(stable_file_content_hash(config)?),
            config_id: None,
            config_version: None,
        },
        &live_worker_alert_sink_settings(&app_config.live.alerts),
    )
    .await
    {
        if runtime::should_fail_on_reconciliation_gate_log_write_failure(&app_config) {
            return Err(error);
        }
        eprintln!("reconciliation gate audit log write failed: {error}");
    }

    match decision.status {
        broker::ReconciliationGateStatus::Allow => {
            println!("reconciliation gate ok");
            Ok(())
        }
        broker::ReconciliationGateStatus::Block => {
            print_reconciliation_gate_failures(&decision.failures);
            if runtime::should_enforce_reconciliation_gate_block(&app_config, &decision) {
                bail!("reconciliation gate blocked")
            }
            println!("reconciliation gate warn-only policy allowed");
            Ok(())
        }
    }
}

async fn enforce_live_reconciliation_gate(
    app_config: &config::AppConfig,
    db: &storage::Db,
    context: runtime::ReconciliationGateAuditLogContext,
) -> Result<()> {
    if let Some(decision) =
        runtime::evaluate_live_reconciliation_gate_from_storage(app_config, db).await?
    {
        if let Err(error) = runtime::record_reconciliation_gate_decision(
            db,
            app_config,
            &decision,
            context,
            &live_worker_alert_sink_settings(&app_config.live.alerts),
        )
        .await
        {
            if runtime::should_fail_on_reconciliation_gate_log_write_failure(app_config) {
                return Err(error);
            }
            eprintln!("reconciliation gate audit log write failed: {error}");
        }
        match decision.status {
            broker::ReconciliationGateStatus::Allow => {}
            broker::ReconciliationGateStatus::Block => {
                print_reconciliation_gate_failures(&decision.failures);
                if runtime::should_enforce_reconciliation_gate_block(app_config, &decision) {
                    bail!("reconciliation gate blocked")
                }
                eprintln!("reconciliation gate warn-only policy allowed");
            }
        }
    }
    Ok(())
}

fn print_reconciliation_gate_failures(failures: &[broker::ReconciliationGateFailure]) {
    for failure in failures {
        eprintln!("{}", runtime::format_reconciliation_gate_failure(failure));
    }
}

async fn load_db(config_path: &str) -> Result<(config::AppConfig, storage::Db)> {
    let app_config = config::AppConfig::from_toml_file(config_path)?;
    ensure_database_parent(&app_config.database.url)?;
    let db = storage::Db::connect(&app_config.database.url).await?;
    Ok((app_config, db))
}

async fn run_live_worker(launch_file: &str) -> Result<()> {
    let launch_bytes = tokio::fs::read(launch_file)
        .await
        .with_context(|| format!("failed to read live worker launch file {launch_file}"))?;
    let launch: runtime::LiveWorkerLaunchSpec = serde_json::from_slice(&launch_bytes)
        .with_context(|| format!("failed to parse live worker launch file {launch_file}"))?;
    launch.validate_no_embedded_secrets()?;
    ensure_database_parent(&launch.db_url)?;

    let app_config = app_config_from_launch(&launch)?;
    let db = storage::Db::connect(&launch.db_url).await?;
    db.migrate().await?;
    enforce_live_reconciliation_gate(
        &app_config,
        &db,
        runtime::ReconciliationGateAuditLogContext {
            run_id: Some(launch.run_id.clone()),
            source: "cli.live_worker".to_string(),
            config_path: launch.config_path.clone(),
            config_format: Some(launch.config_format.clone()),
            config_checksum: Some(stable_bytes_hash(launch.config_content.as_bytes())),
            config_id: None,
            config_version: None,
        },
    )
    .await?;
    let initial_cash = Decimal::from_str(&app_config.portfolio.initial_cash)?;
    let settings = runtime::LiveRuntimeSettings {
        run_id: launch.run_id.clone(),
        broker_kind: live_worker_broker_kind(app_config.broker.kind),
        account_id: app_config.paper.account_id.clone(),
        base_currency: app_config.portfolio.base_currency.clone(),
        initial_cash,
        broker_snapshot_interval_ms: launch.broker_snapshot_interval_ms,
        alert_sink: live_worker_alert_sink_settings(&app_config.live.alerts),
        logging: log_writer_settings(&app_config),
    };
    let broker = live_worker_broker_for_config(&app_config)?;
    let cancel = runtime::CancellationFlag::default();

    write_worker_event(runtime::LiveWorkerEvent::WorkerStarted {
        run_id: launch.run_id.clone(),
        pid: std::process::id(),
    })?;

    let live_runtime = runtime::LiveRuntime::new_with_broker(db, settings, broker)
        .with_startup_recovery_unmatched_open_orders_policy(
            launch.startup_recovery_unmatched_open_orders_policy,
        );
    let runtime_cancel = cancel.clone();
    let mut runtime_join = tokio::spawn(async move { live_runtime.run(runtime_cancel).await });

    write_worker_event(runtime::LiveWorkerEvent::RuntimeStarted {
        run_id: launch.run_id.clone(),
    })?;

    let mut stdin_lines =
        tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(tokio::io::stdin()));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
    let mut stdin_closed = false;
    let mut shutdown_requested = false;

    loop {
        tokio::select! {
            maybe_line = stdin_lines.next_line(), if !stdin_closed => {
                match maybe_line? {
                    Some(line) => {
                        let command = runtime::parse_worker_command_line(&line)?;
                        match command {
                            runtime::LiveWorkerCommand::HealthCheck { request_id } => {
                                write_worker_event(runtime::LiveWorkerEvent::Health {
                                    run_id: launch.run_id.clone(),
                                    request_id,
                                    status: if shutdown_requested {
                                        "stopping".to_string()
                                    } else {
                                        "running".to_string()
                                    },
                                })?;
                            }
                            runtime::LiveWorkerCommand::Shutdown { reason, .. } => {
                                if !shutdown_requested {
                                    shutdown_requested = true;
                                    write_worker_event(runtime::LiveWorkerEvent::RuntimeStopping {
                                        run_id: launch.run_id.clone(),
                                        reason,
                                    })?;
                                    cancel.cancel();
                                }
                            }
                        }
                    }
                    None => {
                        stdin_closed = true;
                    }
                }
            }
            _ = heartbeat.tick() => {
                write_worker_event(runtime::LiveWorkerEvent::Heartbeat {
                    run_id: launch.run_id.clone(),
                    status: if shutdown_requested {
                        "stopping".to_string()
                    } else {
                        "running".to_string()
                    },
                    ts_ms: chrono::Utc::now().timestamp_millis(),
                })?;
            }
            runtime_result = &mut runtime_join => {
                match runtime_result? {
                    Ok(()) => {
                        write_worker_event(runtime::LiveWorkerEvent::RuntimeStopped {
                            run_id: launch.run_id.clone(),
                            status: "stopped".to_string(),
                        })?;
                        return Ok(());
                    }
                    Err(error) => {
                        write_worker_event(runtime::LiveWorkerEvent::RuntimeFailed {
                            run_id: launch.run_id.clone(),
                            error: error.to_string(),
                        })?;
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn app_config_from_launch(launch: &runtime::LiveWorkerLaunchSpec) -> Result<config::AppConfig> {
    match launch.config_format.as_str() {
        "TOML" => Ok(config::AppConfig::from_toml_str(&launch.config_content)?),
        "JSON" => Ok(serde_json::from_str(&launch.config_content)?),
        other => bail!("unsupported launch config_format {other}"),
    }
}

fn write_worker_event(event: runtime::LiveWorkerEvent) -> Result<()> {
    println!("{}", runtime::worker_event_line(&event)?);
    std::io::stdout().flush()?;
    Ok(())
}

fn live_worker_broker_for_config(app_config: &config::AppConfig) -> Result<Arc<dyn Broker>> {
    match app_config.broker.kind {
        config::BrokerKind::InteractiveBrokers => {
            if app_config.broker.mode != config::BrokerMode::Paper {
                bail!(
                    "live runtime IBKR Gateway adapter requires broker.mode = paper in this phase"
                );
            }
            let adapter =
                IbkrPaperGatewayAdapter::try_new(ibkr_paper_gateway_settings(app_config)?)?;
            Ok(Arc::new(adapter))
        }
        config::BrokerKind::Binance => {
            if app_config.broker.mode != config::BrokerMode::Paper {
                bail!("live runtime Binance adapter requires broker.mode = paper in this phase");
            }
            let adapter =
                BinanceSpotTestnetAdapter::try_new(binance_testnet_settings(app_config)?)?;
            Ok(Arc::new(adapter))
        }
        kind => Ok(Arc::new(
            FakeBrokerAdapter::new(live_worker_broker_kind(kind))
                .with_startup_unmatched_open_order(
                    app_config.broker.fake_startup_unmatched_open_order,
                ),
        )),
    }
}

fn live_worker_broker_kind(kind: config::BrokerKind) -> broker::BrokerKind {
    match kind {
        config::BrokerKind::Simulated => broker::BrokerKind::Simulated,
        config::BrokerKind::Futu => broker::BrokerKind::Futu,
        config::BrokerKind::Binance => broker::BrokerKind::Binance,
        config::BrokerKind::Okx => broker::BrokerKind::Okx,
        config::BrokerKind::InteractiveBrokers => broker::BrokerKind::InteractiveBrokers,
    }
}

fn live_worker_alert_sink_settings(
    alerts: &config::LiveAlertsConfig,
) -> runtime::AlertSinkSettings {
    if !alerts.enabled {
        return runtime::AlertSinkSettings::Noop;
    }
    if !alerts.sinks.is_empty() {
        let sinks = alerts
            .sinks
            .iter()
            .filter_map(|sink| live_worker_alert_sink_from_config(sink, alerts))
            .collect::<Vec<_>>();
        return match sinks.len() {
            0 => runtime::AlertSinkSettings::Noop,
            1 => sinks
                .into_iter()
                .next()
                .unwrap_or(runtime::AlertSinkSettings::Noop),
            _ => runtime::AlertSinkSettings::Multi(sinks),
        };
    }
    live_worker_alert_sink_from_legacy_config(alerts)
}

fn live_worker_alert_sink_from_legacy_config(
    alerts: &config::LiveAlertsConfig,
) -> runtime::AlertSinkSettings {
    let sink = config::LiveAlertSinkConfig {
        sink: alerts.sink.clone().unwrap_or_default(),
        file_path: alerts.file_path.clone(),
        webhook_url: alerts.webhook_url.clone(),
        cooldown_ms: alerts.cooldown_ms,
        webhook_timeout_ms: alerts.webhook_timeout_ms,
        webhook_max_retries: alerts.webhook_max_retries,
        webhook_auth_token: alerts.webhook_auth_token.clone(),
    };
    live_worker_alert_sink_from_config(&sink, alerts).unwrap_or(runtime::AlertSinkSettings::Noop)
}

fn live_worker_alert_sink_from_config(
    sink: &config::LiveAlertSinkConfig,
    defaults: &config::LiveAlertsConfig,
) -> Option<runtime::AlertSinkSettings> {
    match (
        sink.sink.as_str(),
        sink.file_path.as_ref().filter(|path| !path.is_empty()),
        sink.webhook_url.as_ref().filter(|url| !url.is_empty()),
    ) {
        ("file", Some(path), _) => Some(runtime::AlertSinkSettings::File {
            path: path.clone(),
            cooldown_ms: sink.cooldown_ms.or(defaults.cooldown_ms).unwrap_or(300_000),
        }),
        ("webhook", _, Some(url)) => Some(runtime::AlertSinkSettings::Webhook {
            url: url.clone(),
            cooldown_ms: sink.cooldown_ms.or(defaults.cooldown_ms).unwrap_or(300_000),
            timeout_ms: sink
                .webhook_timeout_ms
                .or(defaults.webhook_timeout_ms)
                .unwrap_or(3_000),
            max_retries: sink
                .webhook_max_retries
                .or(defaults.webhook_max_retries)
                .unwrap_or(2),
            auth_token: sink
                .webhook_auth_token
                .clone()
                .or_else(|| defaults.webhook_auth_token.clone()),
        }),
        _ => None,
    }
}

fn alert_dedup_key_for_cli(message: &str, run_id: &str, fields: &serde_json::Value) -> String {
    let account_id = fields
        .get("account_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let symbol = fields
        .get("symbol")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let reason = fields
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    format!("{message}|{run_id}|{account_id}|{symbol}|{reason}")
}

async fn persist_cli_run_config_snapshot(
    db: &storage::Db,
    run_spec: &runtime::RunSpec,
    config_path: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read config snapshot from {config_path}"))?;
    let snapshot = serde_json::json!({
        "run_spec": run_spec_snapshot_json(run_spec),
        "template": {
            "format": "TOML",
            "content": content,
        }
    });
    let snapshot_content = serde_json::to_string(&snapshot)?;
    let ts_ms = chrono::Utc::now().timestamp_millis();
    db.record_run_config_snapshot(storage::RunConfigSnapshotCommand {
        run_id: run_spec.run_id.clone(),
        content: snapshot_content.clone(),
        format: "JSON".to_string(),
        checksum: Some(stable_bytes_hash(snapshot_content.as_bytes())),
        ts_ms,
    })
    .await?;
    Ok(())
}

fn run_spec_snapshot_json(run_spec: &runtime::RunSpec) -> serde_json::Value {
    serde_json::json!({
        "run_id": run_spec.run_id,
        "mode": runtime_mode_slug(&run_spec.mode),
        "strategy": {
            "name": run_spec.strategy.name,
            "universe": run_spec.strategy.universe,
            "alpha": run_spec.strategy.alpha,
            "alpha_conflict_resolution": run_spec.strategy.alpha_conflict_resolution,
            "symbols": run_spec.strategy.symbols,
            "fast_window": run_spec.strategy.fast_window,
            "slow_window": run_spec.strategy.slow_window,
        },
        "data": {
            "source": run_spec.data.source,
            "path": run_spec.data.path,
            "inputs": run_spec.data.inputs.iter().map(|input| {
                serde_json::json!({
                    "symbol": input.symbol,
                    "source": input.source,
                    "path": input.path,
                })
            }).collect::<Vec<_>>(),
        },
        "portfolio": {
            "initial_cash": run_spec.portfolio.initial_cash,
            "base_currency": run_spec.portfolio.base_currency,
            "order_qty": run_spec.portfolio.order_qty,
            "max_abs_qty": run_spec.portfolio.max_abs_qty,
        },
        "risk": {
            "max_order_notional": run_spec.risk.max_order_notional,
            "min_cash_after_order": run_spec.risk.min_cash_after_order,
            "max_exposure": run_spec.risk.max_exposure,
            "max_drawdown": run_spec.risk.max_drawdown,
            "max_leverage": run_spec.risk.max_leverage,
            "max_margin_used": run_spec.risk.max_margin_used,
            "trading_halted": run_spec.risk.trading_halted,
            "allow_short": run_spec.risk.allow_short,
            "daily_loss_limit": run_spec.risk.daily_loss_limit,
            "max_order_attempts_per_day": run_spec.risk.max_order_attempts_per_day,
            "max_order_failures_per_day": run_spec.risk.max_order_failures_per_day,
            "max_price_deviation_bps": run_spec.risk.max_price_deviation_bps,
            "max_market_data_age_ms": run_spec.risk.max_market_data_age_ms,
            "max_consecutive_strategy_losses": run_spec.risk.max_consecutive_strategy_losses,
            "max_consecutive_strategy_errors": run_spec.risk.max_consecutive_strategy_errors,
            "trading_session": run_spec.risk.trading_session.as_ref().map(|session| {
                serde_json::json!({
                    "mode": session.mode,
                    "timezone": session.timezone,
                    "start": session.start,
                    "end": session.end,
                })
            }),
        },
        "broker": {
            "kind": broker_kind_slug(run_spec.broker.kind),
            "mode": broker_mode_slug(run_spec.broker.mode),
            "base_url": run_spec.broker.base_url,
            "host": run_spec.broker.host,
            "port": run_spec.broker.port,
            "client_id": run_spec.broker.client_id,
            "api_key_env": run_spec.broker.api_key_env,
            "secret_key_env": run_spec.broker.secret_key_env,
            "recv_window_ms": run_spec.broker.recv_window_ms,
            "order_submit_enabled": run_spec.broker.order_submit_enabled,
            "fake_startup_unmatched_open_order": run_spec.broker.fake_startup_unmatched_open_order,
        },
        "paper": {
            "account_id": run_spec.paper.account_id,
            "slippage_bps": run_spec.paper.slippage_bps,
            "fee_bps": run_spec.paper.fee_bps,
            "bar_delay_ms": run_spec.paper.bar_delay_ms,
        },
        "live_enabled": run_spec.live_enabled,
    })
}

fn runtime_mode_slug(mode: &config::RuntimeMode) -> &'static str {
    match mode {
        config::RuntimeMode::Backtest => "backtest",
        config::RuntimeMode::Replay => "replay",
        config::RuntimeMode::Paper => "paper",
        config::RuntimeMode::Live => "live",
    }
}

fn load_configured_market_slices(app_config: &config::AppConfig) -> Result<Vec<data::MarketSlice>> {
    let inputs = configured_bar_inputs(app_config)?;
    Ok(data::load_market_slices(&inputs)?)
}

fn configured_bar_inputs(app_config: &config::AppConfig) -> Result<Vec<data::BarInput>> {
    if app_config.data.inputs.is_empty() {
        return Ok(vec![data::BarInput::new(
            primary_strategy_symbol(app_config),
            app_config.data.source.clone(),
            app_config.data.path.clone(),
        )]);
    }

    let input_symbols = app_config
        .data
        .inputs
        .iter()
        .map(|input| input.symbol.as_str())
        .collect::<BTreeSet<_>>();
    for symbol in &app_config.strategy.symbols {
        if !input_symbols.contains(symbol.as_str()) {
            bail!("missing data input for strategy symbol {symbol}");
        }
    }

    Ok(app_config
        .data
        .inputs
        .iter()
        .map(|input| {
            data::BarInput::new(
                input.symbol.clone(),
                input.source.clone(),
                input.path.clone(),
            )
        })
        .collect())
}

fn data_source_description(app_config: &config::AppConfig) -> String {
    if app_config.data.inputs.is_empty() {
        return app_config.data.path.clone();
    }
    app_config
        .data
        .inputs
        .iter()
        .map(|input| format!("{}={}", input.symbol, input.path))
        .collect::<Vec<_>>()
        .join(", ")
}

fn primary_strategy_symbol(app_config: &config::AppConfig) -> String {
    app_config
        .strategy
        .symbols
        .first()
        .cloned()
        .unwrap_or_else(|| "US:NASDAQ:AAPL:EQUITY".to_string())
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

fn ensure_file_parent(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

struct IndicatorBuild {
    builder: String,
    inputs: Vec<data::BarInput>,
    run_id: String,
    feature_name: String,
    period: usize,
    version: String,
    output: String,
    manifest_output: String,
}

fn build_indicator_features(
    indicator: FeatureIndicatorKind,
    build: IndicatorBuild,
) -> Result<usize> {
    let mut records = Vec::new();
    let mut manifest_inputs = Vec::new();
    for input in &build.inputs {
        let bars = data::load_bars(&input.source, &input.path)?;
        manifest_inputs.push(feature_manifest_input_from_bar_input_and_bars(
            input, &bars,
        )?);
        records.extend(indicator_feature_records(
            indicator,
            bars,
            &input.symbol,
            &build.run_id,
            &build.feature_name,
            build.period,
            &build.version,
        )?);
    }
    ensure_file_parent(&build.output)?;
    feature_store::write_feature_records_to_parquet(&build.output, &records)?;
    let manifest = feature_store::build_feature_manifest_with_contract(
        &build.output,
        &records,
        feature_store::FeatureBuildContract {
            builder: build.builder,
            indicator: indicator.label().to_string(),
            value_column: "close".to_string(),
            period: build.period,
            run_id: build.run_id,
            feature_name: build.feature_name,
            version: build.version,
            inputs: manifest_inputs,
        },
    );
    ensure_file_parent(&build.manifest_output)?;
    feature_store::write_feature_manifest(&build.manifest_output, &manifest)?;
    Ok(records.len())
}

fn feature_manifest_input_from_bar_input(
    input: &data::BarInput,
) -> feature_store::FeatureManifestInput {
    feature_store::FeatureManifestInput {
        symbol: input.symbol.clone(),
        source: input.source.clone(),
        path: input.path.clone(),
        content_hash: None,
        bar_count: None,
        first_ts_ms: None,
        last_ts_ms: None,
    }
}

fn feature_manifest_input_from_bar_input_and_bars(
    input: &data::BarInput,
    bars: &[data::Bar],
) -> Result<feature_store::FeatureManifestInput> {
    let mut manifest_input = feature_manifest_input_from_bar_input(input);
    manifest_input.content_hash = Some(stable_file_content_hash(&input.path)?);
    manifest_input.bar_count = Some(bars.len());
    manifest_input.first_ts_ms = bars.first().map(|bar| bar.ts_ms);
    manifest_input.last_ts_ms = bars.last().map(|bar| bar.ts_ms);
    Ok(manifest_input)
}

fn stable_file_content_hash(path: &str) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(stable_bytes_hash(&bytes))
}

fn stable_bytes_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn indicator_inputs(
    source: Option<String>,
    input: Option<String>,
    symbol: Option<String>,
    inputs_config: Option<String>,
) -> Result<Vec<data::BarInput>> {
    match (source, input, symbol, inputs_config) {
        (None, None, None, Some(config_path)) => {
            let app_config = config::AppConfig::from_toml_file(config_path)?;
            configured_bar_inputs(&app_config)
        }
        (Some(source), Some(input), Some(symbol), None) => {
            Ok(vec![data::BarInput::new(symbol, source, input)])
        }
        _ => bail!(
            "feature-build-indicator requires either --inputs-config or all of --source, --input and --symbol"
        ),
    }
}

fn indicator_feature_records(
    indicator: FeatureIndicatorKind,
    bars: Vec<data::Bar>,
    symbol: &str,
    run_id: &str,
    feature_name: &str,
    period: usize,
    version: &str,
) -> Result<Vec<feature_store::FeatureRecord>> {
    match indicator {
        FeatureIndicatorKind::Sma => {
            let mut average = indicators::SimpleMovingAverage::new(period)?;
            Ok(bars
                .into_iter()
                .filter_map(|bar| {
                    average.update(bar.close).map(|value| {
                        feature_store::FeatureRecord::new(
                            run_id.to_string(),
                            symbol.to_string(),
                            bar.ts_ms,
                            feature_name.to_string(),
                            value,
                            version.to_string(),
                        )
                    })
                })
                .collect())
        }
        FeatureIndicatorKind::Ema => {
            let mut average = indicators::ExponentialMovingAverage::new(period)?;
            Ok(bars
                .into_iter()
                .filter_map(|bar| {
                    average.update(bar.close).map(|value| {
                        feature_store::FeatureRecord::new(
                            run_id.to_string(),
                            symbol.to_string(),
                            bar.ts_ms,
                            feature_name.to_string(),
                            value,
                            version.to_string(),
                        )
                    })
                })
                .collect())
        }
        FeatureIndicatorKind::Rsi => {
            let mut index = indicators::RelativeStrengthIndex::new(period)?;
            Ok(bars
                .into_iter()
                .filter_map(|bar| {
                    index.update(bar.close).map(|value| {
                        feature_store::FeatureRecord::new(
                            run_id.to_string(),
                            symbol.to_string(),
                            bar.ts_ms,
                            feature_name.to_string(),
                            value,
                            version.to_string(),
                        )
                    })
                })
                .collect())
        }
    }
}

fn sqlite_file_path(database_url: &str) -> Option<&str> {
    if database_url == "sqlite::memory:" || database_url == "sqlite://:memory:" {
        return None;
    }
    database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
}

fn optional_decimal(value: Option<&str>) -> Result<Option<Decimal>> {
    value.map(Decimal::from_str).transpose().map_err(Into::into)
}

fn trading_session_window(
    session: Option<&config::TradingSessionConfig>,
) -> Result<Option<algorithm::TradingSessionWindow>> {
    Ok(session.map(|session| {
        algorithm::TradingSessionWindow::new(
            session.mode.clone(),
            session.timezone.clone(),
            session.start.clone(),
            session.end.clone(),
        )
    }))
}

fn backtest_settings(app_config: &config::AppConfig) -> Result<BacktestSettings> {
    Ok(BacktestSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        config_json: "{}".to_string(),
        universe_name: app_config.strategy.universe.clone(),
        alpha_name: app_config.strategy.alpha.clone(),
        symbols: app_config.strategy.symbols.clone(),
        universe_filter: strategy_universe_filter(app_config)?,
        alpha_components: strategy_alpha_components(app_config),
        alpha_conflict_resolution: strategy_alpha_conflict_resolution(app_config)?,
        alpha_gate: strategy_alpha_gate(app_config)?,
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
        allow_short: app_config.effective_allow_short(),
        shortable_symbols: app_config.shortable_symbols(),
        initial_equity: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        daily_loss_limit: optional_decimal(app_config.risk.daily_loss_limit.as_deref())?,
        max_order_attempts_per_day: app_config.risk.max_order_attempts_per_day,
        max_order_failures_per_day: app_config.risk.max_order_failures_per_day,
        max_price_deviation_bps: optional_decimal(
            app_config.risk.max_price_deviation_bps.as_deref(),
        )?,
        max_market_data_age_ms: app_config.risk.max_market_data_age_ms,
        max_consecutive_strategy_losses: app_config.risk.max_consecutive_strategy_losses,
        max_consecutive_strategy_errors: app_config.risk.max_consecutive_strategy_errors,
        trading_session: trading_session_window(app_config.risk.trading_session.as_ref())?,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
        logging: log_writer_settings(app_config),
    })
}

fn paper_settings(app_config: &config::AppConfig) -> Result<PaperSettings> {
    Ok(PaperSettings {
        run_id: app_config.runtime.run_id.clone(),
        strategy_name: app_config.strategy.name.clone(),
        config_json: "{}".to_string(),
        universe_name: app_config.strategy.universe.clone(),
        alpha_name: app_config.strategy.alpha.clone(),
        symbols: app_config.strategy.symbols.clone(),
        universe_filter: strategy_universe_filter(app_config)?,
        alpha_components: strategy_alpha_components(app_config),
        alpha_conflict_resolution: strategy_alpha_conflict_resolution(app_config)?,
        alpha_gate: strategy_alpha_gate(app_config)?,
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
        allow_short: app_config.effective_allow_short(),
        shortable_symbols: app_config.shortable_symbols(),
        initial_cash: Decimal::from_str(&app_config.portfolio.initial_cash)?,
        daily_loss_limit: optional_decimal(app_config.risk.daily_loss_limit.as_deref())?,
        max_order_attempts_per_day: app_config.risk.max_order_attempts_per_day,
        max_order_failures_per_day: app_config.risk.max_order_failures_per_day,
        max_price_deviation_bps: optional_decimal(
            app_config.risk.max_price_deviation_bps.as_deref(),
        )?,
        max_market_data_age_ms: app_config.risk.max_market_data_age_ms,
        max_consecutive_strategy_losses: app_config.risk.max_consecutive_strategy_losses,
        max_consecutive_strategy_errors: app_config.risk.max_consecutive_strategy_errors,
        trading_session: trading_session_window(app_config.risk.trading_session.as_ref())?,
        base_currency: app_config.portfolio.base_currency.clone(),
        slippage_bps: Decimal::from_str(&app_config.paper.slippage_bps)?,
        fee_bps: Decimal::from_str(&app_config.paper.fee_bps)?,
        simulated_funding_rate: None,
        fast_window: app_config.strategy.fast_window,
        slow_window: app_config.strategy.slow_window,
        bar_delay_ms: app_config.paper.bar_delay_ms.unwrap_or(0),
        logging: log_writer_settings(app_config),
    })
}

fn log_writer_settings(app_config: &config::AppConfig) -> LogWriterSettings {
    LogWriterSettings {
        enabled: app_config.logging.enabled,
        buffer_size: app_config.logging.buffer_size,
        flush_interval_ms: app_config.logging.flush_interval_ms,
        min_level: app_config.logging.level.clone(),
        categories: app_config.logging.categories.clone(),
        ..LogWriterSettings::default()
    }
}

fn system_log_retention_policy(
    app_config: &config::AppConfig,
) -> storage::SystemLogRetentionPolicy {
    storage::SystemLogRetentionPolicy {
        retention_days: app_config.logging.retention_days,
    }
}

async fn run_configured_log_retention(
    db: &storage::Db,
    app_config: &config::AppConfig,
) -> Result<u64> {
    Ok(db
        .purge_system_logs_by_retention(
            chrono::Utc::now().timestamp_millis(),
            system_log_retention_policy(app_config),
        )
        .await?)
}

fn strategy_universe_filter(
    app_config: &config::AppConfig,
) -> Result<strategies::StrategyUniverseFilterConfig> {
    Ok(strategies::StrategyUniverseFilterConfig {
        include_symbols: app_config.strategy.universe_filter.include_symbols.clone(),
        exclude_symbols: app_config.strategy.universe_filter.exclude_symbols.clone(),
        symbol_prefixes: app_config.strategy.universe_filter.symbol_prefixes.clone(),
        require_current_data: app_config.strategy.universe_filter.require_current_data,
        max_symbols: app_config.strategy.universe_filter.max_symbols,
        feature_rank: strategy_universe_rank(app_config)?,
    })
}

fn strategy_universe_rank(
    app_config: &config::AppConfig,
) -> Result<Option<strategies::StrategyUniverseRankConfig>> {
    let Some(rank) = &app_config.strategy.universe_rank else {
        return Ok(None);
    };
    if rank.source != "parquet" {
        bail!(
            "unsupported universe rank feature source {}; expected parquet",
            rank.source
        );
    }
    if let Some(manifest_path) = &rank.manifest_path {
        let manifest = feature_store::load_feature_manifest(manifest_path)?;
        feature_store::validate_feature_manifest_for_contract(
            &manifest,
            &rank.path,
            &rank.run_id,
            &app_config.strategy.symbols,
            &rank.feature_name,
            rank.version.as_deref(),
        )?;
        validate_feature_manifest_input_contract(&manifest, app_config)?;
        validate_feature_manifest_build_contract(
            &manifest,
            rank.build_indicator.clone(),
            rank.build_period,
            rank.build_value_column.clone(),
        )?;
    }
    Ok(Some(strategies::StrategyUniverseRankConfig {
        run_id: rank.run_id.clone(),
        feature_name: rank.feature_name.clone(),
        version: rank.version.clone(),
        descending: rank.descending,
        records: feature_store::load_feature_records_from_parquet(&rank.path)?,
    }))
}

fn strategy_alpha_components(
    app_config: &config::AppConfig,
) -> Vec<strategies::StrategyAlphaComponentConfig> {
    app_config
        .strategy
        .alpha_components
        .iter()
        .map(|component| strategies::StrategyAlphaComponentConfig {
            name: component.name.clone(),
            category: component.category.clone(),
            fast_window: component.fast_window,
            slow_window: component.slow_window,
            weight: component.weight,
        })
        .collect()
}

fn strategy_alpha_conflict_resolution(
    app_config: &config::AppConfig,
) -> Result<strategies::StrategyAlphaConflictResolution> {
    match app_config.strategy.alpha_conflict_resolution.as_str() {
        "highest_confidence" => Ok(strategies::StrategyAlphaConflictResolution::HighestConfidence),
        "net_signal" => Ok(strategies::StrategyAlphaConflictResolution::NetSignal),
        "majority_vote" => Ok(strategies::StrategyAlphaConflictResolution::MajorityVote),
        "category_majority" => Ok(strategies::StrategyAlphaConflictResolution::CategoryMajority),
        other => bail!("unknown alpha conflict resolution {other}"),
    }
}

fn strategy_alpha_gate(
    app_config: &config::AppConfig,
) -> Result<Option<strategies::StrategyAlphaGateConfig>> {
    let Some(gate) = &app_config.strategy.alpha_gate else {
        return Ok(None);
    };
    if gate.source != "parquet" {
        bail!(
            "unsupported alpha gate feature source {}; expected parquet",
            gate.source
        );
    }
    if let Some(manifest_path) = &gate.manifest_path {
        let manifest = feature_store::load_feature_manifest(manifest_path)?;
        feature_store::validate_feature_manifest_for_contract(
            &manifest,
            &gate.path,
            &gate.run_id,
            &app_config.strategy.symbols,
            &gate.feature_name,
            gate.version.as_deref(),
        )?;
        validate_feature_manifest_input_contract(&manifest, app_config)?;
        validate_feature_manifest_build_contract(
            &manifest,
            gate.build_indicator.clone(),
            gate.build_period,
            gate.build_value_column.clone(),
        )?;
    }
    Ok(Some(strategies::StrategyAlphaGateConfig {
        run_id: gate.run_id.clone(),
        feature_name: gate.feature_name.clone(),
        version: gate.version.clone(),
        min_value: gate
            .min_value
            .as_deref()
            .map(Decimal::from_str)
            .transpose()?,
        max_value: gate
            .max_value
            .as_deref()
            .map(Decimal::from_str)
            .transpose()?,
        records: feature_store::load_feature_records_from_parquet(&gate.path)?,
    }))
}

fn validate_feature_manifest_input_contract(
    manifest: &feature_store::FeatureManifest,
    app_config: &config::AppConfig,
) -> Result<()> {
    let inputs = configured_bar_inputs(app_config)?;
    let mut manifest_inputs = Vec::with_capacity(inputs.len());
    for input in &inputs {
        let bars = data::load_bars(&input.source, &input.path)?;
        manifest_inputs.push(feature_manifest_input_from_bar_input_and_bars(
            input, &bars,
        )?);
    }
    feature_store::validate_feature_manifest_for_input_contract(manifest, &manifest_inputs)?;
    Ok(())
}

fn validate_feature_manifest_build_contract(
    manifest: &feature_store::FeatureManifest,
    indicator: Option<String>,
    period: Option<usize>,
    value_column: Option<String>,
) -> Result<()> {
    feature_store::validate_feature_manifest_for_build_contract(
        manifest,
        &feature_store::FeatureBuildContractExpectation {
            indicator,
            value_column,
            period,
        },
    )?;
    Ok(())
}

fn gate_failure_values(fields: &serde_json::Value, key: &str) -> Vec<String> {
    fields
        .get("failures")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|failure| failure.get(key).and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        LocalFill, LocalOrder, backtest_settings, binance_accounting_records_from_fills,
        binance_balance_total, binance_base_asset, binance_cancel_outcome,
        binance_local_order_matches_remote_open, binance_testnet_settings,
        ibkr_execution_match_summary, ibkr_local_fill_state_counts,
        ibkr_local_order_expects_remote_open, ibkr_local_order_in_scope,
        ibkr_local_order_matches_remote_open, ibkr_recovered_order_status, paper_settings,
        settings_with_broker_initial_cash, sync_cancelled_open_orders, system_log_retention_policy,
    };
    use broker::{
        BinanceAssetBalance, BinanceOpenOrder, BrokerOpenOrder, BrokerOrder, BrokerOrderStatus,
        CancelledOpenOrder, IbkrExecution, IbkrOpenOrder,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use trader_core::{OrderSide, OrderType};

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
            &[LocalFill {
                id: "fill-1".to_string(),
                order_id: "order-1".to_string(),
                symbol: "BTCUSDT".to_string(),
                side: "BUY".to_string(),
                price: "63820.39".to_string(),
                qty: "0.001".to_string(),
                fee: "0.01".to_string(),
            }],
            11,
        )
        .unwrap();

        assert_eq!(records.balance.total, dec!(9936.17961));
        let position = records.position.unwrap();
        assert_eq!(position.symbol, "BTCUSDT");
        assert_eq!(position.qty, dec!(0.001));
        assert_eq!(position.avg_price, dec!(63820.39));
        assert_eq!(records.snapshot.market_value, dec!(63.82039));
        assert_eq!(records.snapshot.equity, dec!(10000.00000));
    }

    #[test]
    fn binance_accounting_records_accumulate_existing_fills() {
        let fills = vec![
            LocalFill {
                id: "fill-1".to_string(),
                order_id: "order-1".to_string(),
                symbol: "BTCUSDT".to_string(),
                side: "BUY".to_string(),
                price: "63820.39".to_string(),
                qty: "0.001".to_string(),
                fee: "0.01".to_string(),
            },
            LocalFill {
                id: "fill-2".to_string(),
                order_id: "order-2".to_string(),
                symbol: "BTCUSDT".to_string(),
                side: "BUY".to_string(),
                price: "63960".to_string(),
                qty: "0.001".to_string(),
                fee: "0.01".to_string(),
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
        assert_eq!(position.qty, dec!(0.002));
        assert_eq!(position.avg_price, dec!(63890.1950));
        assert_eq!(records.snapshot.market_value, dec!(127.7803900));
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
        let by_client = LocalOrder {
            id: "order-client-42".to_string(),
            client_order_id: "client-42".to_string(),
            broker_order_id: None,
            account_id: "binance-testnet".to_string(),
            symbol: "BTCUSDT".to_string(),
            qty: "0.001".to_string(),
            filled_qty: "0".to_string(),
            status: "NEW".to_string(),
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
    fn ibkr_reconcile_matches_local_order_by_client_or_broker_id() {
        let remote_orders = vec![
            IbkrOpenOrder {
                order_id: 42,
                account_id: "DU12345".to_string(),
                symbol: "AAPL".to_string(),
                side: "BUY".to_string(),
                order_type: "LMT".to_string(),
                quantity: dec!(1),
                limit_price: Some(dec!(185.25)),
                status: "Submitted".to_string(),
                client_order_id: "client-42".to_string(),
                filled_qty: Decimal::ZERO,
            },
            IbkrOpenOrder {
                order_id: 77,
                account_id: "DU12345".to_string(),
                symbol: "AAPL".to_string(),
                side: "SELL".to_string(),
                order_type: "LMT".to_string(),
                quantity: dec!(1),
                limit_price: Some(dec!(186)),
                status: "Submitted".to_string(),
                client_order_id: String::new(),
                filled_qty: Decimal::ZERO,
            },
        ];
        let by_client = sample_order("client-42", None);
        let by_broker = sample_order("other-client", Some("77"));

        assert!(ibkr_local_order_matches_remote_open(
            &by_client,
            &remote_orders
        ));
        assert!(ibkr_local_order_matches_remote_open(
            &by_broker,
            &remote_orders
        ));
    }

    #[test]
    fn parses_gate_account_requirement() {
        let requirement =
            runtime::parse_reconciliation_gate_account_requirement("ibkr:DU****91").unwrap();

        assert_eq!(requirement.broker, "ibkr");
        assert_eq!(requirement.account_id, "DU****91");
    }

    #[test]
    fn rejects_gate_account_requirement_without_separator() {
        let error = runtime::parse_reconciliation_gate_account_requirement("ibkr")
            .unwrap_err()
            .to_string();

        assert!(error.contains("expected broker:account_id"));
    }

    #[test]
    fn ibkr_reconcile_only_expects_active_local_orders_to_be_remote_open() {
        for status in [
            "SUBMITTED",
            "NEW",
            "PARTIALLY_FILLED",
            "PendingSubmit",
            "PreSubmitted",
            "Submitted",
            "ApiPending",
        ] {
            let mut order = sample_order("client-active", None);
            order.status = status.to_string();
            assert!(
                ibkr_local_order_expects_remote_open(&order),
                "status should require remote open order: {status}"
            );
        }

        for status in [
            "PendingCancel",
            "Cancelled",
            "Canceled",
            "ApiCancelled",
            "Filled",
            "Inactive",
            "REJECTED",
            "EXPIRED",
        ] {
            let mut order = sample_order("client-terminal", None);
            order.status = status.to_string();
            assert!(
                !ibkr_local_order_expects_remote_open(&order),
                "status should not require remote open order: {status}"
            );
        }

        let mut filled_order = sample_order("client-filled", None);
        filled_order.status = "PreSubmitted".to_string();
        filled_order.filled_qty = "1".to_string();
        assert!(!ibkr_local_order_expects_remote_open(&filled_order));
    }

    #[test]
    fn ibkr_reconcile_matches_aggregated_execution_fields() {
        let executions = [
            IbkrExecution {
                request_id: 1,
                order_id: 42,
                client_order_id: "client-42".to_string(),
                trade_id: "exec-42-a".to_string(),
                symbol: "AAPL".to_string(),
                side: "BOT".to_string(),
                qty: dec!(0.4),
                price: dec!(185),
                fee: dec!(0.15),
            },
            IbkrExecution {
                request_id: 1,
                order_id: 42,
                client_order_id: "client-42".to_string(),
                trade_id: "exec-42-b".to_string(),
                symbol: "AAPL".to_string(),
                side: "BUY".to_string(),
                qty: dec!(0.6),
                price: dec!(185.5),
                fee: dec!(0.20),
            },
            IbkrExecution {
                request_id: 1,
                order_id: 5,
                client_order_id: "trader-diagnostic-order".to_string(),
                trade_id: "exec-diagnostic".to_string(),
                symbol: "AAPL".to_string(),
                side: "BUY".to_string(),
                qty: dec!(1),
                price: dec!(318.98),
                fee: dec!(1.000003),
            },
        ];
        let order = sample_order("client-42", None);
        let fill = LocalFill {
            id: "fill-42".to_string(),
            order_id: order.id.clone(),
            symbol: "US:SMART:AAPL:EQUITY".to_string(),
            side: "BUY".to_string(),
            price: "185.3".to_string(),
            qty: "1".to_string(),
            fee: "0.35".to_string(),
        };

        let summary = ibkr_execution_match_summary(&executions, &[order], &[fill]).unwrap();

        assert_eq!(summary.matched, 2);
        assert_eq!(summary.matched_orders, 1);
        assert_eq!(summary.max_per_order, 2);
        assert_eq!(summary.field_drifts, 0);
        assert_eq!(summary.matched_qty, dec!(1));
    }

    #[test]
    fn ibkr_reconcile_detects_execution_field_drift() {
        let execution = IbkrExecution {
            request_id: 1,
            order_id: 42,
            client_order_id: "client-42".to_string(),
            trade_id: "exec-42".to_string(),
            symbol: "AAPL".to_string(),
            side: "BUY".to_string(),
            qty: dec!(1),
            price: dec!(185.25),
            fee: dec!(0.35),
        };
        let order = sample_order("client-42", Some("42"));
        let fill = LocalFill {
            id: "fill-42".to_string(),
            order_id: order.id.clone(),
            symbol: "US:SMART:MSFT:EQUITY".to_string(),
            side: "SELL".to_string(),
            price: "186".to_string(),
            qty: "2".to_string(),
            fee: "0.70".to_string(),
        };

        let summary = ibkr_execution_match_summary(&[execution], &[order], &[fill]).unwrap();

        assert_eq!(summary.matched, 1);
        assert_eq!(summary.matched_orders, 1);
        assert_eq!(summary.max_per_order, 1);
        assert_eq!(summary.field_drifts, 1);
        assert_eq!(summary.matched_qty, dec!(1));
    }

    #[test]
    fn ibkr_reconcile_does_not_match_execution_without_local_fill() {
        let execution = IbkrExecution {
            request_id: 1,
            order_id: 42,
            client_order_id: "client-42".to_string(),
            trade_id: "exec-42".to_string(),
            symbol: "AAPL".to_string(),
            side: "BUY".to_string(),
            qty: dec!(1),
            price: dec!(185.25),
            fee: dec!(0.35),
        };

        let summary = ibkr_execution_match_summary(
            &[execution],
            &[sample_order("client-42", Some("42"))],
            &[],
        )
        .unwrap();

        assert_eq!(summary.matched, 0);
        assert_eq!(summary.matched_orders, 0);
        assert_eq!(summary.max_per_order, 0);
        assert_eq!(summary.field_drifts, 0);
        assert_eq!(summary.matched_qty, Decimal::ZERO);
    }

    #[test]
    fn ibkr_reconcile_scopes_local_orders_by_account_and_symbol() {
        let matching = sample_order("client-aapl", Some("42"));
        let mut other_symbol = matching.clone();
        other_symbol.symbol = "US:SMART:MSFT:EQUITY".to_string();
        let mut other_account = matching.clone();
        other_account.account_id = "DU99999".to_string();

        assert!(ibkr_local_order_in_scope(&matching, "DU12345", "AAPL"));
        assert!(!ibkr_local_order_in_scope(&other_symbol, "DU12345", "AAPL"));
        assert!(!ibkr_local_order_in_scope(
            &other_account,
            "DU12345",
            "AAPL"
        ));
    }

    #[test]
    fn ibkr_reconcile_counts_full_and_partial_local_orders() {
        let mut full = sample_order("client-full", Some("42"));
        full.filled_qty = "1".to_string();
        let mut partial = sample_order("client-partial", Some("43"));
        partial.filled_qty = "0.4".to_string();
        let unfilled = sample_order("client-unfilled", Some("44"));

        let counts = ibkr_local_fill_state_counts(&[full, partial, unfilled]).unwrap();

        assert_eq!(counts, (1, 1));
    }

    #[test]
    fn ibkr_recover_prefers_open_order_status() {
        let local = sample_order("client-42", None);
        let remote = IbkrOpenOrder {
            order_id: 42,
            account_id: "DU12345".to_string(),
            symbol: "AAPL".to_string(),
            side: "BUY".to_string(),
            order_type: "LMT".to_string(),
            quantity: dec!(1),
            limit_price: Some(dec!(185.25)),
            status: "Submitted".to_string(),
            client_order_id: "client-42".to_string(),
            filled_qty: Decimal::ZERO,
        };

        let status = ibkr_recovered_order_status(&local, Some(&remote), Decimal::ZERO).unwrap();

        assert_eq!(status, "Submitted");
    }

    #[test]
    fn ibkr_recover_marks_execution_only_order_by_filled_qty() {
        let local = sample_order("client-42", Some("42"));

        let partial = ibkr_recovered_order_status(&local, None, dec!(0.5)).unwrap();
        let filled = ibkr_recovered_order_status(&local, None, dec!(1)).unwrap();

        assert_eq!(partial, "PARTIALLY_FILLED");
        assert_eq!(filled, "FILLED");
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
    fn live_worker_binance_paper_requires_testnet_credentials() {
        unsafe {
            std::env::remove_var("BINANCE_TESTNET_API_KEY");
            std::env::remove_var("BINANCE_TESTNET_SECRET_KEY");
        }
        let config = sample_app_config();

        let error = match super::live_worker_broker_for_config(&config) {
            Ok(_) => panic!("expected Binance live-worker broker to require testnet credentials"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("BINANCE_TESTNET_API_KEY"));
    }

    #[test]
    fn binance_submit_uses_broker_cash_as_initial_cash() {
        let mut settings = paper::PaperSettings::sample();
        settings.initial_cash = dec!(100000);

        let settings = settings_with_broker_initial_cash(settings, dec!(9744.45561));

        assert_eq!(settings.initial_cash, dec!(9744.45561));
    }

    #[test]
    fn runtime_settings_use_logging_config_for_writer_tuning() {
        let mut config = sample_app_config();
        config.logging.enabled = false;
        config.logging.buffer_size = 64;
        config.logging.flush_interval_ms = 250;
        config.logging.retention_days = 7;

        let backtest = backtest_settings(&config).unwrap();
        let paper = paper_settings(&config).unwrap();
        let retention = system_log_retention_policy(&config);

        assert!(!backtest.logging.enabled);
        assert_eq!(backtest.logging.buffer_size, 64);
        assert_eq!(backtest.logging.flush_interval_ms, 250);
        assert!(!paper.logging.enabled);
        assert_eq!(paper.logging.buffer_size, 64);
        assert_eq!(paper.logging.flush_interval_ms, 250);
        assert_eq!(retention.retention_days, 7);
    }

    #[tokio::test]
    async fn kill_switch_cancel_sync_updates_local_orders_by_client_or_broker_id() {
        let db = storage::Db::connect("sqlite::memory:").await.unwrap();
        db.migrate().await.unwrap();
        db.insert_order(storage::NewOrder {
            id: "order-client".to_string(),
            run_id: "run-1".to_string(),
            client_order_id: "client-42".to_string(),
            broker_order_id: None,
            account_id: "paper".to_string(),
            symbol: "AAPL".to_string(),
            side: "BUY".to_string(),
            order_type: "LIMIT".to_string(),
            price: Some("185".to_string()),
            qty: "1".to_string(),
            filled_qty: "0".to_string(),
            status: "SUBMITTED".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
        })
        .await
        .unwrap();
        db.insert_order(storage::NewOrder {
            id: "order-broker".to_string(),
            run_id: "run-1".to_string(),
            client_order_id: "client-local-only".to_string(),
            broker_order_id: Some("broker-77".to_string()),
            account_id: "paper".to_string(),
            symbol: "AAPL".to_string(),
            side: "SELL".to_string(),
            order_type: "LIMIT".to_string(),
            price: Some("186".to_string()),
            qty: "1".to_string(),
            filled_qty: "0".to_string(),
            status: "SUBMITTED".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
        })
        .await
        .unwrap();

        let synced = sync_cancelled_open_orders(
            &db,
            "run-1",
            &[
                cancelled_open_order("client-42", "broker-42"),
                cancelled_open_order("", "broker-77"),
            ],
        )
        .await
        .unwrap();

        assert_eq!(synced, 2);
        let client_order = db
            .get_order_by_client_order_id("client-42")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(client_order.broker_order_id.as_deref(), Some("broker-42"));
        assert_eq!(client_order.status, "CANCELLED");
        let broker_order = db
            .get_order_by_client_order_id("client-local-only")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(broker_order.broker_order_id.as_deref(), Some("broker-77"));
        assert_eq!(broker_order.status, "CANCELLED");
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

    fn sample_order(client_order_id: &str, broker_order_id: Option<&str>) -> LocalOrder {
        LocalOrder {
            id: format!("order-{client_order_id}"),
            client_order_id: client_order_id.to_string(),
            broker_order_id: broker_order_id.map(str::to_string),
            account_id: "DU12345".to_string(),
            symbol: "US:SMART:AAPL:EQUITY".to_string(),
            qty: "1".to_string(),
            filled_qty: "0".to_string(),
            status: "SUBMITTED".to_string(),
        }
    }

    fn cancelled_open_order(client_order_id: &str, broker_order_id: &str) -> CancelledOpenOrder {
        CancelledOpenOrder {
            open_order: BrokerOpenOrder {
                broker_order_id: broker_order_id.to_string(),
                client_order_id: client_order_id.to_string(),
                account_id: "paper".to_string(),
                symbol: "AAPL".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                price: Some(dec!(185)),
                qty: dec!(1),
                filled_qty: dec!(0),
                status: "Submitted".to_string(),
            },
            cancelled_order: BrokerOrder {
                broker_order_id: broker_order_id.to_string(),
                account_id: "paper".to_string(),
                symbol: "AAPL".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                qty: dec!(1),
                price: Some(dec!(185)),
                status: BrokerOrderStatus::Cancelled,
            },
        }
    }
}
