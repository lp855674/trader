use terminal_core::models::{OpenOrderRow, QuoteView, TerminalOverview, TerminalWatchRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Watchlist,
    Quote,
    Orders,
    Positions,
    Events,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationState {
    None,
    Review,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEffect {
    NoRefresh,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Ws,
    Order,
    Quote,
    Error,
    Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFilter {
    All,
    Order,
    Quote,
    Ws,
    Error,
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEvent {
    pub kind: EventKind,
    pub message: String,
}

const MAX_RECENT_EVENTS: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub active_panel: ActivePanel,
    pub active_account: String,
    pub selected_symbol: Option<String>,
    pub selected_watch_index: usize,
    pub confirmation_state: ConfirmationState,
    pub connection_state: ConnectionState,
    pub event_filter: EventFilter,
    pub event_scroll: usize,
    pub status_message: Option<String>,
    pub recent_events: Vec<RecentEvent>,
    pub runtime_mode: String,
    pub watchlist: Vec<TerminalWatchRow>,
    pub positions: Vec<terminal_core::models::LocalPositionRow>,
    pub open_orders: Vec<OpenOrderRow>,
    pub quote: Option<QuoteView>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_panel: ActivePanel::Watchlist,
            active_account: "acc_mvp_paper".to_string(),
            selected_symbol: None,
            selected_watch_index: 0,
            confirmation_state: ConfirmationState::None,
            connection_state: ConnectionState::Connecting,
            event_filter: EventFilter::All,
            event_scroll: 0,
            status_message: None,
            recent_events: Vec::new(),
            runtime_mode: "observe_only".to_string(),
            watchlist: Vec::new(),
            positions: Vec::new(),
            open_orders: Vec::new(),
            quote: None,
        }
    }

    pub fn begin_submit(&mut self, symbol: String, side: String) {
        self.selected_symbol = Some(symbol);
        self.confirmation_state = if self.active_account == "acc_lb_live" {
            ConfirmationState::Review
        } else {
            ConfirmationState::Final
        };
        self.push_event(format!("submit {side} order pending confirmation"));
    }

    pub fn confirm_current_action(&mut self) {
        self.confirmation_state = match self.confirmation_state {
            ConfirmationState::None => ConfirmationState::None,
            ConfirmationState::Review => ConfirmationState::Final,
            ConfirmationState::Final => ConfirmationState::None,
        };
    }

    pub fn handle_stream_disconnected(&mut self) {
        self.connection_state = ConnectionState::Reconnecting;
        self.push_event("stream disconnected; attempting reconnect".to_string());
    }

    pub fn handle_polling_ok(&mut self) {
        if self.connection_state != ConnectionState::Connected {
            self.connection_state = ConnectionState::Reconnecting;
        }
    }

    pub fn handle_polling_degraded(&mut self, message: String) {
        self.connection_state = ConnectionState::Degraded;
        self.push_event(message);
    }

    pub fn handle_quote_refresh_failed(&mut self, symbol: &str, message: String) {
        if self.selected_symbol.as_deref() == Some(symbol) {
            self.quote = None;
        }
        self.handle_polling_degraded(message);
    }

    pub fn handle_stream_message(
        &mut self,
        message: &terminal_core::models::StreamMessage,
    ) -> StreamEffect {
        match message {
            terminal_core::models::StreamMessage::Hello { .. } => {
                self.connection_state = ConnectionState::Connected;
                self.push_event("stream connected".to_string());
                StreamEffect::NoRefresh
            }
            terminal_core::models::StreamMessage::OrderCreated { payload } => {
                if self.apply_order_created(payload) {
                    self.push_event("order event: order_created".to_string());
                    StreamEffect::NoRefresh
                } else {
                    self.push_event("order event: order_created".to_string());
                    StreamEffect::Refresh
                }
            }
            terminal_core::models::StreamMessage::OrderUpdated { payload } => {
                if self.apply_order_updated(payload) {
                    self.push_event("order event: order_updated".to_string());
                    StreamEffect::NoRefresh
                } else {
                    self.push_event("order event: order_updated".to_string());
                    StreamEffect::Refresh
                }
            }
            terminal_core::models::StreamMessage::OrderCancelled { payload } => {
                if self.apply_order_cancelled(payload) {
                    self.push_event("order event: order_cancelled".to_string());
                    StreamEffect::NoRefresh
                } else {
                    self.push_event("order event: order_cancelled".to_string());
                    StreamEffect::Refresh
                }
            }
            terminal_core::models::StreamMessage::OrderReplaced { payload } => {
                if self.apply_order_replaced(payload) {
                    self.push_event("order event: order_replaced".to_string());
                    StreamEffect::NoRefresh
                } else {
                    self.push_event("order event: order_replaced".to_string());
                    StreamEffect::Refresh
                }
            }
            terminal_core::models::StreamMessage::QuoteUpdated { payload } => {
                if self.apply_quote_updated(payload) {
                    self.push_event("quote event: quote_updated".to_string());
                    StreamEffect::NoRefresh
                } else {
                    self.push_event("quote event: quote_updated".to_string());
                    StreamEffect::Refresh
                }
            }
            terminal_core::models::StreamMessage::Error {
                error_code,
                message,
            } => {
                self.push_event(format!("stream error {error_code}: {message}"));
                StreamEffect::NoRefresh
            }
        }
    }

    pub fn apply_overview(&mut self, overview: TerminalOverview) {
        self.active_account = overview.account_id;
        self.runtime_mode = overview.runtime_mode;
        self.watchlist = overview.watchlist;
        self.positions = overview.positions;
        self.open_orders = overview.open_orders;

        if self.watchlist.is_empty() {
            self.selected_watch_index = 0;
            self.selected_symbol = None;
            self.quote = None;
            return;
        }

        let selected_index = self
            .selected_symbol
            .as_deref()
            .and_then(|symbol| self.watchlist.iter().position(|row| row.symbol == symbol))
            .unwrap_or(0);
        self.selected_watch_index = selected_index;
        self.selected_symbol = self
            .watchlist
            .get(self.selected_watch_index)
            .map(|row| row.symbol.clone());
    }

    pub fn apply_quote(&mut self, quote: QuoteView) {
        self.selected_symbol = Some(quote.symbol.clone());
        if let Some(index) = self.watchlist.iter().position(|row| row.symbol == quote.symbol) {
            self.selected_watch_index = index;
        }
        self.quote = Some(quote);
    }

    pub fn select_next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Watchlist => ActivePanel::Quote,
            ActivePanel::Quote => ActivePanel::Orders,
            ActivePanel::Orders => ActivePanel::Positions,
            ActivePanel::Positions => ActivePanel::Events,
            ActivePanel::Events => ActivePanel::Watchlist,
        };
    }

    pub fn select_previous_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Watchlist => ActivePanel::Events,
            ActivePanel::Quote => ActivePanel::Watchlist,
            ActivePanel::Orders => ActivePanel::Quote,
            ActivePanel::Positions => ActivePanel::Orders,
            ActivePanel::Events => ActivePanel::Positions,
        };
    }

    pub fn select_next_symbol(&mut self) {
        if self.watchlist.is_empty() {
            return;
        }
        self.selected_watch_index = (self.selected_watch_index + 1) % self.watchlist.len();
        self.selected_symbol = self
            .watchlist
            .get(self.selected_watch_index)
            .map(|row| row.symbol.clone());
    }

    pub fn select_previous_symbol(&mut self) {
        if self.watchlist.is_empty() {
            return;
        }
        self.selected_watch_index = if self.selected_watch_index == 0 {
            self.watchlist.len() - 1
        } else {
            self.selected_watch_index - 1
        };
        self.selected_symbol = self
            .watchlist
            .get(self.selected_watch_index)
            .map(|row| row.symbol.clone());
    }

    pub fn connection_label(&self) -> &'static str {
        match self.connection_state {
            ConnectionState::Connecting => "WS CONNECTING",
            ConnectionState::Connected => "WS CONNECTED",
            ConnectionState::Reconnecting => "WS RECONNECTING",
            ConnectionState::Degraded => "DEGRADED",
        }
    }

    pub fn event_filter_label(&self) -> &'static str {
        match self.event_filter {
            EventFilter::All => "ALL",
            EventFilter::Order => "ORDER",
            EventFilter::Quote => "QUOTE",
            EventFilter::Ws => "WS",
            EventFilter::Error => "ERROR",
            EventFilter::Sync => "SYNC",
        }
    }

    pub fn cycle_event_filter(&mut self) {
        self.event_filter = match self.event_filter {
            EventFilter::All => EventFilter::Order,
            EventFilter::Order => EventFilter::Quote,
            EventFilter::Quote => EventFilter::Ws,
            EventFilter::Ws => EventFilter::Error,
            EventFilter::Error => EventFilter::Sync,
            EventFilter::Sync => EventFilter::All,
        };
        self.event_scroll = 0;
        self.status_message = Some(format!("event filter: {}", self.event_filter_label()));
    }

    pub fn filtered_recent_events(&self) -> Vec<&RecentEvent> {
        self.recent_events
            .iter()
            .filter(|event| match self.event_filter {
                EventFilter::All => true,
                EventFilter::Order => event.kind == EventKind::Order,
                EventFilter::Quote => event.kind == EventKind::Quote,
                EventFilter::Ws => event.kind == EventKind::Ws,
                EventFilter::Error => event.kind == EventKind::Error,
                EventFilter::Sync => event.kind == EventKind::Sync,
            })
            .collect()
    }

    pub fn visible_recent_events(&self, limit: usize) -> Vec<&RecentEvent> {
        let filtered = self.filtered_recent_events();
        if filtered.is_empty() || limit == 0 {
            return Vec::new();
        }
        let max_offset = filtered.len().saturating_sub(limit);
        let start = self.event_scroll.min(max_offset);
        filtered.into_iter().skip(start).take(limit).collect()
    }

    pub fn scroll_events_down(&mut self, window_size: usize) {
        let filtered_len = self.filtered_recent_events().len();
        if filtered_len <= window_size || window_size == 0 {
            self.event_scroll = 0;
            return;
        }
        let max_offset = filtered_len - window_size;
        self.event_scroll = (self.event_scroll + 1).min(max_offset);
        self.status_message = Some(format!("event scroll: {}", self.event_scroll + 1));
    }

    pub fn scroll_events_up(&mut self, _window_size: usize) {
        self.event_scroll = self.event_scroll.saturating_sub(1);
        self.status_message = Some(format!("event scroll: {}", self.event_scroll + 1));
    }

    pub fn push_event(&mut self, message: String) {
        self.push_typed_event(infer_event_kind(&message), message);
    }

    pub fn push_typed_event(&mut self, kind: EventKind, message: String) {
        self.status_message = Some(message.clone());
        self.recent_events.insert(0, RecentEvent { kind, message });
        if self.recent_events.len() > MAX_RECENT_EVENTS {
            self.recent_events.truncate(MAX_RECENT_EVENTS);
        }
    }

    fn apply_order_updated(&mut self, payload: &serde_json::Value) -> bool {
        let Some(order_update) = deserialize_payload::<OrderUpdatedPayload>(payload) else {
            return false;
        };
        let Some(order) = self
            .open_orders
            .iter_mut()
            .find(|order| order.order_id == order_update.order_id)
        else {
            return false;
        };
        order.status = order_update.status;
        order.qty = order_update.qty;
        order.limit_price = order_update.limit_price;
        true
    }

    fn apply_order_created(&mut self, payload: &serde_json::Value) -> bool {
        let Some(order_created) = deserialize_payload::<OrderCreatedPayload>(payload) else {
            return false;
        };
        let new_order = OpenOrderRow {
            order_id: order_created.order_id,
            venue: order_created.venue,
            symbol: order_created.symbol,
            side: order_created.side,
            qty: order_created.qty,
            status: order_created.status,
            order_type: order_created.order_type,
            limit_price: order_created.limit_price,
            exchange_ref: order_created.exchange_ref,
            created_at_ms: order_created.created_at_ms,
            updated_at_ms: order_created.updated_at_ms,
        };

        if let Some(existing_order) = self
            .open_orders
            .iter_mut()
            .find(|order| order.order_id == new_order.order_id)
        {
            *existing_order = new_order;
        } else {
            self.open_orders.insert(0, new_order);
        }
        true
    }

    fn apply_order_cancelled(&mut self, payload: &serde_json::Value) -> bool {
        let Some(order_cancelled) = deserialize_payload::<OrderCancelledPayload>(payload) else {
            return false;
        };
        let original_len = self.open_orders.len();
        self.open_orders
            .retain(|order| order.order_id != order_cancelled.order_id);
        self.open_orders.len() != original_len
    }

    fn apply_order_replaced(&mut self, payload: &serde_json::Value) -> bool {
        let Some(order_replaced) = deserialize_payload::<OrderReplacedPayload>(payload) else {
            return false;
        };
        let Some(order) = self
            .open_orders
            .iter_mut()
            .find(|order| order.order_id == order_replaced.order_id)
        else {
            return false;
        };
        order.qty = order_replaced.qty;
        order.limit_price = order_replaced.limit_price;
        true
    }

    fn apply_quote_updated(&mut self, payload: &serde_json::Value) -> bool {
        let Some(quote_updated) = deserialize_payload::<QuoteUpdatedPayload>(payload) else {
            return false;
        };

        if let Some(watch_row) = self
            .watchlist
            .iter_mut()
            .find(|row| row.symbol == quote_updated.symbol)
        {
            watch_row.last_price = quote_updated.last_price;
            watch_row.venue = quote_updated.venue.clone();
        }

        if self.selected_symbol.as_deref() == Some(quote_updated.symbol.as_str()) {
            self.quote = Some(QuoteView {
                symbol: quote_updated.symbol,
                venue: quote_updated.venue,
                last_price: quote_updated.last_price,
                day_high: quote_updated.day_high,
                day_low: quote_updated.day_low,
                bars: quote_updated.bars,
            });
        }

        true
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, serde::Deserialize)]
struct OrderCreatedPayload {
    order_id: String,
    venue: String,
    symbol: String,
    side: String,
    qty: f64,
    status: String,
    order_type: String,
    limit_price: Option<f64>,
    exchange_ref: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Debug, serde::Deserialize)]
struct OrderUpdatedPayload {
    order_id: String,
    status: String,
    qty: f64,
    limit_price: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct OrderCancelledPayload {
    order_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct OrderReplacedPayload {
    order_id: String,
    qty: f64,
    limit_price: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct QuoteUpdatedPayload {
    symbol: String,
    venue: String,
    last_price: Option<f64>,
    day_high: Option<f64>,
    day_low: Option<f64>,
    bars: Vec<terminal_core::models::QuoteBar>,
}

fn deserialize_payload<T>(payload: &serde_json::Value) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(payload.clone()).ok()
}

fn infer_event_kind(message: &str) -> EventKind {
    if message.starts_with("order event:") || message.starts_with("submit ") {
        EventKind::Order
    } else if message.starts_with("quote event:") {
        EventKind::Quote
    } else if message.starts_with("stream error ") {
        EventKind::Error
    } else if message.starts_with("stream ") {
        EventKind::Ws
    } else {
        EventKind::Sync
    }
}
