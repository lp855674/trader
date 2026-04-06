//! Event Bus for strategy system
//!
//! Provides broadcast channel pattern for decoupled event delivery with filtering and routing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::broadcast::error::SendError;
use tokio::sync::broadcast::Sender;
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, info, warn};

/// Event types that flow through the event bus
#[derive(Debug, Clone)]
pub enum EventBusEvent {
    /// Strategy signal generated
    Signal {
        strategy_id: String,
        instrument_id: String,
        side: String,
        quantity: f64,
        timestamp_ms: i64,
    },
    /// Data update (kline or tick)
    DataUpdate {
        instrument_id: String,
        granularity: String,
        timestamp_ms: i64,
    },
    /// Error event
    Error {
        source: String,
        error: String,
    },
    /// System event
    System {
        kind: SystemEvent,
    },
}

#[derive(Debug, Clone)]
pub enum SystemEvent {
    Shutdown,
    HealthCheck,
    Metrics {
        timestamp: i64,
        metrics: HashMap<String, f64>,
    },
}

impl EventBusEvent {
    pub fn new_signal(
        strategy_id: String,
        instrument_id: String,
        side: String,
        quantity: f64,
        timestamp_ms: i64,
    ) -> Self {
        Self::Signal {
            strategy_id,
            instrument_id,
            side,
            quantity,
            timestamp_ms,
        }
    }

    pub fn new_data_update(instrument_id: String, granularity: String, timestamp_ms: i64) -> Self {
        Self::DataUpdate {
            instrument_id,
            granularity,
            timestamp_ms,
        }
    }
}

/// Event filter for routing
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// Filter by event type
    pub event_types: Vec<String>,
    /// Filter by instrument
    pub instruments: Vec<String>,
    /// Filter by strategy
    pub strategies: Vec<String>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self {
            event_types: vec![],
            instruments: vec![],
            strategies: vec![],
        }
    }

    pub fn matches(&self, event: &EventBusEvent) -> bool {
        if !self.event_types.is_empty() {
            let event_type = match event {
                EventBusEvent::Signal { .. } => "signal".to_string(),
                EventBusEvent::DataUpdate { .. } => "data_update".to_string(),
                EventBusEvent::Error { .. } => "error".to_string(),
                EventBusEvent::System { kind } => match kind {
                    SystemEvent::Shutdown => "system.shutdown".to_string(),
                    SystemEvent::HealthCheck => "system.health".to_string(),
                    SystemEvent::Metrics { .. } => "system.metrics".to_string(),
                },
            };
            if !self.event_types.contains(&event_type) {
                return false;
            }
        }

        if !self.instruments.is_empty() {
            let instrument_id = match event {
                EventBusEvent::Signal { instrument_id, .. } => instrument_id.clone(),
                EventBusEvent::DataUpdate { instrument_id, .. } => instrument_id.clone(),
                _ => return false,
            };
            if !self.instruments.contains(&instrument_id) {
                return false;
            }
        }

        if !self.strategies.is_empty() {
            let strategy_id = match event {
                EventBusEvent::Signal { strategy_id, .. } => strategy_id.clone(),
                _ => return false,
            };
            if !self.strategies.contains(&strategy_id) {
                return false;
            }
        }

        true
    }
}

/// Subscriber handle for event bus
#[derive(Debug)]
pub struct Subscriber {
    sender: Sender<EventBusEvent>,
    filter: Option<EventFilter>,
    sequence: AtomicU64,
    /// 保留一个订阅端，避免 `broadcast::channel` 返回的 `Receiver` 被丢弃后 `Sender::send` 全部失败。
    _keepalive: broadcast::Receiver<EventBusEvent>,
}

impl Subscriber {
    pub fn new(sender: Sender<EventBusEvent>, filter: Option<EventFilter>) -> Self {
        let _keepalive = sender.subscribe();
        Self {
            sender,
            filter,
            sequence: AtomicU64::new(0),
            _keepalive,
        }
    }

    pub fn send(&self, event: EventBusEvent) -> Result<(), SendError<EventBusEvent>> {
        self.sequence.fetch_add(1, Ordering::SeqCst);
        self.sender.send(event).map(|_| ())
    }

    pub fn get_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    pub fn is_filtered(&self, event: &EventBusEvent) -> bool {
        self.filter.as_ref().map_or(true, |f| f.matches(event))
    }
}

/// Event Bus with sequence tracking and filtering
pub struct EventBus {
    channel: broadcast::Sender<EventBusEvent>,
    sequence: AtomicU64,
    active: bool,
}

impl EventBus {
    pub fn new(capacity: usize) -> (Self, Subscriber) {
        let (tx, rx) = broadcast::channel(capacity);
        let subscriber = Subscriber::new(tx.clone(), None);
        drop(rx);

        (Self {
            channel: tx,
            sequence: AtomicU64::new(0),
            active: false,
        }, subscriber)
    }

    pub fn new_with_filter(capacity: usize, filter: EventFilter) -> (Self, Subscriber) {
        let (tx, rx) = broadcast::channel(capacity);
        let subscriber = Subscriber::new(tx.clone(), Some(filter));
        drop(rx);

        (Self {
            channel: tx,
            sequence: AtomicU64::new(0),
            active: false,
        }, subscriber)
    }

    pub async fn start(&mut self) {
        self.active = true;
        info!("EventBus started");

        let mut rx = self.channel.subscribe();
        let mut sequence = 0u64;

        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            sequence = sequence + 1;
                            self.sequence.store(sequence, Ordering::SeqCst);
                            debug!("EventBus received: {:?}", event);
                        }
                        Err(_) => {
                            self.active = false;
                            info!("EventBus channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("EventBus stopped");
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn get_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    pub fn advance_sequence(&self) -> u64 {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        self.channel
            .clone()
            .send(EventBusEvent::System {
                kind: SystemEvent::Metrics {
                    timestamp: seq as i64,
                    metrics: HashMap::new(),
                },
            })
            .ok();
        seq
    }

    pub fn send(&self, event: EventBusEvent) -> Result<(), SendError<EventBusEvent>> {
        self.channel.send(event).map(|_| ())
    }

    pub fn broadcast_health_check(&self) {
        let _ = self.channel.send(EventBusEvent::System {
            kind: SystemEvent::HealthCheck,
        });
    }

    pub fn broadcast_shutdown(&self) {
        let _ = self.channel.send(EventBusEvent::System {
            kind: SystemEvent::Shutdown,
        });
    }
}

/// Event loop processor
pub struct EventProcessor {
    subscriber: Subscriber,
    tick_interval: Duration,
    last_tick: Instant,
    rx: broadcast::Receiver<EventBusEvent>,
}

impl EventProcessor {
    pub fn new(subscriber: Subscriber, tick_interval: Duration) -> Self {
        let rx = subscriber.sender.subscribe();
        Self {
            subscriber,
            tick_interval,
            last_tick: Instant::now(),
            rx,
        }
    }

    pub async fn run(&mut self) {
        let mut ticker = interval(self.tick_interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.process_tick().await;
                }
                event = self.rx.recv() => {
                    match event {
                        Ok(event) => {
                            self.process_event(event).await;
                        }
                        Err(_) => {
                            info!("EventProcessor channel closed");
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn process_tick(&mut self) {
        self.last_tick = Instant::now();
        let _ = self.subscriber.sender.send(EventBusEvent::System {
            kind: SystemEvent::Metrics {
                timestamp: Instant::now().duration_since(Instant::now()).as_millis() as i64,
                metrics: HashMap::new(),
            },
        });
    }

    async fn process_event(&mut self, event: EventBusEvent) {
        match event {
            EventBusEvent::Signal { .. } => {
                debug!("Processing signal event");
            }
            EventBusEvent::DataUpdate { .. } => {
                debug!("Processing data update event");
            }
            EventBusEvent::Error { .. } => {
                debug!("Processing error event");
            }
            EventBusEvent::System { .. } => {
                debug!("Processing system event");
            }
        }
    }

    pub fn get_elapsed_since_last_tick(&self) -> Duration {
        self.last_tick.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_basic() {
        let (bus, _subscriber) = EventBus::new(100);
        let mut processor = EventProcessor::new(
            Subscriber::new(bus.channel.clone(), None),
            Duration::from_millis(100),
        );

        let handle = tokio::spawn(async move {
            processor.run().await;
        });

        // Send events
        for i in 0..5 {
            let _ = bus.send(EventBusEvent::DataUpdate {
                instrument_id: "BTC/USDT".to_string(),
                granularity: "1m".to_string(),
                timestamp_ms: 1712345678000 + i as i64,
            });
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.abort();
    }

    #[tokio::test]
    async fn test_event_filter() {
        let filter = EventFilter {
            event_types: vec!["signal".to_string()],
            instruments: vec!["BTC/USDT".to_string()],
            strategies: vec![],
        };

        let (_bus, subscriber) = EventBus::new_with_filter(100, filter);

        // Signal event should match
        let signal = EventBusEvent::new_signal(
            "strategy_1".to_string(),
            "BTC/USDT".to_string(),
            "buy".to_string(),
            1.0,
            1712345678000,
        );
        assert!(subscriber.is_filtered(&signal));

        // Wrong instrument should not match
        let wrong_signal = EventBusEvent::new_signal(
            "strategy_1".to_string(),
            "ETH/USDT".to_string(),
            "buy".to_string(),
            1.0,
            1712345678000,
        );
        assert!(!subscriber.is_filtered(&wrong_signal));
    }

    #[tokio::test]
    async fn test_sequence_number() {
        let (_bus, subscriber) = EventBus::new(100);

        let seq1 = subscriber.get_sequence();
        subscriber.send(EventBusEvent::DataUpdate {
            instrument_id: "BTC/USDT".to_string(),
            granularity: "1m".to_string(),
            timestamp_ms: 1712345678000,
        }).unwrap();
        let seq2 = subscriber.get_sequence();

        assert!(seq2 > seq1, "Sequence should increment");
    }
}
