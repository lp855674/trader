use serde::{Deserialize, Serialize};

/// Serializable state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub version: u64,
    pub timestamp_ms: u64,
    pub payload: serde_json::Value,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct StateSaver {
    checkpoints: Vec<StateSnapshot>,
    next_version: u64,
}

impl StateSaver {
    pub fn new() -> Self {
        Self {
            checkpoints: Vec::new(),
            next_version: 1,
        }
    }

    /// Save state, returning the version number assigned.
    pub fn save(&mut self, state: serde_json::Value) -> u64 {
        let version = self.next_version;
        self.next_version += 1;
        self.checkpoints.push(StateSnapshot {
            version,
            timestamp_ms: now_ms(),
            payload: state,
        });
        version
    }

    /// Serialize latest snapshot to JSON bytes.
    pub fn serialize_latest(&self) -> Option<Vec<u8>> {
        self.checkpoints
            .last()
            .and_then(|s| serde_json::to_vec(s).ok())
    }

    /// Restore from JSON bytes. Returns the snapshot if valid.
    pub fn restore(bytes: &[u8]) -> Result<StateSnapshot, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn latest(&self) -> Option<&StateSnapshot> {
        self.checkpoints.last()
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    /// Validate that payload is a non-null object (basic check).
    pub fn validate(snapshot: &StateSnapshot) -> bool {
        snapshot.payload.is_object() || snapshot.payload.is_array()
    }
}

impl Default for StateSaver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_retrieve() {
        let mut ss = StateSaver::new();
        let v = ss.save(serde_json::json!({"orders": 5}));
        assert_eq!(v, 1);
        assert_eq!(ss.checkpoint_count(), 1);
        let snap = ss.latest().unwrap();
        assert_eq!(snap.payload["orders"], 5);
    }

    #[test]
    fn serialize_restore_roundtrip() {
        let mut ss = StateSaver::new();
        ss.save(serde_json::json!({"positions": {"AAPL": 100}}));
        let bytes = ss.serialize_latest().unwrap();
        let restored = StateSaver::restore(&bytes).unwrap();
        assert_eq!(restored.payload["positions"]["AAPL"], 100);
    }

    #[test]
    fn validate_rejects_null() {
        let snap = StateSnapshot {
            version: 1,
            timestamp_ms: 0,
            payload: serde_json::Value::Null,
        };
        assert!(!StateSaver::validate(&snap));
    }
}
