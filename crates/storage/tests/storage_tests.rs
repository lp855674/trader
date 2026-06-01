use storage::{Db, NewInstrument};

#[tokio::test]
async fn instrument_round_trip() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    db.insert_instrument(NewInstrument {
        symbol: "US:NASDAQ:AAPL:EQUITY".to_string(),
        market: "US".to_string(),
        exchange: "NASDAQ".to_string(),
        asset_class: "EQUITY".to_string(),
        currency: "USD".to_string(),
        lot_size: "1".to_string(),
        tick_size: "0.01".to_string(),
        tradable: true,
    })
    .await
    .unwrap();

    let instrument = db
        .get_instrument("US:NASDAQ:AAPL:EQUITY")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(instrument.symbol, "US:NASDAQ:AAPL:EQUITY");
    assert!(instrument.tradable);
}
