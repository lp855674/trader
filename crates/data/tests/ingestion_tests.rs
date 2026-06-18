use data::ingestion::binance_meta::parse_binance_market_meta;

#[test]
fn ingestion_parses_binance_exchange_info_into_market_meta() {
    let payload = r#"
    {
      "symbols": [
        {
          "symbol": "BTCUSDT",
          "status": "TRADING",
          "baseAsset": "BTC",
          "quoteAsset": "USDT",
          "baseAssetPrecision": 8,
          "quoteAssetPrecision": 8,
          "filters": [
            { "filterType": "PRICE_FILTER", "tickSize": "0.01000000" },
            { "filterType": "LOT_SIZE", "minQty": "0.00001000", "maxQty": "9000.00000000", "stepSize": "0.00001000" },
            { "filterType": "MIN_NOTIONAL", "minNotional": "5.00000000" }
          ]
        }
      ]
    }
    "#;

    let records = parse_binance_market_meta(payload, 42).unwrap();

    assert_eq!(records.len(), 1);
    let meta = &records[0];
    assert_eq!(meta.exchange, "BINANCE");
    assert_eq!(meta.symbol, "BTCUSDT");
    assert_eq!(meta.base_asset, "BTC");
    assert_eq!(meta.quote_asset, "USDT");
    assert_eq!(meta.instrument_type, "SPOT");
    assert_eq!(meta.price_tick.as_deref(), Some("0.01000000"));
    assert_eq!(meta.qty_step.as_deref(), Some("0.00001000"));
    assert_eq!(meta.min_qty.as_deref(), Some("0.00001000"));
    assert_eq!(meta.max_qty.as_deref(), Some("9000.00000000"));
    assert_eq!(meta.min_notional.as_deref(), Some("5.00000000"));
    assert_eq!(meta.qty_precision, Some(8));
    assert!(meta.is_active);
    assert_eq!(meta.created_at_ms, 42);
    assert_eq!(meta.updated_at_ms, 42);
}
