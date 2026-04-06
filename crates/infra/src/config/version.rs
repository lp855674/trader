/// Config versioning with rollback support.

#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub version: u32,
    pub config: serde_json::Value,
    pub timestamp_ms: u64,
    pub author: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct ConfigVersioning {
    history: Vec<ConfigSnapshot>,
    current_version: u32,
}

impl ConfigVersioning {
    pub fn new(initial: serde_json::Value, author: &str) -> Self {
        Self {
            history: vec![ConfigSnapshot {
                version: 1,
                config: initial,
                timestamp_ms: now_ms(),
                author: author.to_string(),
            }],
            current_version: 1,
        }
    }

    pub fn commit(&mut self, config: serde_json::Value, author: &str) -> u32 {
        self.current_version += 1;
        self.history.push(ConfigSnapshot {
            version: self.current_version,
            config,
            timestamp_ms: now_ms(),
            author: author.to_string(),
        });
        self.current_version
    }

    pub fn rollback(&mut self) -> Option<&ConfigSnapshot> {
        if self.history.len() > 1 {
            self.history.pop();
            self.current_version = self.history.last().map(|s| s.version).unwrap_or(1);
            self.history.last()
        } else {
            None
        }
    }

    pub fn current(&self) -> &ConfigSnapshot {
        self.history.last().expect("always has at least one snapshot")
    }

    pub fn version_count(&self) -> usize {
        self.history.len()
    }

    pub fn get_version(&self, version: u32) -> Option<&ConfigSnapshot> {
        self.history.iter().find(|s| s.version == version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_increments_version() {
        let mut cv = ConfigVersioning::new(serde_json::json!({"v": 1}), "alice");
        let v = cv.commit(serde_json::json!({"v": 2}), "bob");
        assert_eq!(v, 2);
        assert_eq!(cv.version_count(), 2);
    }

    #[test]
    fn rollback_removes_latest() {
        let mut cv = ConfigVersioning::new(serde_json::json!({"v": 1}), "alice");
        cv.commit(serde_json::json!({"v": 2}), "bob");
        let rolled = cv.rollback().unwrap();
        assert_eq!(rolled.version, 1);
        assert_eq!(cv.version_count(), 1);
    }

    #[test]
    fn rollback_at_initial_returns_none() {
        let mut cv = ConfigVersioning::new(serde_json::json!({}), "alice");
        assert!(cv.rollback().is_none());
    }
}
