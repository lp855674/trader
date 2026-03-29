use crate::venue::Venue;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InstrumentId {
    pub venue: Venue,
    pub symbol: String,
}
