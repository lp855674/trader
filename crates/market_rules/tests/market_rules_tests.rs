use market_rules::{
    ConfiguredMarketRuleProvider, ContractRiskError, ContractRiskLimits, FeeRule, FeeRuleEngine,
    FeeTier, FeeVolumeEntry, FeeVolumeWindow, MarketRuleError, MarketRuleProvider, MarketRuleSet,
    StaticMarketRuleProvider,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::BTreeMap;
use trader_core::{OrderRequest, OrderSide, OrderType};

#[test]
fn fee_volume_window_parses_supported_values_and_defaults_to_run() {
    assert_eq!(
        "run".parse::<FeeVolumeWindow>().unwrap(),
        FeeVolumeWindow::Run
    );
    assert_eq!(
        "rolling_30d".parse::<FeeVolumeWindow>().unwrap(),
        FeeVolumeWindow::Rolling30d
    );
    assert_eq!(
        "calendar_month".parse::<FeeVolumeWindow>().unwrap(),
        FeeVolumeWindow::CalendarMonth
    );
    assert!("daily".parse::<FeeVolumeWindow>().is_err());
    assert_eq!(
        FeeRule::flat("fee-test", Decimal::ONE, Decimal::ONE).volume_window,
        FeeVolumeWindow::Run
    );
}

#[test]
fn rejects_quantity_below_lot_size() {
    let rules = MarketRuleSet::us_equity();
    let order = market_order(Decimal::new(5, 1));

    assert_eq!(
        rules
            .validate_order(&order, Decimal::from(100))
            .unwrap_err(),
        MarketRuleError::InvalidLotSize
    );
}

#[test]
fn rejects_limit_price_off_tick_size() {
    let rules = MarketRuleSet::us_equity();
    let mut order = market_order(Decimal::ONE);
    order.order_type = OrderType::Limit;
    order.price = Some(Decimal::new(100_001, 3));

    assert_eq!(
        rules
            .validate_order(&order, Decimal::from(100))
            .unwrap_err(),
        MarketRuleError::InvalidTickSize
    );
}

#[test]
fn rejects_notional_below_minimum() {
    let rules = MarketRuleSet {
        lot_size: Decimal::ONE,
        tick_size: Decimal::new(1, 2),
        min_qty: Decimal::ONE,
        min_notional: Decimal::from(100),
        allow_market_orders: true,
        initial_margin_rate: Decimal::ZERO,
    };
    let order = market_order(Decimal::ONE);

    assert_eq!(
        rules.validate_order(&order, Decimal::from(50)).unwrap_err(),
        MarketRuleError::MinNotional
    );
}

#[test]
fn accepts_valid_us_equity_market_order() {
    let rules = MarketRuleSet::us_equity();
    rules
        .validate_order(&market_order(Decimal::ONE), Decimal::from(100))
        .unwrap();
}

#[test]
fn selects_cn_equity_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CN:SSE:600000:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::from(100));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
}

#[test]
fn selects_hk_equity_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("HK:HKEX:00700:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::from(100));
    assert_eq!(rules.tick_size, Decimal::new(1, 3));
}

#[test]
fn selects_us_equity_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("US:NASDAQ:AAPL:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::ONE);
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
}

#[test]
fn static_market_rule_provider_uses_existing_symbol_rules() {
    let provider = StaticMarketRuleProvider;
    let rules = provider.rules_for_symbol("US:NASDAQ:AAPL:EQUITY").unwrap();

    assert_eq!(rules.lot_size, Decimal::ONE);
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
}

#[test]
fn configured_market_rule_provider_overrides_exact_symbol_and_falls_back() {
    let mut configured_rules = BTreeMap::new();
    configured_rules.insert(
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        MarketRuleSet {
            lot_size: dec!(10),
            tick_size: dec!(0.05),
            min_qty: dec!(10),
            min_notional: dec!(100),
            allow_market_orders: false,
            initial_margin_rate: Decimal::ZERO,
        },
    );
    let provider = ConfiguredMarketRuleProvider::new(configured_rules);

    let overridden = provider.rules_for_symbol("US:NASDAQ:AAPL:EQUITY").unwrap();
    assert_eq!(overridden.lot_size, dec!(10));
    assert_eq!(overridden.tick_size, dec!(0.05));
    assert!(!overridden.allow_market_orders);

    let fallback = provider.rules_for_symbol("US:NASDAQ:MSFT:EQUITY").unwrap();
    assert_eq!(fallback, MarketRuleSet::us_equity());
}

#[test]
fn fee_rule_selects_maker_or_taker_bps_by_order_type() {
    let rule = FeeRule::flat("fee-test", Decimal::ONE, Decimal::from(25));

    assert_eq!(
        rule.maker_taker_fee_bps(OrderType::Market, Decimal::ZERO),
        Decimal::from(25)
    );
    assert_eq!(
        rule.maker_taker_fee_bps(OrderType::Stop, Decimal::ZERO),
        Decimal::from(25)
    );
    assert_eq!(
        rule.maker_taker_fee_bps(OrderType::Limit, Decimal::ZERO),
        Decimal::ONE
    );
    assert_eq!(
        rule.maker_taker_fee_bps(OrderType::StopLimit, Decimal::ZERO),
        Decimal::ONE
    );
    assert_eq!(
        rule.maker_taker_fee_bps(OrderType::PostOnly, Decimal::ZERO),
        Decimal::ONE
    );
}

#[test]
fn fee_rule_applies_minimum_fee_floor() {
    let mut rule = FeeRule::flat("fee-test", Decimal::ONE, Decimal::from(25));
    rule.minimum_fee = Some(Decimal::new(1, 2));

    assert_eq!(
        rule.fee(
            OrderType::Market,
            Decimal::from(20),
            Decimal::ONE,
            Decimal::ZERO
        ),
        Decimal::new(5, 2)
    );
    assert_eq!(
        rule.fee(
            OrderType::Limit,
            Decimal::from(20),
            Decimal::ONE,
            Decimal::ZERO
        ),
        Decimal::new(1, 2)
    );
}

#[test]
fn fee_rule_returns_structured_breakdown_with_minimum_adjustment() {
    let mut rule = FeeRule::flat("fee-test", Decimal::ONE, Decimal::from(25));
    rule.minimum_fee = Some(dec!(0.05));
    rule.tax_bps = Some(dec!(2));
    rule.exchange_fee_bps = Some(dec!(3));

    let breakdown = rule.fee_breakdown(OrderType::Limit, dec!(20), Decimal::ONE, Decimal::ZERO);

    assert_eq!(breakdown.commission, dec!(0.002));
    assert_eq!(breakdown.tax, dec!(0.004));
    assert_eq!(breakdown.exchange_fee, dec!(0.006));
    assert_eq!(breakdown.minimum_fee_adjustment, dec!(0.038));
    assert_eq!(breakdown.total, dec!(0.05));
}

#[test]
fn fee_rule_engine_advances_tiers_from_accumulated_fill_notional() {
    let mut rule = FeeRule::flat("fee-tiered", Decimal::from(99), Decimal::from(99));
    rule.tiers = vec![
        FeeTier {
            volume_from: Decimal::ZERO,
            volume_to: Some(dec!(10)),
            maker_bps: Decimal::ONE,
            taker_bps: dec!(10),
        },
        FeeTier {
            volume_from: dec!(10),
            volume_to: None,
            maker_bps: dec!(0.5),
            taker_bps: dec!(2),
        },
    ];
    let mut rules_by_symbol = BTreeMap::new();
    rules_by_symbol.insert("US:NASDAQ:AAPL:EQUITY".to_string(), rule);
    let mut engine = FeeRuleEngine::new(rules_by_symbol);

    let first = engine
        .apply_fill(
            "US:NASDAQ:AAPL:EQUITY",
            OrderType::Market,
            dec!(10),
            Decimal::ONE,
        )
        .unwrap();
    let second = engine
        .apply_fill(
            "US:NASDAQ:AAPL:EQUITY",
            OrderType::Market,
            dec!(10),
            Decimal::ONE,
        )
        .unwrap();

    assert_eq!(first.total, dec!(0.01));
    assert_eq!(second.total, dec!(0.002));
    assert_eq!(engine.volume_for_rule("fee-tiered"), dec!(20));
}

#[test]
fn fee_rule_engine_evicts_rolling_30d_volume_before_selecting_tier() {
    const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

    let mut rule = FeeRule::flat("fee-rolling", Decimal::from(99), Decimal::from(99));
    rule.volume_window = FeeVolumeWindow::Rolling30d;
    rule.tiers = vec![
        FeeTier {
            volume_from: Decimal::ZERO,
            volume_to: Some(dec!(100)),
            maker_bps: Decimal::ONE,
            taker_bps: dec!(10),
        },
        FeeTier {
            volume_from: dec!(100),
            volume_to: None,
            maker_bps: dec!(0.5),
            taker_bps: Decimal::ONE,
        },
    ];
    let mut rules_by_symbol = BTreeMap::new();
    rules_by_symbol.insert("US:NASDAQ:AAPL:EQUITY".to_string(), rule);
    let mut seed_entries = BTreeMap::new();
    seed_entries.insert(
        "fee-rolling".to_string(),
        vec![
            FeeVolumeEntry {
                ts_ms: 0,
                notional: dec!(100),
            },
            FeeVolumeEntry {
                ts_ms: DAY_MS,
                notional: dec!(10),
            },
        ],
    );
    let mut engine = FeeRuleEngine::with_volume_entries_by_rule(rules_by_symbol, seed_entries);

    let breakdown = engine
        .apply_fill_at(
            "US:NASDAQ:AAPL:EQUITY",
            OrderType::Market,
            dec!(10),
            Decimal::ONE,
            30 * DAY_MS + 1,
        )
        .unwrap();

    assert_eq!(breakdown.total, dec!(0.01));
    assert_eq!(engine.volume_for_rule("fee-rolling"), dec!(20));
}

#[test]
fn fee_rule_engine_evicts_seed_and_runtime_rolling_volume() {
    const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

    let mut rule = FeeRule::flat("fee-rolling-runtime", Decimal::from(99), Decimal::from(99));
    rule.volume_window = FeeVolumeWindow::Rolling30d;
    rule.tiers = vec![
        FeeTier {
            volume_from: Decimal::ZERO,
            volume_to: Some(dec!(20)),
            maker_bps: Decimal::ONE,
            taker_bps: dec!(10),
        },
        FeeTier {
            volume_from: dec!(20),
            volume_to: None,
            maker_bps: dec!(0.5),
            taker_bps: dec!(2),
        },
    ];
    let mut rules_by_symbol = BTreeMap::new();
    rules_by_symbol.insert("US:NASDAQ:AAPL:EQUITY".to_string(), rule);

    let start_ms = 1_700_000_000_000;
    let mut seed_entries = BTreeMap::new();
    seed_entries.insert(
        "fee-rolling-runtime".to_string(),
        vec![FeeVolumeEntry {
            ts_ms: start_ms - DAY_MS,
            notional: dec!(20),
        }],
    );
    let mut engine = FeeRuleEngine::with_volume_entries_by_rule(rules_by_symbol, seed_entries);

    let first = engine
        .apply_fill_at(
            "US:NASDAQ:AAPL:EQUITY",
            OrderType::Market,
            dec!(20),
            Decimal::ONE,
            start_ms + 2,
        )
        .unwrap();
    assert_eq!(first.total, dec!(0.004));

    let second = engine
        .apply_fill_at(
            "US:NASDAQ:AAPL:EQUITY",
            OrderType::Market,
            Decimal::ONE,
            dec!(2),
            start_ms + 31 * DAY_MS,
        )
        .unwrap();

    assert_eq!(second.total, dec!(0.002));
    assert_eq!(engine.volume_for_rule("fee-rolling-runtime"), dec!(2));
}

#[test]
fn fee_rule_engine_resets_calendar_month_volume_before_selecting_tier() {
    let mut rule = FeeRule::flat("fee-calendar", Decimal::from(99), Decimal::from(99));
    rule.volume_window = FeeVolumeWindow::CalendarMonth;
    rule.tiers = vec![
        FeeTier {
            volume_from: Decimal::ZERO,
            volume_to: Some(dec!(100)),
            maker_bps: Decimal::ONE,
            taker_bps: dec!(10),
        },
        FeeTier {
            volume_from: dec!(100),
            volume_to: None,
            maker_bps: dec!(0.5),
            taker_bps: Decimal::ONE,
        },
    ];
    let mut rules_by_symbol = BTreeMap::new();
    rules_by_symbol.insert("US:NASDAQ:AAPL:EQUITY".to_string(), rule);
    let mut seed_entries = BTreeMap::new();
    seed_entries.insert(
        "fee-calendar".to_string(),
        vec![
            FeeVolumeEntry {
                ts_ms: 1_703_980_800_000,
                notional: dec!(100),
            },
            FeeVolumeEntry {
                ts_ms: 1_704_153_600_000,
                notional: dec!(10),
            },
        ],
    );
    let mut engine = FeeRuleEngine::with_volume_entries_by_rule(rules_by_symbol, seed_entries);

    let breakdown = engine
        .apply_fill_at(
            "US:NASDAQ:AAPL:EQUITY",
            OrderType::Market,
            dec!(10),
            Decimal::ONE,
            1_707_868_800_000,
        )
        .unwrap();

    assert_eq!(breakdown.total, dec!(0.01));
    assert_eq!(engine.volume_for_rule("fee-calendar"), dec!(10));
}

#[test]
fn fee_rule_adds_tax_and_exchange_fee_before_minimum_floor() {
    let mut rule = FeeRule::flat("fee-test", Decimal::ONE, Decimal::from(25));
    rule.minimum_fee = Some(Decimal::new(1, 2));
    rule.tax_bps = Some(Decimal::from(5));
    rule.exchange_fee_bps = Some(Decimal::from(2));

    assert_eq!(
        rule.fee(
            OrderType::Market,
            Decimal::from(20),
            Decimal::ONE,
            Decimal::ZERO
        ),
        Decimal::new(64, 3)
    );
    assert_eq!(
        rule.fee(
            OrderType::Limit,
            Decimal::from(20),
            Decimal::ONE,
            Decimal::ZERO
        ),
        Decimal::new(16, 3)
    );
}

#[test]
fn fee_rule_selects_tier_by_volume_boundary() {
    let mut rule = FeeRule::flat("fee-test", Decimal::from(10), Decimal::from(20));
    rule.tax_bps = Some(Decimal::from(2));
    rule.exchange_fee_bps = Some(Decimal::from(3));
    rule.tiers = vec![
        FeeTier {
            volume_from: Decimal::ZERO,
            volume_to: Some(Decimal::from(1000)),
            maker_bps: Decimal::from(4),
            taker_bps: Decimal::from(8),
        },
        FeeTier {
            volume_from: Decimal::from(1000),
            volume_to: None,
            maker_bps: Decimal::ONE,
            taker_bps: Decimal::from(2),
        },
    ];

    assert_eq!(
        rule.total_fee_bps(OrderType::Market, Decimal::from(999)),
        Decimal::from(13)
    );
    assert_eq!(
        rule.total_fee_bps(OrderType::Market, Decimal::from(1000)),
        Decimal::from(7)
    );
    assert_eq!(
        rule.total_fee_bps(OrderType::Limit, Decimal::from(1000)),
        Decimal::from(6)
    );
}

#[test]
fn fee_rule_applies_tier_tax_exchange_and_minimum_floor() {
    let mut rule = FeeRule::flat("fee-test", Decimal::from(50), Decimal::from(60));
    rule.minimum_fee = Some(Decimal::new(5, 2));
    rule.tax_bps = Some(Decimal::from(2));
    rule.exchange_fee_bps = Some(Decimal::from(3));
    rule.tiers = vec![FeeTier {
        volume_from: Decimal::ZERO,
        volume_to: None,
        maker_bps: Decimal::ONE,
        taker_bps: Decimal::from(7),
    }];

    assert_eq!(
        rule.fee(
            OrderType::Market,
            Decimal::from(20),
            Decimal::ONE,
            Decimal::ZERO
        ),
        Decimal::new(5, 2)
    );
    assert_eq!(
        rule.fee(
            OrderType::Limit,
            Decimal::from(100),
            Decimal::from(2),
            Decimal::ZERO
        ),
        Decimal::new(12, 2)
    );
}

#[test]
fn selects_crypto_spot_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT").unwrap();

    assert_eq!(rules.lot_size, Decimal::new(1, 6));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
    assert_eq!(rules.min_notional, Decimal::from(10));
}

#[test]
fn selects_crypto_perp_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CRYPTO:BINANCE:BTCUSDT_PERP:CRYPTO_PERP").unwrap();

    assert_eq!(rules.lot_size, Decimal::new(1, 3));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
    assert_eq!(rules.min_notional, Decimal::from(5));
    assert_eq!(rules.initial_margin_rate, Decimal::new(1, 1));
}

#[test]
fn selects_crypto_future_rules_from_symbol() {
    let rules = MarketRuleSet::for_symbol("CRYPTO:BINANCE:BTCUSDT_240628:CRYPTO_FUTURE").unwrap();

    assert_eq!(rules.lot_size, Decimal::new(1, 3));
    assert_eq!(rules.tick_size, Decimal::new(1, 2));
    assert_eq!(rules.min_notional, Decimal::from(5));
    assert_eq!(rules.initial_margin_rate, Decimal::new(1, 1));
}

#[test]
fn contract_risk_limits_reject_excessive_leverage() {
    let limits = ContractRiskLimits::crypto_perp();

    assert_eq!(
        limits
            .validate(dec!(126), dec!(1000), dec!(2), dec!(200), dec!(0.0001))
            .unwrap_err(),
        ContractRiskError::MaxLeverage
    );
}

#[test]
fn contract_risk_limits_reject_insufficient_margin_ratio() {
    let limits = ContractRiskLimits::crypto_perp();

    assert_eq!(
        limits
            .validate(dec!(10), dec!(1000), dec!(1.01), dec!(200), dec!(0.0001))
            .unwrap_err(),
        ContractRiskError::InsufficientMargin
    );
}

#[test]
fn contract_risk_limits_reject_funding_rate_out_of_bounds() {
    let limits = ContractRiskLimits::crypto_perp();

    assert_eq!(
        limits
            .validate(dec!(10), dec!(1000), dec!(2), dec!(200), dec!(0.02))
            .unwrap_err(),
        ContractRiskError::FundingRateBounds
    );
}

#[test]
fn rejects_unknown_symbol_rule_set() {
    let error = MarketRuleSet::for_symbol("US:NASDAQ:AAPL:OPTION").unwrap_err();

    assert_eq!(
        error,
        MarketRuleError::UnsupportedSymbol("US:NASDAQ:AAPL:OPTION".to_string())
    );
}

fn market_order(qty: Decimal) -> OrderRequest {
    OrderRequest {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        qty,
        price: None,
        account_id: "paper".to_string(),
    }
}
