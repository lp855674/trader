use crate::core::DataItem;

// ── BatchConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub flush_interval_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            flush_interval_ms: 5000,
        }
    }
}

// ── BatchProcessor ────────────────────────────────────────────────────────────

pub struct BatchProcessor {
    pub pending: Vec<DataItem>,
    pub config: BatchConfig,
    pub processed_count: u64,
}

impl BatchProcessor {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            pending: Vec::new(),
            config,
            processed_count: 0,
        }
    }

    pub fn push(&mut self, item: DataItem) {
        self.pending.push(item);
    }

    pub fn flush(&mut self) -> Vec<DataItem> {
        let items = std::mem::take(&mut self.pending);
        self.processed_count += items.len() as u64;
        items
    }

    pub fn should_flush(&self, ts_ms: i64, last_flush_ms: i64) -> bool {
        if self.pending.len() >= self.config.max_batch_size {
            return true;
        }
        let elapsed = (ts_ms - last_flush_ms).max(0) as u64;
        elapsed >= self.config.flush_interval_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::NormalizedBar;

    fn bar_item(ts_ms: i64) -> DataItem {
        DataItem::Bar(NormalizedBar {
            ts_ms,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 1.0,
        })
    }

    #[test]
    fn batch_flush_on_size() {
        let config = BatchConfig {
            max_batch_size: 3,
            flush_interval_ms: 10_000,
        };
        let mut proc = BatchProcessor::new(config);
        proc.push(bar_item(1));
        proc.push(bar_item(2));
        assert!(!proc.should_flush(5000, 0));
        proc.push(bar_item(3));
        assert!(proc.should_flush(5000, 0));
    }

    #[test]
    fn batch_flush_on_interval() {
        let config = BatchConfig {
            max_batch_size: 1000,
            flush_interval_ms: 5000,
        };
        let proc = BatchProcessor::new(config);
        assert!(proc.should_flush(6000, 0));
        assert!(!proc.should_flush(4000, 0));
    }

    #[test]
    fn flush_returns_items() {
        let mut proc = BatchProcessor::new(BatchConfig::default());
        proc.push(bar_item(1));
        proc.push(bar_item(2));
        let items = proc.flush();
        assert_eq!(items.len(), 2);
        assert_eq!(proc.pending.len(), 0);
        assert_eq!(proc.processed_count, 2);
    }
}
