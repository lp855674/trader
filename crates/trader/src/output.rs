use comfy_table::Table;
use terminal_core::models::{OrderActionResult, OrderRow, QuoteView};

pub fn render_order_action(result: &OrderActionResult, json: bool) -> Result<String, String> {
    if json {
        serde_json::to_string_pretty(result).map_err(|error| error.to_string())
    } else {
        let mut table = Table::new();
        table.set_header(vec!["order_id", "status"]);
        table.add_row(vec![result.order_id.as_str(), result.status.as_str()]);
        Ok(table.to_string())
    }
}

pub fn render_quote(quote: &QuoteView, json: bool) -> Result<String, String> {
    if json {
        serde_json::to_string_pretty(quote).map_err(|error| error.to_string())
    } else {
        let mut table = Table::new();
        table.set_header(vec!["symbol", "venue", "last_price", "day_high", "day_low"]);
        table.add_row(vec![
            quote.symbol.as_str(),
            quote.venue.as_str(),
            &format_option_f64(quote.last_price),
            &format_option_f64(quote.day_high),
            &format_option_f64(quote.day_low),
        ]);
        Ok(table.to_string())
    }
}

pub fn render_orders(orders: &[OrderRow], json: bool) -> Result<String, String> {
    if json {
        serde_json::to_string_pretty(orders).map_err(|error| error.to_string())
    } else {
        let mut table = Table::new();
        table.set_header(vec![
            "order_id",
            "symbol",
            "side",
            "qty",
            "type",
            "limit_price",
            "status",
        ]);
        for order in orders {
            table.add_row(vec![
                order.order_id.as_str(),
                order.symbol.as_str(),
                order.side.as_str(),
                &format!("{:.4}", order.qty),
                order.order_type.as_str(),
                &format_option_f64(order.limit_price),
                order.status.as_str(),
            ]);
        }
        Ok(table.to_string())
    }
}

fn format_option_f64(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.4}"))
        .unwrap_or_else(|| "-".to_string())
}
