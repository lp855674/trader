use crate::align::GapSpec;
use crate::core::DataItem;
use domain::NormalizedBar;

// ── CallbackEvent ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CallbackEvent {
    OnBar(NormalizedBar),
    OnTick(DataItem),
    OnGap(GapSpec),
    OnComplete { total_items: u64 },
}

// ── ReplayCallback trait ──────────────────────────────────────────────────────

pub trait ReplayCallback: Send {
    fn on_event(&mut self, event: CallbackEvent);
}

// ── CallbackManager ───────────────────────────────────────────────────────────

pub struct CallbackManager {
    pub callbacks: Vec<Box<dyn ReplayCallback>>,
}

impl CallbackManager {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    pub fn add(&mut self, cb: Box<dyn ReplayCallback>) {
        self.callbacks.push(cb);
    }

    pub fn fire(&mut self, event: CallbackEvent) {
        for cb in &mut self.callbacks {
            cb.on_event(event.clone());
        }
    }
}

impl Default for CallbackManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct CountingCallback {
        count: Arc<Mutex<u32>>,
    }

    impl ReplayCallback for CountingCallback {
        fn on_event(&mut self, _event: CallbackEvent) {
            *self.count.lock().unwrap() += 1;
        }
    }

    #[test]
    fn callback_manager_fires_all() {
        let count1 = Arc::new(Mutex::new(0u32));
        let count2 = Arc::new(Mutex::new(0u32));

        let mut mgr = CallbackManager::new();
        mgr.add(Box::new(CountingCallback {
            count: count1.clone(),
        }));
        mgr.add(Box::new(CountingCallback {
            count: count2.clone(),
        }));

        let bar = NormalizedBar {
            ts_ms: 1000,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 1.0,
        };
        mgr.fire(CallbackEvent::OnBar(bar.clone()));
        mgr.fire(CallbackEvent::OnBar(bar));

        assert_eq!(*count1.lock().unwrap(), 2);
        assert_eq!(*count2.lock().unwrap(), 2);
    }
}
