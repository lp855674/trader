//! Core domain types for quantd.

pub mod ids;
pub mod trading;
pub mod venue;

pub use ids::InstrumentId;
pub use trading::{AccountMode, NormalizedBar, OrderIntent, Side, Signal};
pub use venue::Venue;

#[cfg(test)]
mod tests {
    use super::{InstrumentId, Venue};

    #[test]
    fn instrument_id_roundtrip_json() {
        let id = InstrumentId {
            venue: Venue::Crypto,
            symbol: "BTC-USD".to_string(),
        };
        let json = serde_json::to_string(&id).expect("serialize");
        let back: InstrumentId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }
}
