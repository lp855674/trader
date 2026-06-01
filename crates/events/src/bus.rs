use crate::{envelope, AnyEventEnvelope, SignalEvent, TraderEvent};
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Debug, Error)]
pub enum EventBusError {
    #[error("event bus has no active receivers")]
    NoReceivers,
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AnyEventEnvelope>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _receiver) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AnyEventEnvelope> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: AnyEventEnvelope) -> Result<(), EventBusError> {
        self.sender
            .send(event)
            .map(|_| ())
            .map_err(|_| EventBusError::NoReceivers)
    }

    pub fn publish_signal(&self, signal: SignalEvent) -> Result<(), EventBusError> {
        self.publish(envelope("strategy", TraderEvent::Signal(signal)))
    }
}
