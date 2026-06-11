use data::Bar;
use rust_decimal_macros::dec;
use universe::{StaticUniverseSelector, UniverseContext, UniverseError, UniverseSelector};

#[test]
fn static_universe_selector_returns_configured_symbols() {
    let selector = StaticUniverseSelector::new(vec![
        "US:NASDAQ:AAPL:EQUITY".to_string(),
        "US:NASDAQ:MSFT:EQUITY".to_string(),
    ]);
    let context = UniverseContext {
        primary_symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        bar: bar(),
    };

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
    let context = UniverseContext {
        primary_symbol: " ".to_string(),
        bar: bar(),
    };

    assert_eq!(selector.select(&context), Err(UniverseError::EmptySymbol));
}

fn bar() -> Bar {
    Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1))
}
