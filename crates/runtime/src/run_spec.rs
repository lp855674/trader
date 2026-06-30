use config::AppConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpec {
    pub run_id: String,
    pub mode: config::RuntimeMode,
    pub strategy: StrategySpec,
    pub data: DataSpec,
    pub portfolio: PortfolioSpec,
    pub risk: RiskSpec,
    pub broker: BrokerSpec,
    pub paper: PaperSpec,
    pub live_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategySpec {
    pub name: String,
    pub universe: String,
    pub alpha: String,
    pub alpha_conflict_resolution: String,
    pub symbols: Vec<String>,
    pub fast_window: usize,
    pub slow_window: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSpec {
    pub source: String,
    pub path: String,
    pub inputs: Vec<DataInputSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataInputSpec {
    pub symbol: String,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioSpec {
    pub initial_cash: String,
    pub base_currency: String,
    pub order_qty: String,
    pub max_abs_qty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskSpec {
    pub max_order_notional: String,
    pub min_cash_after_order: String,
    pub max_exposure: String,
    pub max_drawdown: String,
    pub max_leverage: String,
    pub max_margin_used: String,
    pub trading_halted: bool,
    pub allow_short: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerSpec {
    pub kind: config::BrokerKind,
    pub mode: config::BrokerMode,
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub client_id: Option<u32>,
    pub api_key_env: Option<String>,
    pub secret_key_env: Option<String>,
    pub recv_window_ms: Option<u64>,
    pub order_submit_enabled: bool,
    pub fake_startup_unmatched_open_order: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperSpec {
    pub account_id: String,
    pub slippage_bps: String,
    pub fee_bps: String,
    pub bar_delay_ms: Option<u64>,
}

impl From<&AppConfig> for RunSpec {
    fn from(value: &AppConfig) -> Self {
        Self {
            run_id: value.runtime.run_id.clone(),
            mode: value.runtime.mode.clone(),
            strategy: StrategySpec {
                name: value.strategy.name.clone(),
                universe: value.strategy.universe.clone(),
                alpha: value.strategy.alpha.clone(),
                alpha_conflict_resolution: value.strategy.alpha_conflict_resolution.clone(),
                symbols: value.strategy.symbols.clone(),
                fast_window: value.strategy.fast_window,
                slow_window: value.strategy.slow_window,
            },
            data: DataSpec {
                source: value.data.source.clone(),
                path: value.data.path.clone(),
                inputs: value
                    .data
                    .inputs
                    .iter()
                    .map(|input| DataInputSpec {
                        symbol: input.symbol.clone(),
                        source: input.source.clone(),
                        path: input.path.clone(),
                    })
                    .collect(),
            },
            portfolio: PortfolioSpec {
                initial_cash: value.portfolio.initial_cash.clone(),
                base_currency: value.portfolio.base_currency.clone(),
                order_qty: value.portfolio.order_qty.clone(),
                max_abs_qty: value.portfolio.max_abs_qty.clone(),
            },
            risk: RiskSpec {
                max_order_notional: value.risk.max_order_notional.clone(),
                min_cash_after_order: value.risk.min_cash_after_order.clone(),
                max_exposure: value.risk.max_exposure.clone(),
                max_drawdown: value.risk.max_drawdown.clone(),
                max_leverage: value.risk.max_leverage.clone(),
                max_margin_used: value.risk.max_margin_used.clone(),
                trading_halted: value.risk.trading_halted,
                allow_short: value.risk.allow_short,
            },
            broker: BrokerSpec {
                kind: value.broker.kind,
                mode: value.broker.mode,
                base_url: value.broker.base_url.clone(),
                host: value.broker.host.clone(),
                port: value.broker.port,
                client_id: value.broker.client_id,
                api_key_env: value.broker.api_key_env.clone(),
                secret_key_env: value.broker.secret_key_env.clone(),
                recv_window_ms: value.broker.recv_window_ms,
                order_submit_enabled: value.broker.order_submit_enabled,
                fake_startup_unmatched_open_order: value.broker.fake_startup_unmatched_open_order,
            },
            paper: PaperSpec {
                account_id: value.paper.account_id.clone(),
                slippage_bps: value.paper.slippage_bps.clone(),
                fee_bps: value.paper.fee_bps.clone(),
                bar_delay_ms: value.paper.bar_delay_ms,
            },
            live_enabled: value.live.enabled,
        }
    }
}
