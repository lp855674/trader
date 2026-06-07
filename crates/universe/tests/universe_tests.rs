use data::Bar;
use rust_decimal_macros::dec;
use universe::{StaticUniverseSelector, UniverseContext, UniverseSelector};

#[test]
fn static_universe_selects_configured_symbol_for_matching_bar() {
    let selector = StaticUniverseSelector::new(vec!["US:NASDAQ:AAPL:EQUITY".to_string()]);
    let context = UniverseContext {
        primary_symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        bar: Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
    };

    let selected = selector.select(&context).unwrap();

    assert_eq!(selected, vec!["US:NASDAQ:AAPL:EQUITY"]);
}

#[test]
fn static_universe_filters_out_unconfigured_symbol() {
    let selector = StaticUniverseSelector::new(vec!["US:NASDAQ:MSFT:EQUITY".to_string()]);
    let context = UniverseContext {
        primary_symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        bar: Bar::new(1, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
    };

    let selected = selector.select(&context).unwrap();

    assert!(selected.is_empty());
}
