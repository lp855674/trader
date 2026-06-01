use data::load_bars_from_csv;
use rust_decimal_macros::dec;

#[test]
fn loads_sample_bars_from_csv() {
    let bars = load_bars_from_csv("../../datasets/sample/aapl_1d.csv").unwrap();

    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].ts_ms, 1704067200000);
    assert_eq!(bars[0].close, dec!(108.00));
}
