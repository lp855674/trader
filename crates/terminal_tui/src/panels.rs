use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use crate::app::{ActivePanel, AppState, ConnectionState, EventKind};

pub fn render(frame: &mut Frame, app: &AppState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(45),
            Constraint::Percentage(35),
        ])
        .split(layout[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body[2]);

    render_header(frame, app, layout[0]);
    render_watchlist(frame, app, body[0]);
    render_quote(frame, app, body[1]);
    render_orders(frame, app, right[0]);
    render_positions(frame, app, right[1]);
    render_status_bar(frame, app, layout[2]);
}

pub fn render_header(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Min(0),
            Constraint::Length(26),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(app.connection_label())
            .style(connection_style(app.connection_state))
            .block(Block::default().borders(Borders::ALL).title("Link")),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "account={} | runtime={} | symbol={} | events={}",
            app.active_account,
            app.runtime_mode,
            app.selected_symbol.as_deref().unwrap_or("-"),
            app.event_filter_label(),
        ))
        .block(Block::default().borders(Borders::ALL).title("Session")),
        sections[1],
    );
    frame.render_widget(
        Paragraph::new("q quit | Tab switch | arrows or j/k move | r refresh | e events")
            .block(Block::default().borders(Borders::ALL).title("Keys")),
        sections[2],
    );
}

pub fn render_watchlist(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let rows = app.watchlist.iter().enumerate().map(|(index, row)| {
        let style = if index == app.selected_watch_index {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            Cell::from(row.symbol.clone()),
            Cell::from(row.venue.clone()),
            Cell::from(format_option_f64(row.last_price)),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(45),
            Constraint::Percentage(30),
            Constraint::Percentage(25),
        ],
    )
    .header(Row::new(vec!["Symbol", "Venue", "Last"]).style(header_style()))
    .block(panel_block("Watchlist", app.active_panel == ActivePanel::Watchlist));
    frame.render_widget(table, area);
}

pub fn render_quote(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let text = if let Some(quote) = &app.quote {
        let last_bar = quote.bars.last();
        vec![
            Line::from(format!("symbol: {}", quote.symbol)),
            Line::from(format!("venue: {}", quote.venue)),
            Line::from(format!("last: {}", format_option_f64(quote.last_price))),
            Line::from(format!("day high: {}", format_option_f64(quote.day_high))),
            Line::from(format!("day low: {}", format_option_f64(quote.day_low))),
            Line::from(format!("bars: {}", quote.bars.len())),
            Line::from(format!(
                "latest close: {}",
                last_bar
                    .map(|bar| format!("{:.4}", bar.close))
                    .unwrap_or_else(|| "-".to_string())
            )),
        ]
    } else {
        vec![
            Line::from(format!(
                "symbol: {}",
                app.selected_symbol.as_deref().unwrap_or("-")
            )),
            Line::from("quote unavailable"),
        ]
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(panel_block("Quote", app.active_panel == ActivePanel::Quote))
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub fn render_orders(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let rows = app.open_orders.iter().map(|row| {
        Row::new(vec![
            Cell::from(row.symbol.clone()),
            Cell::from(row.side.clone()),
            Cell::from(format!("{:.4}", row.qty)),
            Cell::from(format_option_f64(row.limit_price)),
            Cell::from(row.status.clone()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
    )
    .header(Row::new(vec!["Symbol", "Side", "Qty", "Limit", "Status"]).style(header_style()))
    .block(panel_block("Orders", app.active_panel == ActivePanel::Orders));
    frame.render_widget(table, area);
}

pub fn render_positions(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let rows = app.positions.iter().map(|row| {
        Row::new(vec![
            Cell::from(row.symbol.clone()),
            Cell::from(row.venue.clone()),
            Cell::from(format!("{:.4}", row.net_qty)),
            Cell::from(row.last_fill_at_ms.to_string()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
        ],
    )
    .header(Row::new(vec!["Symbol", "Venue", "Net Qty", "Last Fill"]).style(header_style()))
    .block(
        panel_block("Positions + Runtime", app.active_panel == ActivePanel::Positions).title(
            format!(
                "Positions + Runtime [{} | {}]",
                app.active_account, app.runtime_mode
            ),
        ),
    );
    frame.render_widget(table, area);
}

pub fn render_status_bar(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);
    let base_message = app.status_message.as_deref().unwrap_or("ready");
    let window_size = sections[1].height.saturating_sub(2) as usize;
    let visible_events = app.visible_recent_events(window_size.max(1));
    let recent_event_lines = if visible_events.is_empty() {
        vec![Line::from("no recent events")]
    } else {
        visible_events
            .iter()
            .map(|event| {
                Line::from(vec![
                    ratatui::text::Span::styled(
                        format!("[{}] ", event_kind_label(event.kind)),
                        event_kind_style(event.kind),
                    ),
                    ratatui::text::Span::raw(event.message.clone()),
                ])
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(base_message)
            .block(Block::default().borders(Borders::ALL).title("Status")),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new(recent_event_lines)
            .block(
                panel_block(
                    format!(
                        "Recent Events [{} @{}]",
                        app.event_filter_label(),
                        app.event_scroll + 1
                    ),
                    app.active_panel == ActivePanel::Events,
                ),
            )
            .wrap(Wrap { trim: true }),
        sections[1],
    );
}

fn panel_block(title: impl Into<String>, is_active: bool) -> Block<'static> {
    let border_style = if is_active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Block::default()
        .borders(Borders::ALL)
        .title(title.into())
        .border_style(border_style)
}

fn header_style() -> Style {
    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

fn connection_style(state: ConnectionState) -> Style {
    match state {
        ConnectionState::Connecting => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ConnectionState::Connected => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ConnectionState::Reconnecting => Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD),
        ConnectionState::Degraded => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

fn event_kind_label(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Ws => "WS",
        EventKind::Order => "ORDER",
        EventKind::Quote => "QUOTE",
        EventKind::Error => "ERROR",
        EventKind::Sync => "SYNC",
    }
}

fn event_kind_style(kind: EventKind) -> Style {
    match kind {
        EventKind::Ws => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        EventKind::Order => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        EventKind::Quote => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        EventKind::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        EventKind::Sync => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    }
}

fn format_option_f64(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.4}"))
        .unwrap_or_else(|| "-".to_string())
}
