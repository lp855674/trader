use data::{Bar, load_bars, load_bars_from_parquet, write_bars_to_parquet};
use rust_decimal_macros::dec;

#[test]
fn parquet_bar_round_trip_preserves_ohlcv_values() {
    let path = std::env::temp_dir().join(format!(
        "trader-data-parquet-{}.parquet",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bars = vec![
        Bar::new(
            1704067200000,
            dec!(100.10),
            dec!(110.20),
            dec!(99.90),
            dec!(108.00),
            dec!(1000.5),
        ),
        Bar::new(
            1704153600000,
            dec!(108.00),
            dec!(112.00),
            dec!(107.50),
            dec!(111.25),
            dec!(1200.75),
        ),
    ];

    write_bars_to_parquet(&path, &bars).unwrap();
    let loaded = load_bars_from_parquet(&path).unwrap();

    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, bars);
}

#[test]
fn load_bars_dispatches_parquet_source() {
    let path = std::env::temp_dir().join(format!(
        "trader-data-dispatch-{}.parquet",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bars = vec![Bar::new(
        1704067200000,
        dec!(100.10),
        dec!(110.20),
        dec!(99.90),
        dec!(108.00),
        dec!(1000.5),
    )];
    write_bars_to_parquet(&path, &bars).unwrap();

    let loaded = load_bars("parquet", &path).unwrap();

    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, bars);
}
