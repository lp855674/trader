use algorithm::{
    AccountingUpdatedPayload, AlgorithmEngine, AlgorithmEngineSettings, AlgorithmOrderPayload,
    AlphaGeneratedPayload, BrokerPositionSnapshot, ContractAccountingBook, ContractFill,
    EngineEventKind, FundingRateEvent, PositionSide, SimulatedContractAccounting,
    UniverseSelectedPayload,
};
use alpha::{AlphaModel, CompositeAlphaModel};
use data::{Bar, MarketSlice, SymbolBar};
use events::{EventBus, SignalEvent, SignalSide};
use rust_decimal_macros::dec;
use std::collections::BTreeSet;
use strategies::{MovingAverageCrossStrategy, StrategyRuntimeMode};
use trader_core::OrderSide;
use universe::StaticUniverseSelector;

#[tokio::test]
async fn contract_accounting_opens_updates_and_closes_long_position() {
    let mut book = SimulatedContractAccounting::new("paper".to_string(), dec!(5));

    book.on_fill(&contract_fill(OrderSide::Buy, dec!(1), dec!(100), 1))
        .await
        .unwrap();
    book.on_fill(&contract_fill(OrderSide::Buy, dec!(1), dec!(120), 2))
        .await
        .unwrap();

    let increased = book
        .get_position(
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            PositionSide::Long,
        )
        .unwrap();
    assert_eq!(increased.qty, dec!(2));
    assert_eq!(increased.avg_price, dec!(110));
    assert_eq!(increased.margin_used, dec!(44));

    book.on_fill(&contract_fill(OrderSide::Sell, dec!(2), dec!(130), 3))
        .await
        .unwrap();

    let closed = book
        .get_position(
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            PositionSide::Long,
        )
        .unwrap();
    assert_eq!(closed.qty, dec!(0));
    assert_eq!(closed.avg_price, dec!(0));
    assert_eq!(closed.margin_used, dec!(0));
    assert_eq!(closed.realized_pnl, dec!(40));
}

#[tokio::test]
async fn contract_accounting_settles_funding_into_fee_and_realized_pnl() {
    let mut book = SimulatedContractAccounting::new("paper".to_string(), dec!(10));

    book.on_fill(&contract_fill(OrderSide::Buy, dec!(0.5), dec!(65000), 1))
        .await
        .unwrap();
    book.on_funding(&FundingRateEvent {
        exchange: "BINANCE".to_string(),
        symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        funding_time_ms: 2,
        funding_rate: dec!(0.0001),
        mark_price: dec!(65000),
    })
    .await
    .unwrap();

    let position = book
        .get_position(
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
            PositionSide::Long,
        )
        .unwrap();
    assert_eq!(position.funding_fee, dec!(-3.25));
    assert_eq!(position.realized_pnl, dec!(-3.25));
}

#[tokio::test]
async fn contract_accounting_reconciliation_reports_qty_drift() {
    let mut book = SimulatedContractAccounting::new("paper".to_string(), dec!(5));
    book.on_fill(&contract_fill(OrderSide::Buy, dec!(1), dec!(100), 1))
        .await
        .unwrap();

    let report = book
        .on_reconciliation(&BrokerPositionSnapshot {
            account_id: "paper".to_string(),
            exchange: "BINANCE".to_string(),
            symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
            position_side: PositionSide::Long,
            qty: dec!(1.25),
            avg_price: dec!(100),
            margin_used: dec!(20),
            ts_ms: 2,
        })
        .await
        .unwrap();

    assert_eq!(report.drift_count(), 1);
    assert!(report.drifts[0].reason.contains("qty mismatch"));
}

#[test]
fn algorithm_engine_emits_full_decision_chain_for_selected_symbol() {
    let strategy = MovingAverageCrossStrategy::new("ma", "US:NASDAQ:AAPL:EQUITY", 2, 3).unwrap();
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-a".to_string(),
            mode: StrategyRuntimeMode::Paper,
            account_id: "paper".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::new(),
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );

    let bars = [bar(1, dec!(100)), bar(2, dec!(101)), bar(3, dec!(102))];
    let mut decision = None;
    for current_bar in bars {
        decision = engine.on_bar(current_bar).unwrap().decision;
    }

    let decision = decision.expect("third bar should produce a trading decision");
    let kinds = decision
        .events
        .iter()
        .map(|event| event.kind)
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            EngineEventKind::UniverseSelected,
            EngineEventKind::AlphaGenerated,
            EngineEventKind::PortfolioTargetGenerated,
            EngineEventKind::MarketRuleValidated,
            EngineEventKind::RiskApproved,
            EngineEventKind::ExecutionOrderGenerated,
            EngineEventKind::OmsSubmitted,
            EngineEventKind::OmsAccepted,
        ]
    );
    assert_eq!(decision.order.unwrap().symbol, "US:NASDAQ:AAPL:EQUITY");
}

#[test]
fn algorithm_engine_uses_highest_confidence_composite_alpha_signal() {
    let strategy = CompositeAlphaModel::new(vec![
        Box::new(FixedAlphaModel::new("low", SignalSide::Sell, 0.2)),
        Box::new(FixedAlphaModel::new("high", SignalSide::Buy, 0.9)),
    ]);
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-composite".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::new(),
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );

    let decision = engine
        .on_bar(bar(1, dec!(100)))
        .unwrap()
        .decision
        .expect("composite alpha should emit a decision");

    assert_eq!(decision.order.unwrap().side, trader_core::OrderSide::Buy);
}

#[test]
fn algorithm_engine_applies_sell_signal_as_short_position() {
    let strategy = CompositeAlphaModel::new(vec![Box::new(FixedAlphaModel::new(
        "short-alpha",
        SignalSide::Sell,
        0.9,
    ))]);
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-short".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: true,
            shortable_symbols: BTreeSet::new(),
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );

    let decision = engine
        .on_bar(bar(1, dec!(100)))
        .unwrap()
        .decision
        .expect("sell alpha should emit a short order");
    let order = decision.order.unwrap();
    assert_eq!(order.side, trader_core::OrderSide::Sell);
    assert_eq!(order.qty, dec!(1));

    let applied = engine
        .apply_execution(
            &order,
            &algorithm::ExecutionReport {
                broker_order_id: "short-fill".to_string(),
                status: "FILLED".to_string(),
                price: dec!(100),
                qty: dec!(1),
                fee: dec!(0),
            },
            1,
        )
        .unwrap();

    assert_eq!(applied.snapshot.cash, dec!(100100));
    assert_eq!(applied.snapshot.market_value, dec!(-100));
    assert_eq!(applied.snapshot.equity, dec!(100000));
    assert_eq!(applied.snapshot.position_qty, dec!(-1));
    assert_eq!(applied.snapshot.positions[0].qty, dec!(-1));
}

#[test]
fn algorithm_engine_allows_short_only_for_configured_symbols_in_mixed_universe() {
    let mut shortable_symbols = BTreeSet::new();
    shortable_symbols.insert("CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string());
    let mut engine = AlgorithmEngine::new_with_universe(
        AlgorithmEngineSettings {
            run_id: "run-mixed-short".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: false,
            shortable_symbols,
            initial_cash: dec!(100000),
        },
        Box::new(StaticUniverseSelector::new(vec![
            "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        ])),
        Box::new(SymbolSellAlphaModel),
    );

    let perp_step = engine
        .on_market_slice(MarketSlice::new(
            1,
            vec![SymbolBar::new(
                "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP",
                bar(1, dec!(100)),
            )],
        ))
        .unwrap();
    let perp_order = perp_step.decisions[0].order.as_ref().unwrap();
    assert_eq!(perp_order.symbol, "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP");
    assert_eq!(perp_order.side, trader_core::OrderSide::Sell);

    let mut blocked_engine = AlgorithmEngine::new_with_universe(
        AlgorithmEngineSettings {
            run_id: "run-mixed-equity-short".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::from([
                "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string()
            ]),
            initial_cash: dec!(100000),
        },
        Box::new(StaticUniverseSelector::new(vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
        ])),
        Box::new(SymbolSellAlphaModel),
    );

    let error = blocked_engine
        .on_market_slice(MarketSlice::new(
            1,
            vec![SymbolBar::new("US:NASDAQ:AAPL:EQUITY", bar(1, dec!(100)))],
        ))
        .unwrap_err();
    assert!(error.to_string().contains("short selling is disabled"));
}

#[test]
fn algorithm_engine_rejects_derivative_target_above_margin_limit() {
    let symbol = "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP";
    let mut engine = AlgorithmEngine::new_with_universe(
        AlgorithmEngineSettings {
            run_id: "run-margin".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: symbol.to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(9),
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::from([symbol.to_string()]),
            initial_cash: dec!(100000),
        },
        Box::new(StaticUniverseSelector::new(vec![symbol.to_string()])),
        Box::new(SymbolSellAlphaModel),
    );

    let error = engine
        .on_market_slice(MarketSlice::new(
            1,
            vec![SymbolBar::new(symbol, bar(1, dec!(100)))],
        ))
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("portfolio margin exceeds max margin")
    );
}

#[test]
fn algorithm_engine_rejects_contract_order_exceeding_max_leverage() {
    let symbol = "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP";
    let mut engine = AlgorithmEngine::new_with_universe(
        AlgorithmEngineSettings {
            run_id: "run-contract-leverage".to_string(),
            mode: StrategyRuntimeMode::Backtest,
            account_id: "backtest".to_string(),
            symbol: symbol.to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(126),
            max_margin_used: dec!(1000000),
            trading_halted: false,
            allow_short: true,
            shortable_symbols: BTreeSet::from([symbol.to_string()]),
            initial_cash: dec!(100000),
        },
        Box::new(StaticUniverseSelector::new(vec![symbol.to_string()])),
        Box::new(SymbolEchoAlphaModel),
    );

    let error = engine
        .on_market_slice(MarketSlice::new(
            1,
            vec![SymbolBar::new(symbol, bar(1, dec!(100)))],
        ))
        .unwrap_err();

    assert!(error.to_string().contains("contract leverage exceeds max"));
}

#[test]
fn algorithm_engine_generates_orders_for_each_selected_symbol_in_market_slice() {
    let mut engine = AlgorithmEngine::new_with_universe(
        AlgorithmEngineSettings {
            run_id: "run-multi".to_string(),
            mode: StrategyRuntimeMode::Paper,
            account_id: "paper".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::new(),
            initial_cash: dec!(100000),
        },
        Box::new(StaticUniverseSelector::new(vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ])),
        Box::new(SymbolEchoAlphaModel),
    );

    let step = engine
        .on_market_slice(market_slice(1, dec!(20), dec!(50)))
        .unwrap();

    let symbols = step
        .decisions
        .iter()
        .map(|decision| decision.order.as_ref().unwrap().symbol.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        symbols,
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
    assert_eq!(step.snapshot.positions.len(), 0);
    assert_eq!(step.decisions[0].order_number, 1);
    assert_eq!(step.decisions[1].order_number, 2);
}

#[test]
fn algorithm_engine_publishes_runtime_events_with_run_id_source() {
    let event_bus = EventBus::new(16);
    let mut receiver = event_bus.subscribe();
    let strategy = CompositeAlphaModel::new(vec![Box::new(FixedAlphaModel::new(
        "source",
        SignalSide::Buy,
        0.9,
    ))]);
    let mut engine = AlgorithmEngine::new(
        AlgorithmEngineSettings {
            run_id: "run-source".to_string(),
            mode: StrategyRuntimeMode::Paper,
            account_id: "paper".to_string(),
            symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
            order_qty: dec!(1),
            max_abs_qty: dec!(100),
            max_order_qty: dec!(100),
            max_order_notional: dec!(1000000),
            min_cash_after_order: dec!(0),
            max_exposure: dec!(1000000),
            max_drawdown: dec!(1),
            max_leverage: dec!(10),
            max_margin_used: dec!(0),
            trading_halted: false,
            allow_short: false,
            shortable_symbols: BTreeSet::new(),
            initial_cash: dec!(100000),
        },
        Box::new(strategy),
    );
    engine.set_event_bus(event_bus);

    engine.on_bar(bar(1, dec!(100))).unwrap();

    let event = receiver.try_recv().unwrap();
    assert_eq!(event.source, "run-source");
}

#[test]
fn algorithm_payload_structs_serialize_stable_schema_fields() {
    let order = serde_json::to_value(AlgorithmOrderPayload {
        run_id: "run-schema".to_string(),
        broker_order_id: Some("broker-1".to_string()),
        account_id: "paper".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        order_type: "MARKET".to_string(),
        qty: "1".to_string(),
        filled_qty: "1".to_string(),
        status: "FILLED".to_string(),
    })
    .unwrap();
    assert_eq!(order["run_id"], serde_json::json!("run-schema"));
    assert_eq!(order["symbol"], serde_json::json!("US:NASDAQ:AAPL:EQUITY"));
    assert_eq!(order["status"], serde_json::json!("FILLED"));
    assert_eq!(order["filled_qty"], serde_json::json!("1"));
    assert_eq!(order["broker_order_id"], serde_json::json!("broker-1"));

    let accounting = serde_json::to_value(AccountingUpdatedPayload {
        run_id: "run-schema".to_string(),
        cash: "99999".to_string(),
        realized_pnl: "12.5".to_string(),
    })
    .unwrap();
    assert_eq!(accounting["cash"], serde_json::json!("99999"));
    assert_eq!(accounting["realized_pnl"], serde_json::json!("12.5"));

    let universe = serde_json::to_value(UniverseSelectedPayload {
        run_id: "run-schema".to_string(),
        mode: "Paper".to_string(),
        symbols: vec!["US:NASDAQ:AAPL:EQUITY".to_string()],
    })
    .unwrap();
    assert_eq!(universe["run_id"], serde_json::json!("run-schema"));

    let alpha = serde_json::to_value(AlphaGeneratedPayload {
        run_id: "run-schema".to_string(),
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: "BUY".to_string(),
        confidence: 0.9,
    })
    .unwrap();
    assert_eq!(alpha["symbol"], serde_json::json!("US:NASDAQ:AAPL:EQUITY"));
}

struct FixedAlphaModel {
    signal: SignalEvent,
}

struct SymbolEchoAlphaModel;
struct SymbolSellAlphaModel;

impl AlphaModel for SymbolEchoAlphaModel {
    fn on_bar(&mut self, _bar: &Bar) -> Option<SignalEvent> {
        None
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, _bar: &Bar) -> Option<SignalEvent> {
        Some(SignalEvent {
            strategy_id: "echo".to_string(),
            symbol: symbol.to_string(),
            side: SignalSide::Buy,
            confidence: 0.9,
            ts: chrono::Utc::now(),
        })
    }
}

impl AlphaModel for SymbolSellAlphaModel {
    fn on_bar(&mut self, _bar: &Bar) -> Option<SignalEvent> {
        None
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, _bar: &Bar) -> Option<SignalEvent> {
        Some(SignalEvent {
            strategy_id: "sell".to_string(),
            symbol: symbol.to_string(),
            side: SignalSide::Sell,
            confidence: 0.9,
            ts: chrono::Utc::now(),
        })
    }
}

impl FixedAlphaModel {
    fn new(strategy_id: &str, side: SignalSide, confidence: f64) -> Self {
        Self {
            signal: SignalEvent {
                strategy_id: strategy_id.to_string(),
                symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
                side,
                confidence,
                ts: chrono::Utc::now(),
            },
        }
    }
}

impl AlphaModel for FixedAlphaModel {
    fn on_bar(&mut self, _bar: &Bar) -> Option<SignalEvent> {
        Some(self.signal.clone())
    }
}

fn contract_fill(
    side: OrderSide,
    qty: rust_decimal::Decimal,
    price: rust_decimal::Decimal,
    ts_ms: i64,
) -> ContractFill {
    ContractFill {
        run_id: "run-contract-accounting".to_string(),
        account_id: "paper".to_string(),
        exchange: "BINANCE".to_string(),
        symbol: "CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP".to_string(),
        asset_class: "CRYPTO_PERP".to_string(),
        margin_mode: "cross".to_string(),
        side,
        qty,
        price,
        fee: dec!(0),
        ts_ms,
    }
}

fn bar(ts_ms: i64, close: rust_decimal::Decimal) -> Bar {
    Bar::new(ts_ms, close, close, close, close, dec!(1))
}

fn market_slice(
    ts_ms: i64,
    aapl_close: rust_decimal::Decimal,
    msft_close: rust_decimal::Decimal,
) -> MarketSlice {
    MarketSlice::new(
        ts_ms,
        vec![
            SymbolBar::new("US:NASDAQ:AAPL:EQUITY", bar(ts_ms, aapl_close)),
            SymbolBar::new("US:NASDAQ:MSFT:EQUITY", bar(ts_ms, msft_close)),
        ],
    )
}
