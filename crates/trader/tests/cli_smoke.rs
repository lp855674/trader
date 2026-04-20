use clap::Parser;
use terminal_core::models::{OrderActionResult, OrderRow, QuoteView};

#[test]
fn submit_command_parses_limit_order_arguments() {
    let cli = trader::cli::Cli::parse_from([
        "trader",
        "order",
        "submit",
        "--account-id",
        "acc_mvp_paper",
        "--symbol",
        "AAPL.US",
        "--side",
        "buy",
        "--qty",
        "10",
        "--limit-price",
        "123.45",
    ]);

    match cli.command {
        trader::cli::Command::Order { action } => match action {
            trader::cli::OrderCommand::Submit(body) => assert_eq!(body.symbol, "AAPL.US"),
            other => panic!("unexpected order command: {other:?}"),
        },
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn render_order_action_supports_json_output() {
    let rendered = trader::output::render_order_action(
        &OrderActionResult {
            order_id: "ord-1".to_string(),
            status: "SUBMITTED".to_string(),
        },
        true,
    )
    .expect("render");
    assert!(rendered.contains("\"order_id\": \"ord-1\""));
}

#[test]
fn render_quote_supports_table_output() {
    let rendered = trader::output::render_quote(
        &QuoteView {
            symbol: "AAPL.US".to_string(),
            venue: "US_EQUITY".to_string(),
            last_price: Some(124.0),
            day_high: Some(126.0),
            day_low: Some(119.5),
            bars: Vec::new(),
        },
        false,
    )
    .expect("render");
    assert!(rendered.contains("AAPL.US"));
    assert!(rendered.contains("US_EQUITY"));
}

#[test]
fn render_orders_supports_table_output() {
    let rendered = trader::output::render_orders(
        &[OrderRow {
            order_id: "ord-1".to_string(),
            venue: "US_EQUITY".to_string(),
            symbol: "AAPL.US".to_string(),
            side: "buy".to_string(),
            qty: 10.0,
            status: "SUBMITTED".to_string(),
            order_type: "limit".to_string(),
            limit_price: Some(123.45),
            exchange_ref: Some("lb-1".to_string()),
            created_at_ms: 1000,
            updated_at_ms: 1001,
        }],
        false,
    )
    .expect("render");
    assert!(rendered.contains("ord-1"));
    assert!(rendered.contains("AAPL.US"));
    assert!(rendered.contains("SUBMITTED"));
}
