#[test]
fn live_account_submit_requires_two_confirmation_steps() {
    let mut app = terminal_tui::app::AppState::new();
    app.active_account = "acc_lb_live".to_string();
    app.begin_submit("AAPL.US".to_string(), "buy".to_string());
    assert_eq!(
        app.confirmation_state,
        terminal_tui::app::ConfirmationState::Review
    );
    app.confirm_current_action();
    assert_eq!(
        app.confirmation_state,
        terminal_tui::app::ConfirmationState::Final
    );
}

#[test]
fn websocket_disconnect_marks_terminal_degraded() {
    let mut app = terminal_tui::app::AppState::new();
    app.handle_stream_disconnected();
    assert_eq!(
        app.connection_state,
        terminal_tui::app::ConnectionState::Reconnecting
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("stream disconnected; attempting reconnect")
    );
}

#[test]
fn overview_refresh_selects_first_watch_symbol_when_none_selected() {
    let mut app = terminal_tui::app::AppState::new();
    app.apply_overview(terminal_core::models::TerminalOverview {
        account_id: "acc_mvp_paper".to_string(),
        runtime_mode: "observe_only".to_string(),
        watchlist: vec![
            terminal_core::models::TerminalWatchRow {
                symbol: "AAPL.US".to_string(),
                venue: "US_EQUITY".to_string(),
                last_price: Some(124.0),
            },
            terminal_core::models::TerminalWatchRow {
                symbol: "MSFT.US".to_string(),
                venue: "US_EQUITY".to_string(),
                last_price: Some(310.0),
            },
        ],
        positions: Vec::new(),
        open_orders: Vec::new(),
    });

    assert_eq!(app.selected_symbol.as_deref(), Some("AAPL.US"));
}

#[test]
fn watchlist_navigation_wraps_around() {
    let mut app = terminal_tui::app::AppState::new();
    app.apply_overview(terminal_core::models::TerminalOverview {
        account_id: "acc_mvp_paper".to_string(),
        runtime_mode: "observe_only".to_string(),
        watchlist: vec![
            terminal_core::models::TerminalWatchRow {
                symbol: "AAPL.US".to_string(),
                venue: "US_EQUITY".to_string(),
                last_price: Some(124.0),
            },
            terminal_core::models::TerminalWatchRow {
                symbol: "MSFT.US".to_string(),
                venue: "US_EQUITY".to_string(),
                last_price: Some(310.0),
            },
        ],
        positions: Vec::new(),
        open_orders: Vec::new(),
    });

    app.select_next_symbol();
    assert_eq!(app.selected_symbol.as_deref(), Some("MSFT.US"));

    app.select_next_symbol();
    assert_eq!(app.selected_symbol.as_deref(), Some("AAPL.US"));

    app.select_previous_symbol();
    assert_eq!(app.selected_symbol.as_deref(), Some("MSFT.US"));
}

#[test]
fn panel_cycle_includes_events_panel() {
    let mut app = terminal_tui::app::AppState::new();

    app.select_next_panel();
    assert_eq!(app.active_panel, terminal_tui::app::ActivePanel::Quote);
    app.select_next_panel();
    assert_eq!(app.active_panel, terminal_tui::app::ActivePanel::Orders);
    app.select_next_panel();
    assert_eq!(app.active_panel, terminal_tui::app::ActivePanel::Positions);
    app.select_next_panel();
    assert_eq!(app.active_panel, terminal_tui::app::ActivePanel::Events);
    app.select_next_panel();
    assert_eq!(app.active_panel, terminal_tui::app::ActivePanel::Watchlist);
}

#[test]
fn event_panel_focus_scrolls_events_without_changing_selected_symbol() {
    let mut app = terminal_tui::app::AppState::new();
    app.apply_overview(terminal_core::models::TerminalOverview {
        account_id: "acc_mvp_paper".to_string(),
        runtime_mode: "observe_only".to_string(),
        watchlist: vec![
            terminal_core::models::TerminalWatchRow {
                symbol: "AAPL.US".to_string(),
                venue: "US_EQUITY".to_string(),
                last_price: Some(124.0),
            },
            terminal_core::models::TerminalWatchRow {
                symbol: "MSFT.US".to_string(),
                venue: "US_EQUITY".to_string(),
                last_price: Some(310.0),
            },
        ],
        positions: Vec::new(),
        open_orders: Vec::new(),
    });
    for index in 0..6 {
        app.push_event(format!("order event: item-{index}"));
    }
    app.active_panel = terminal_tui::app::ActivePanel::Events;

    app.scroll_events_down(3);

    assert_eq!(app.selected_symbol.as_deref(), Some("AAPL.US"));
    assert_eq!(app.event_scroll, 1);
}

#[test]
fn websocket_hello_clears_degraded_state_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();
    app.connection_state = terminal_tui::app::ConnectionState::Degraded;

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::Hello {
        schema_version: 1,
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert_eq!(
        app.connection_state,
        terminal_tui::app::ConnectionState::Connected
    );
    assert_eq!(app.status_message.as_deref(), Some("stream connected"));
}

#[test]
fn new_terminal_starts_in_connecting_state() {
    let app = terminal_tui::app::AppState::new();
    assert_eq!(
        app.connection_state,
        terminal_tui::app::ConnectionState::Connecting
    );
    assert_eq!(app.connection_label(), "WS CONNECTING");
    assert!(app.recent_events.is_empty());
}

#[test]
fn polling_success_after_disconnect_shows_reconnecting_state() {
    let mut app = terminal_tui::app::AppState::new();
    app.handle_stream_disconnected();

    app.handle_polling_ok();

    assert_eq!(
        app.connection_state,
        terminal_tui::app::ConnectionState::Reconnecting
    );
    assert_eq!(app.connection_label(), "WS RECONNECTING");
}

#[test]
fn polling_failure_marks_terminal_degraded() {
    let mut app = terminal_tui::app::AppState::new();

    app.handle_polling_degraded("quote refresh failed".to_string());

    assert_eq!(
        app.connection_state,
        terminal_tui::app::ConnectionState::Degraded
    );
    assert_eq!(app.connection_label(), "DEGRADED");
}

#[test]
fn quote_refresh_failure_clears_stale_quote_for_selected_symbol() {
    let mut app = terminal_tui::app::AppState::new();
    app.selected_symbol = Some("MSFT.US".to_string());
    app.quote = Some(terminal_core::models::QuoteView {
        symbol: "AAPL.US".to_string(),
        venue: "US_EQUITY".to_string(),
        last_price: Some(124.0),
        day_high: Some(126.0),
        day_low: Some(119.5),
        bars: vec![terminal_core::models::QuoteBar {
            ts_ms: 1000,
            open: 120.0,
            high: 126.0,
            low: 119.5,
            close: 124.0,
            volume: 1000.0,
        }],
    });

    app.handle_quote_refresh_failed(
        "MSFT.US",
        "quote refresh failed: instrument not found".to_string(),
    );

    assert!(app.quote.is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("quote refresh failed: instrument not found")
    );
}

#[test]
fn quote_updated_event_updates_watchlist_and_active_quote_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();
    app.apply_overview(terminal_core::models::TerminalOverview {
        account_id: "acc_mvp_paper".to_string(),
        runtime_mode: "observe_only".to_string(),
        watchlist: vec![terminal_core::models::TerminalWatchRow {
            symbol: "AAPL.US".to_string(),
            venue: "US_EQUITY".to_string(),
            last_price: Some(124.0),
        }],
        positions: Vec::new(),
        open_orders: Vec::new(),
    });
    app.apply_quote(terminal_core::models::QuoteView {
        symbol: "AAPL.US".to_string(),
        venue: "US_EQUITY".to_string(),
        last_price: Some(124.0),
        day_high: Some(126.0),
        day_low: Some(119.5),
        bars: vec![terminal_core::models::QuoteBar {
            ts_ms: 1000,
            open: 120.0,
            high: 126.0,
            low: 119.5,
            close: 124.0,
            volume: 1000.0,
        }],
    });

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::QuoteUpdated {
        payload: serde_json::json!({
            "symbol": "AAPL.US",
            "venue": "US_EQUITY",
            "last_price": 128.0,
            "day_high": 128.0,
            "day_low": 119.5,
            "bars": [
                {
                    "ts_ms": 1000,
                    "open": 120.0,
                    "high": 126.0,
                    "low": 119.5,
                    "close": 124.0,
                    "volume": 1000.0
                },
                {
                    "ts_ms": 2000,
                    "open": 124.0,
                    "high": 128.0,
                    "low": 123.5,
                    "close": 128.0,
                    "volume": 1400.0
                }
            ]
        }),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert_eq!(app.watchlist[0].last_price, Some(128.0));
    assert_eq!(app.quote.as_ref().and_then(|quote| quote.last_price), Some(128.0));
    assert_eq!(app.quote.as_ref().map(|quote| quote.bars.len()), Some(2));
    assert_eq!(
        app.recent_events.first().map(|event| event.message.as_str()),
        Some("quote event: quote_updated")
    );
    assert_eq!(
        app.recent_events.first().map(|event| event.kind),
        Some(terminal_tui::app::EventKind::Quote)
    );
}

#[test]
fn order_events_request_refresh() {
    let mut app = terminal_tui::app::AppState::new();

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::OrderCreated {
        payload: serde_json::json!({
            "order_id": "ord-1",
            "symbol": "AAPL.US",
        }),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::Refresh);
    assert_eq!(
        app.status_message.as_deref(),
        Some("order event: order_created")
    );
}

#[test]
fn order_created_event_inserts_new_order_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::OrderCreated {
        payload: serde_json::json!({
            "order_id": "ord-2",
            "venue": "US_EQUITY",
            "symbol": "AAPL.US",
            "side": "buy",
            "qty": 15.0,
            "status": "SUBMITTED",
            "order_type": "limit",
            "limit_price": 125.25,
            "exchange_ref": "lb-2",
            "created_at_ms": 2000,
            "updated_at_ms": 2001,
        }),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert_eq!(app.open_orders.len(), 1);
    assert_eq!(app.open_orders[0].order_id, "ord-2");
    assert_eq!(app.open_orders[0].qty, 15.0);
    assert_eq!(app.open_orders[0].limit_price, Some(125.25));
}

#[test]
fn websocket_error_updates_status_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::Error {
        error_code: "risk_denied".to_string(),
        message: "observe_only".to_string(),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert_eq!(
        app.status_message.as_deref(),
        Some("stream error risk_denied: observe_only")
    );
    assert_eq!(
        app.recent_events.first().map(|event| event.message.as_str()),
        Some("stream error risk_denied: observe_only")
    );
    assert_eq!(
        app.recent_events.first().map(|event| event.kind),
        Some(terminal_tui::app::EventKind::Error)
    );
}

#[test]
fn recent_events_keep_most_recent_entries_only() {
    let mut app = terminal_tui::app::AppState::new();
    for index in 0..8 {
        app.push_event(format!("event-{index}"));
    }

    assert_eq!(app.recent_events.len(), 6);
    assert_eq!(app.recent_events.first().map(|event| event.message.as_str()), Some("event-7"));
    assert_eq!(app.recent_events.last().map(|event| event.message.as_str()), Some("event-2"));
}

#[test]
fn recent_events_assign_expected_kind_labels() {
    let mut app = terminal_tui::app::AppState::new();
    app.push_event("stream connected".to_string());
    app.push_event("quote event: quote_updated".to_string());
    app.push_event("order event: order_created".to_string());
    app.push_event("stream error risk_denied: observe_only".to_string());
    app.push_event("synced | account=acc_mvp_paper symbol=AAPL.US".to_string());

    assert_eq!(app.recent_events[0].kind, terminal_tui::app::EventKind::Sync);
    assert_eq!(app.recent_events[1].kind, terminal_tui::app::EventKind::Error);
    assert_eq!(app.recent_events[2].kind, terminal_tui::app::EventKind::Order);
    assert_eq!(app.recent_events[3].kind, terminal_tui::app::EventKind::Quote);
    assert_eq!(app.recent_events[4].kind, terminal_tui::app::EventKind::Ws);
}

#[test]
fn event_filter_cycles_and_filters_recent_events() {
    let mut app = terminal_tui::app::AppState::new();
    app.push_event("stream connected".to_string());
    app.push_event("quote event: quote_updated".to_string());
    app.push_event("order event: order_created".to_string());
    app.push_event("stream error risk_denied: observe_only".to_string());

    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::All);
    assert_eq!(app.filtered_recent_events().len(), 4);

    app.cycle_event_filter();
    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::Order);
    assert_eq!(app.filtered_recent_events().len(), 1);
    assert_eq!(
        app.filtered_recent_events()[0].kind,
        terminal_tui::app::EventKind::Order
    );

    app.cycle_event_filter();
    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::Quote);
    assert_eq!(app.filtered_recent_events().len(), 1);

    app.cycle_event_filter();
    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::Ws);
    assert_eq!(app.filtered_recent_events().len(), 1);

    app.cycle_event_filter();
    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::Error);
    assert_eq!(app.filtered_recent_events().len(), 1);

    app.cycle_event_filter();
    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::Sync);
    assert!(app.filtered_recent_events().is_empty());

    app.cycle_event_filter();
    assert_eq!(app.event_filter, terminal_tui::app::EventFilter::All);
}

#[test]
fn visible_recent_events_respects_scroll_offset() {
    let mut app = terminal_tui::app::AppState::new();
    for index in 0..6 {
        app.push_event(format!("order event: item-{index}"));
    }

    let top = app.visible_recent_events(3);
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].message, "order event: item-5");
    assert_eq!(top[2].message, "order event: item-3");

    app.scroll_events_down(3);
    let scrolled = app.visible_recent_events(3);
    assert_eq!(scrolled.len(), 3);
    assert_eq!(scrolled[0].message, "order event: item-4");
    assert_eq!(scrolled[2].message, "order event: item-2");

    app.scroll_events_down(3);
    app.scroll_events_down(3);
    let bottom = app.visible_recent_events(3);
    assert_eq!(bottom[0].message, "order event: item-2");
    assert_eq!(bottom[2].message, "order event: item-0");

    app.scroll_events_up(1);
    let up_once = app.visible_recent_events(3);
    assert_eq!(up_once[0].message, "order event: item-3");
}

#[test]
fn cycling_event_filter_resets_event_scroll() {
    let mut app = terminal_tui::app::AppState::new();
    for index in 0..6 {
        app.push_event(format!("quote event: item-{index}"));
    }
    app.scroll_events_down(3);
    assert_eq!(app.event_scroll, 1);

    app.cycle_event_filter();

    assert_eq!(app.event_scroll, 0);
}

#[test]
fn order_updated_event_mutates_existing_order_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();
    app.open_orders.push(sample_open_order());

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::OrderUpdated {
        payload: serde_json::json!({
            "order_id": "ord-1",
            "status": "PARTIALLY_FILLED",
            "qty": 12.0,
            "limit_price": 124.5,
        }),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert_eq!(app.open_orders[0].status, "PARTIALLY_FILLED");
    assert_eq!(app.open_orders[0].qty, 12.0);
    assert_eq!(app.open_orders[0].limit_price, Some(124.5));
}

#[test]
fn order_cancelled_event_removes_existing_order_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();
    app.open_orders.push(sample_open_order());

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::OrderCancelled {
        payload: serde_json::json!({
            "order_id": "ord-1",
        }),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert!(app.open_orders.is_empty());
}

#[test]
fn order_replaced_event_updates_existing_order_without_refresh() {
    let mut app = terminal_tui::app::AppState::new();
    app.open_orders.push(sample_open_order());

    let effect = app.handle_stream_message(&terminal_core::models::StreamMessage::OrderReplaced {
        payload: serde_json::json!({
            "order_id": "ord-1",
            "qty": 20.0,
            "limit_price": 126.25,
        }),
    });

    assert_eq!(effect, terminal_tui::app::StreamEffect::NoRefresh);
    assert_eq!(app.open_orders[0].qty, 20.0);
    assert_eq!(app.open_orders[0].limit_price, Some(126.25));
}

fn sample_open_order() -> terminal_core::models::OpenOrderRow {
    terminal_core::models::OpenOrderRow {
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
    }
}
