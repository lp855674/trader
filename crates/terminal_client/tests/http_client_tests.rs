#[tokio::test]
async fn http_client_maps_api_error_codes_into_terminal_errors() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/v1/orders")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error_code":"risk_denied","message":"observe_only"}"#)
        .create_async()
        .await;

    let client = terminal_client::QuantdHttpClient::new(server.url(), None);
    let err = client
        .submit_order(&terminal_core::models::SubmitOrderRequest {
            account_id: "acc_mvp_paper".to_string(),
            symbol: "AAPL.US".to_string(),
            side: "buy".to_string(),
            qty: 10.0,
            order_type: "limit".to_string(),
            limit_price: Some(123.45),
        })
        .await
        .expect_err("error");

    assert_eq!(err.code(), "risk_denied");
}

#[tokio::test]
async fn http_client_loads_order_rows() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/v1/orders?account_id=acc_mvp_paper")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[{"order_id":"ord-1","venue":"US_EQUITY","symbol":"AAPL.US","side":"buy","qty":10.0,"status":"SUBMITTED","order_type":"limit","limit_price":123.45,"exchange_ref":"paper-ord-1","created_at_ms":1000,"updated_at_ms":1001}]"#,
        )
        .create_async()
        .await;

    let client = terminal_client::QuantdHttpClient::new(server.url(), None);
    let rows = client
        .get_orders("acc_mvp_paper")
        .await
        .expect("rows");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id, "ord-1");
    assert_eq!(rows[0].symbol, "AAPL.US");
    assert_eq!(rows[0].status, "SUBMITTED");
}
