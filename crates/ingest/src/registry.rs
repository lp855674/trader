use std::sync::Arc;

use domain::Venue;

use crate::adapter::IngestAdapter;

#[derive(Clone, Default)]
pub struct IngestRegistry {
    adapters: Vec<Arc<dyn IngestAdapter>>,
}

impl IngestRegistry {
    pub fn register(&mut self, adapter: Arc<dyn IngestAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn for_venue(&self, venue: Venue) -> impl Iterator<Item = &Arc<dyn IngestAdapter>> {
        self.adapters
            .iter()
            .filter(move |adapter| adapter.venue() == venue)
    }

    pub fn adapter_for_venue(&self, venue: Venue) -> Option<Arc<dyn IngestAdapter>> {
        self.for_venue(venue).next().cloned()
    }
}
