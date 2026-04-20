pub mod actions;
pub mod app;
pub mod forms;
pub mod panels;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use terminal_core::models::StreamMessage;
use tokio::sync::mpsc;

pub async fn run(client: terminal_client::QuantdHttpClient) -> Result<(), String> {
    enable_raw_mode().map_err(|error| error.to_string())?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|error| error.to_string())?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|error| error.to_string())?;

    let result = run_loop(&mut terminal, client).await;

    disable_raw_mode().map_err(|error| error.to_string())?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|error| error.to_string())?;
    terminal.show_cursor().map_err(|error| error.to_string())?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: terminal_client::QuantdHttpClient,
) -> Result<(), String> {
    let mut app = app::AppState::new();
    refresh_app(&client, &mut app).await?;
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();
    tokio::spawn(run_stream_task(client.stream_client(), stream_tx));
    let mut last_refresh = Instant::now();

    loop {
        terminal
            .draw(|frame| panels::render(frame, &app))
            .map_err(|error| error.to_string())?;

        while let Ok(event) = stream_rx.try_recv() {
            match event {
                StreamEvent::Message(message) => {
                    if app.handle_stream_message(&message) == app::StreamEffect::Refresh {
                        refresh_app(&client, &mut app).await?;
                        last_refresh = Instant::now();
                    }
                }
                StreamEvent::Disconnected => app.handle_stream_disconnected(),
            }
        }

        if event::poll(Duration::from_millis(150)).map_err(|error| error.to_string())?
            && let Event::Key(key) = event::read().map_err(|error| error.to_string())?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let mut should_refresh = false;
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Tab => app.select_next_panel(),
                KeyCode::BackTab => app.select_previous_panel(),
                KeyCode::Up | KeyCode::Left | KeyCode::Char('k') => {
                    if app.active_panel == app::ActivePanel::Events {
                        app.scroll_events_up(1);
                    } else {
                        app.select_previous_symbol();
                        should_refresh = true;
                    }
                }
                KeyCode::Down | KeyCode::Right | KeyCode::Char('j') => {
                    if app.active_panel == app::ActivePanel::Events {
                        app.scroll_events_down(1);
                    } else {
                        app.select_next_symbol();
                        should_refresh = true;
                    }
                }
                KeyCode::Char('r') => should_refresh = true,
                KeyCode::Char('e') => app.cycle_event_filter(),
                KeyCode::PageDown => app.scroll_events_down(1),
                KeyCode::PageUp => app.scroll_events_up(1),
                _ => {}
            }

            if should_refresh {
                refresh_app(&client, &mut app).await?;
                last_refresh = Instant::now();
            }
        }

        if last_refresh.elapsed() >= Duration::from_secs(2) {
            refresh_app(&client, &mut app).await?;
            last_refresh = Instant::now();
        }
    }
}

async fn refresh_app(
    client: &terminal_client::QuantdHttpClient,
    app: &mut app::AppState,
) -> Result<(), String> {
    let overview = client
        .get_overview(&app.active_account)
        .await
        .map_err(|error| error.to_string())?;
    app.apply_overview(overview);

    if let Some(symbol) = app.selected_symbol.clone() {
        match client.get_quote(&symbol).await {
            Ok(quote) => {
                app.handle_polling_ok();
                app.apply_quote(quote);
                app.push_event(format!(
                    "synced | account={} symbol={}",
                    app.active_account, symbol
                ));
            }
            Err(error) => {
                app.handle_quote_refresh_failed(&symbol, format!(
                    "quote refresh failed: {}",
                    error.message()
                ));
            }
        }
    } else {
        app.quote = None;
        app.handle_polling_ok();
        app.push_event(format!(
            "synced | account={} watchlist is empty",
            app.active_account
        ));
    }

    Ok(())
}

#[derive(Debug)]
enum StreamEvent {
    Message(StreamMessage),
    Disconnected,
}

async fn run_stream_task(
    client: terminal_client::QuantdStreamClient,
    sender: mpsc::UnboundedSender<StreamEvent>,
) {
    loop {
        let mut stream = match client.connect().await {
            Ok(stream) => stream,
            Err(_) => {
                if sender.send(StreamEvent::Disconnected).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        loop {
            match terminal_client::QuantdStreamClient::next_message(&mut stream).await {
                Ok(Some(message)) => {
                    if sender.send(StreamEvent::Message(message)).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    if sender.send(StreamEvent::Disconnected).is_err() {
                        return;
                    }
                    break;
                }
                Err(_) => {
                    if sender.send(StreamEvent::Disconnected).is_err() {
                        return;
                    }
                    break;
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
