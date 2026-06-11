use data::{Bar, BarInput, MarketSlice, SymbolBar, load_market_slices};
use rust_decimal_macros::dec;

#[test]
fn bar_return_uses_close_to_close() {
    let previous = Bar::new(1, dec!(100), dec!(110), dec!(90), dec!(100), dec!(1000));
    let current = Bar::new(2, dec!(100), dec!(115), dec!(95), dec!(110), dec!(1200));

    assert_eq!(current.close_return(&previous), dec!(0.1));
}

#[test]
fn market_slice_returns_bars_by_symbol_in_stable_order() {
    let slice = MarketSlice::new(
        3,
        vec![
            SymbolBar::new("US:NASDAQ:MSFT:EQUITY", bar_with_close(3, dec!(50))),
            SymbolBar::new("US:NASDAQ:AAPL:EQUITY", bar_with_close(3, dec!(20))),
        ],
    );

    assert_eq!(slice.ts_ms, 3);
    assert_eq!(
        slice.symbols(),
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
    assert_eq!(slice.bar("US:NASDAQ:MSFT:EQUITY").unwrap().close, dec!(50));
}

#[test]
fn load_market_slices_aligns_symbol_inputs_by_timestamp() {
    let aapl_path = temp_csv_path("aapl");
    let msft_path = temp_csv_path("msft");
    std::fs::write(
        &aapl_path,
        "ts_ms,open,high,low,close,volume\n1,10,10,10,10,1\n2,11,11,11,11,1\n",
    )
    .unwrap();
    std::fs::write(
        &msft_path,
        "ts_ms,open,high,low,close,volume\n1,30,30,30,30,1\n2,31,31,31,31,1\n",
    )
    .unwrap();

    let slices = load_market_slices(&[
        BarInput::new(
            "US:NASDAQ:AAPL:EQUITY",
            "csv",
            aapl_path.to_string_lossy().into_owned(),
        ),
        BarInput::new(
            "US:NASDAQ:MSFT:EQUITY",
            "csv",
            msft_path.to_string_lossy().into_owned(),
        ),
    ])
    .unwrap();

    assert_eq!(slices.len(), 2);
    assert_eq!(slices[0].ts_ms, 1);
    assert_eq!(
        slices[0].symbols(),
        vec![
            "US:NASDAQ:AAPL:EQUITY".to_string(),
            "US:NASDAQ:MSFT:EQUITY".to_string()
        ]
    );
    assert_eq!(
        slices[0].bar("US:NASDAQ:AAPL:EQUITY").unwrap().close,
        dec!(10)
    );
    assert_eq!(
        slices[0].bar("US:NASDAQ:MSFT:EQUITY").unwrap().close,
        dec!(30)
    );
    assert_eq!(
        slices[1].bar("US:NASDAQ:AAPL:EQUITY").unwrap().close,
        dec!(11)
    );
    assert_eq!(
        slices[1].bar("US:NASDAQ:MSFT:EQUITY").unwrap().close,
        dec!(31)
    );
}

fn bar_with_close(ts_ms: i64, close: rust_decimal::Decimal) -> Bar {
    Bar::new(ts_ms, close, close, close, close, dec!(1))
}

fn temp_csv_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "trader-data-{name}-{}.csv",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
