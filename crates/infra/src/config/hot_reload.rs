/// In-process hot-reload watcher that compares configs and signals changes.

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigDiff {
    pub changed_keys: Vec<String>,
    pub added_keys: Vec<String>,
    pub removed_keys: Vec<String>,
}

impl ConfigDiff {
    pub fn is_empty(&self) -> bool {
        self.changed_keys.is_empty() && self.added_keys.is_empty() && self.removed_keys.is_empty()
    }
}

pub struct HotReloadWatcher {
    current: serde_json::Value,
    reload_count: u64,
}

impl HotReloadWatcher {
    pub fn new(initial: serde_json::Value) -> Self {
        Self {
            current: initial,
            reload_count: 0,
        }
    }

    /// Compare new config against current. Returns diff and applies if non-empty.
    pub fn check_and_apply(&mut self, new_config: serde_json::Value) -> ConfigDiff {
        let diff = diff_configs(&self.current, &new_config);
        if !diff.is_empty() {
            self.current = new_config;
            self.reload_count += 1;
        }
        diff
    }

    pub fn current(&self) -> &serde_json::Value {
        &self.current
    }

    pub fn reload_count(&self) -> u64 {
        self.reload_count
    }
}

fn diff_configs(old: &serde_json::Value, new: &serde_json::Value) -> ConfigDiff {
    let old_obj = old.as_object();
    let new_obj = new.as_object();
    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();

    if let (Some(old_map), Some(new_map)) = (old_obj, new_obj) {
        for (k, ov) in old_map {
            match new_map.get(k) {
                None => removed.push(k.clone()),
                Some(nv) if nv != ov => changed.push(k.clone()),
                _ => {}
            }
        }
        for k in new_map.keys() {
            if !old_map.contains_key(k) {
                added.push(k.clone());
            }
        }
    }

    ConfigDiff {
        changed_keys: changed,
        added_keys: added,
        removed_keys: removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_changed_key() {
        let mut w = HotReloadWatcher::new(serde_json::json!({"timeout": 1000}));
        let diff = w.check_and_apply(serde_json::json!({"timeout": 2000}));
        assert!(diff.changed_keys.contains(&"timeout".to_string()));
        assert_eq!(w.reload_count(), 1);
    }

    #[test]
    fn no_change_no_reload() {
        let mut w = HotReloadWatcher::new(serde_json::json!({"venue": "paper"}));
        let diff = w.check_and_apply(serde_json::json!({"venue": "paper"}));
        assert!(diff.is_empty());
        assert_eq!(w.reload_count(), 0);
    }

    #[test]
    fn detects_added_and_removed() {
        let mut w = HotReloadWatcher::new(serde_json::json!({"a": 1, "b": 2}));
        let diff = w.check_and_apply(serde_json::json!({"a": 1, "c": 3}));
        assert!(diff.removed_keys.contains(&"b".to_string()));
        assert!(diff.added_keys.contains(&"c".to_string()));
    }
}
