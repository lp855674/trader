use data::Bar;
use rust_decimal_macros::dec;
use universe::{
    FilteredUniverseSelector, RankedUniverseSelector, StaticUniverseSelector, UniverseContext,
    UniverseError, UniverseFilter, UniverseSelector,
};

#[test]
fn static_universe_selector_returns_configured_symbols() {
    let selector = StaticUniverseSelector::new(vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ]);
    let context = UniverseContext::new("US:NASDAQ:AAPL:EQUITY", bar());

    assert_eq!(
        selector.select(&context).unwrap(),
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
}

#[test]
fn static_universe_selector_rejects_empty_primary_symbol() {
    let selector = StaticUniverseSelector::new(vec![]);
    let context = UniverseContext::new(" ", bar());

    assert_eq!(selector.select(&context), Err(UniverseError::EmptySymbol));
}

#[test]
fn filtered_universe_selector_applies_include_exclude_and_prefix_rules() {
    let selector = FilteredUniverseSelector::new(
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NYSE:IBM:EQUITY".to_string(),
            "CRYPTO:BINANCE:BTCUSDT:CRYPTO_SPOT".to_string(),
        ],
        UniverseFilter {
            include_symbols: vec![
                "US:NASDAQ:AAPL:EQUITY".to_string(),
                "US:NASDAQ:MSFT:EQUITY".to_string(),
                "US:NYSE:IBM:EQUITY".to_string(),
            ],
            exclude_symbols: vec!["US:NASDAQ:MSFT:EQUITY".to_string()],
            symbol_prefixes: vec!["US:NASDAQ:".to_string()],
            require_current_data: false,
            max_symbols: None,
        },
    );
    let context = UniverseContext::new("US:NASDAQ:AAPL:EQUITY", bar());

    assert_eq!(
        selector.select(&context).unwrap(),
        vec!["US:NASDAQ:AAPL:EQUITY".to_string()]
    );
}

#[test]
fn filtered_universe_selector_can_require_current_market_data() {
    let selector = FilteredUniverseSelector::new(
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
        ],
        UniverseFilter {
            require_current_data: true,
            ..UniverseFilter::default()
        },
    );
    let context = UniverseContext::new("US:NASDAQ:AAPL:EQUITY", bar())
        .with_available_symbols(vec!["US:NASDAQ:MSFT:EQUITY".to_string()]);

    assert_eq!(
        selector.select(&context).unwrap(),
        vec!["US:NASDAQ:MSFT:EQUITY".to_string()]
    );
}

#[test]
fn ranked_universe_selector_keeps_configured_rank_and_limits_symbols() {
    let selector = RankedUniverseSelector::new(
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NYSE:IBM:EQUITY".to_string(),
        ],
        UniverseFilter {
            require_current_data: true,
            max_symbols: Some(2),
            ..UniverseFilter::default()
        },
    );
    let context =
        UniverseContext::new("US:NASDAQ:AAPL:EQUITY", bar()).with_available_symbols(vec![
            "US:NYSE:IBM:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string(),
            "US:NASDAQ:AAPL:EQUITY".to_string(),
        ]);

    assert_eq!(
        selector.select(&context).unwrap(),
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
}

fn bar() -> Bar {
    Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1))
}
