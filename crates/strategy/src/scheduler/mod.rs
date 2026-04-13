//! Hybrid Scheduler for strategy execution
//!
//! Provides periodic timer-based and event-driven execution with backpressure handling.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::{Instant, interval, sleep};
use tracing::{debug, error, info, warn};

/// Event types that can trigger scheduler actions
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// Timer tick at specified interval
    TimerTick,
    /// New data arrived (kline or tick)
    DataUpdate {
        instrument_id: String,
        timestamp_ms: i64,
    },
    /// Strategy signal generated
    Signal {
        strategy_id: String,
        instrument_id: String,
        timestamp_ms: i64,
    },
    /// Error occurred in data source
    Error { source: String, error: String },
    /// Shutdown request
    Shutdown,
    /// Sequence number advance
    SequenceAdvance { sequence: u64 },
}

impl SchedulerEvent {
    pub fn new_data_update(instrument_id: String, timestamp_ms: i64) -> Self {
        Self::DataUpdate {
            instrument_id,
            timestamp_ms,
        }
    }

    pub fn new_signal(strategy_id: String, instrument_id: String, timestamp_ms: i64) -> Self {
        Self::Signal {
            strategy_id,
            instrument_id,
            timestamp_ms,
        }
    }
}

/// Configuration for the scheduler
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Default interval for periodic execution in milliseconds
    pub default_interval_ms: u64,
    /// Maximum batch size for event processing
    pub max_batch_size: usize,
    /// Buffer size for event queue
    pub event_buffer_size: usize,
    /// Enable sequence number tracking
    pub enable_sequence: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            default_interval_ms: 1000, // 1 second
            max_batch_size: 1000,
            event_buffer_size: 10000,
            enable_sequence: true,
        }
    }
}

/// Periodic timer scheduler component
pub struct PeriodicScheduler {
    config: SchedulerConfig,
    interval: Duration,
    last_tick: Instant,
    active: bool,
}

impl PeriodicScheduler {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            config: SchedulerConfig::default(),
            interval: Duration::from_millis(interval_ms),
            last_tick: Instant::now(),
            active: false,
        }
    }

    pub async fn run(&mut self) {
        self.active = true;
        let mut ticker = interval(self.interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if !self.active {
                        break;
                    }
                    self.last_tick = Instant::now();
                    debug!("Periodic tick at {:?}", self.last_tick);
                }
            }
        }

        info!("Periodic scheduler stopped");
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn elapsed_since_last_tick(&self) -> Duration {
        self.last_tick.elapsed()
    }
}

/// Event-driven scheduler component
pub struct EventDrivenScheduler {
    event_bus: Arc<broadcast::Sender<SchedulerEvent>>,
    subscriber_handle: Option<tokio::task::JoinHandle<()>>,
    active: bool,
    sequence_counter: AtomicU64,
}

impl EventDrivenScheduler {
    pub fn new(event_bus: Arc<broadcast::Sender<SchedulerEvent>>, _enable_sequence: bool) -> Self {
        Self {
            event_bus,
            subscriber_handle: None,
            active: false,
            sequence_counter: AtomicU64::new(0),
        }
    }

    pub async fn start(&mut self) {
        self.active = true;

        let mut rx = self.event_bus.subscribe();

        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            match &event {
                                SchedulerEvent::TimerTick => {}
                                SchedulerEvent::Signal { .. } => {
                                    // Signal events are processed by subscribers
                                }
                                SchedulerEvent::DataUpdate { .. } => {
                                    // Data updates are processed by subscribers
                                }
                                SchedulerEvent::Error { .. } => {
                                    // Errors are logged
                                }
                                SchedulerEvent::SequenceAdvance { sequence: new_seq } => {
                                    self.sequence_counter.store(*new_seq, Ordering::SeqCst);
                                }
                                SchedulerEvent::Shutdown => {
                                    self.active = false;
                                    break;
                                }
                            }
                        }
                        Err(_) => {
                            // Channel closed
                            self.active = false;
                            break;
                        }
                    }
                }
            }
        }

        info!("Event-driven scheduler stopped");
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn get_sequence(&self) -> u64 {
        self.sequence_counter.load(Ordering::SeqCst)
    }

    pub fn advance_sequence(&self) -> u64 {
        let seq = self.sequence_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.event_bus
            .clone()
            .send(SchedulerEvent::SequenceAdvance { sequence: seq })
            .ok();
        seq
    }
}

/// Hybrid scheduler combining periodic and event-driven execution
pub struct HybridScheduler {
    periodic: PeriodicScheduler,
    event_driven: EventDrivenScheduler,
    event_bus: Arc<broadcast::Sender<SchedulerEvent>>,
    last_tick: AtomicU64,
    pending_events: VecDeque<SchedulerEvent>,
    config: SchedulerConfig,
    active: bool,
}

impl HybridScheduler {
    pub fn new(
        interval_ms: u64,
        event_bus: Arc<broadcast::Sender<SchedulerEvent>>,
        config: SchedulerConfig,
    ) -> Self {
        Self {
            periodic: PeriodicScheduler::new(interval_ms),
            event_driven: EventDrivenScheduler::new(event_bus.clone(), config.enable_sequence),
            event_bus,
            last_tick: AtomicU64::new(0),
            pending_events: VecDeque::new(),
            config,
            active: false,
        }
    }

    pub async fn run(&mut self) {
        self.active = true;
        info!(
            "Hybrid scheduler starting with interval {}ms",
            self.config.default_interval_ms
        );

        let mut ticker = interval(Duration::from_millis(self.config.default_interval_ms));
        let mut rx = self.event_bus.subscribe();

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if !self.active {
                        break;
                    }

                    // Emit timer tick event
                    let _ = self.event_bus.send(SchedulerEvent::TimerTick);

                    // Process pending events
                    self.process_pending_events().await;
                }
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            // Buffer the event for processing
                            if self.pending_events.len() < self.config.max_batch_size {
                                self.pending_events.push_back(event);
                            } else {
                                // Drop oldest events if buffer is full (backpressure)
                                let dropped = self.pending_events.pop_front();
                                if let Some(dropped) = dropped {
                                    warn!("Dropped event due to buffer full: {:?}", dropped);
                                }
                                self.pending_events.push_back(event);
                            }
                        }
                        Err(_) => {
                            self.active = false;
                            break;
                        }
                    }
                }
            }
        }

        // Drain remaining events
        while let Some(event) = self.pending_events.pop_front() {
            debug!("Draining event: {:?}", event);
        }

        info!("Hybrid scheduler stopped");
    }

    async fn process_pending_events(&mut self) {
        let _now = Instant::now();
        let mut processed = 0;

        while let Some(event) = self.pending_events.pop_front() {
            match &event {
                SchedulerEvent::TimerTick => {
                    // Timer ticks are handled by the ticker itself
                }
                SchedulerEvent::DataUpdate {
                    instrument_id,
                    timestamp_ms,
                } => {
                    // In real implementation, this would trigger data processing
                    debug!(
                        "Processing data update: {} @ {:?}",
                        instrument_id,
                        Duration::from_millis(*timestamp_ms as u64)
                    );
                }
                SchedulerEvent::Signal { strategy_id, .. } => {
                    // In real implementation, this would trigger strategy evaluation
                    debug!("Processing signal from strategy: {}", strategy_id);
                }
                SchedulerEvent::Error { source, error } => {
                    error!("Processing error from {}: {}", source, error);
                }
                SchedulerEvent::SequenceAdvance { sequence: _ } => {
                    // Sequence already handled by event_driven component
                }
                SchedulerEvent::Shutdown => {
                    self.active = false;
                    return;
                }
            }
            processed += 1;
        }

        if processed > 0 {
            debug!("Processed {} pending events", processed);
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn get_last_tick(&self) -> u64 {
        self.last_tick.load(Ordering::SeqCst)
    }

    pub fn set_last_tick(&self, tick: u64) {
        self.last_tick.store(tick, Ordering::SeqCst);
    }

    pub fn get_pending_count(&self) -> usize {
        self.pending_events.len()
    }

    pub fn get_config(&self) -> &SchedulerConfig {
        &self.config
    }
}

/// Backpressure handler for rate limiting
pub struct BackpressureHandler {
    rate_limit: u64, // events per second
    last_reset: Instant,
    count: AtomicU64,
    active: bool,
}

impl BackpressureHandler {
    pub fn new(rate_limit: u64) -> Self {
        Self {
            rate_limit,
            last_reset: Instant::now(),
            count: AtomicU64::new(0),
            active: true,
        }
    }

    pub async fn allow(&mut self) -> bool {
        if !self.active {
            return false;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_reset);

        // Reset counter if rate window passed
        if elapsed.as_secs() >= 1 {
            self.count.store(0, Ordering::SeqCst);
            self.last_reset = now;
        }

        let current = self.count.load(Ordering::SeqCst);
        let capacity = self.rate_limit.saturating_sub(current);

        if capacity > 0 {
            self.count.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            // Backpressure: wait briefly then retry
            sleep(Duration::from_millis(1)).await;
            false
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hybrid_scheduler_basic() {
        let (tx, rx) = broadcast::channel(100);
        let _keepalive = tx.subscribe();
        drop(rx);
        let bus = Arc::new(tx);
        let mut scheduler = HybridScheduler::new(100, bus.clone(), SchedulerConfig::default());

        // Run for a short time
        let handle = tokio::spawn(async move {
            scheduler.run().await;
        });

        // Give it time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send some events
        for i in 0..10 {
            let _ = bus.send(SchedulerEvent::DataUpdate {
                instrument_id: "BTC/USDT".to_string(),
                timestamp_ms: 1712345678000 + i as i64,
            });
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        handle.abort();
    }

    #[tokio::test]
    async fn test_backpressure_handler() {
        let mut handler = BackpressureHandler::new(10); // 10 events/sec

        // Should allow first 10
        for i in 0..10 {
            assert!(handler.allow().await, "Should allow event {}", i);
        }

        // Should block 11th
        let started = Instant::now();
        let result = handler.allow().await;
        let elapsed = started.elapsed();

        assert!(!result, "Should block event 10");
        assert!(elapsed >= Duration::from_millis(1), "Should have waited");

        // 进入下一秒后计数器重置，才应再次放行
        sleep(Duration::from_millis(1100)).await;
        assert!(handler.allow().await);
    }

    #[tokio::test]
    async fn test_event_sequence() {
        let (tx, rx) = broadcast::channel(100);
        let _keepalive = tx.subscribe();
        drop(rx);
        let scheduler = EventDrivenScheduler::new(Arc::new(tx), true);

        let seq1 = scheduler.get_sequence();
        let seq2 = scheduler.advance_sequence();

        assert!(seq2 > seq1, "Sequence should advance");
    }
}
