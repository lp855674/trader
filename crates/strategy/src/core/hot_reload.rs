// Hot reload mechanism using tokio::sync::watch for config-push based reloads.
//
// No file-system watcher is used; instead callers push `ReloadConfig` values
// via `HotReloadHandle` and `HotReloadWatcher` picks them up and applies them.

use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::watch;

use super::registry::{RegistryError, StrategyFactory, StrategyRegistry};

// ─── HotReloadError ───────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum HotReloadError {
    #[error("registry error: {0}")]
    RegistryError(#[from] RegistryError),

    #[error("build error: {0}")]
    BuildError(String),

    #[error("watcher channel closed")]
    WatcherClosed,
}

// ─── ReloadConfig ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReloadConfig {
    pub strategy_id: String,
    pub new_config: serde_json::Value,
    pub version: u32,
}

// ─── HotReloadHandle ─────────────────────────────────────────────────────────

/// Sender side of the hot-reload channel.  Callers hold this to broadcast
/// reload signals to all connected watchers.
pub struct HotReloadHandle {
    tx: watch::Sender<Option<ReloadConfig>>,
}

impl HotReloadHandle {
    /// Create a new handle/watcher pair.
    pub fn new() -> (Self, watch::Receiver<Option<ReloadConfig>>) {
        let (tx, rx) = watch::channel(None);
        (Self { tx }, rx)
    }

    /// Broadcast a new reload config to all watchers.
    pub fn push_reload(&self, config: ReloadConfig) {
        // Ignore send errors (no receivers is fine).
        let _ = self.tx.send(Some(config));
    }
}

// ─── ConfigDiff ──────────────────────────────────────────────────────────────

/// Structural diff between two JSON objects.
#[derive(Debug, Clone, Default)]
pub struct ConfigDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

impl ConfigDiff {
    /// Compute field-level diff between two JSON objects.
    ///
    /// Only top-level keys are compared.  If either value is not an object the
    /// diff treats the entire value as a single "changed" entry under the key
    /// `"<root>"`.
    pub fn diff(old: &serde_json::Value, new: &serde_json::Value) -> ConfigDiff {
        let old_obj = old.as_object();
        let new_obj = new.as_object();

        match (old_obj, new_obj) {
            (Some(old_map), Some(new_map)) => {
                let mut added = Vec::new();
                let mut removed = Vec::new();
                let mut changed = Vec::new();

                for key in new_map.keys() {
                    if !old_map.contains_key(key) {
                        added.push(key.clone());
                    } else if old_map[key] != new_map[key] {
                        changed.push(key.clone());
                    }
                }
                for key in old_map.keys() {
                    if !new_map.contains_key(key) {
                        removed.push(key.clone());
                    }
                }

                added.sort();
                removed.sort();
                changed.sort();

                ConfigDiff {
                    added,
                    removed,
                    changed,
                }
            }
            _ => {
                // Non-object values: treat as changed root if different
                if old == new {
                    ConfigDiff::default()
                } else {
                    ConfigDiff {
                        changed: vec!["<root>".into()],
                        ..Default::default()
                    }
                }
            }
        }
    }

    /// Returns `true` if there are no differences.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

// ─── HotReloadWatcher ────────────────────────────────────────────────────────

/// Watches for reload signals and applies them to a `StrategyRegistry`.
pub struct HotReloadWatcher {
    rx: watch::Receiver<Option<ReloadConfig>>,
    registry: Arc<Mutex<StrategyRegistry>>,
}

impl HotReloadWatcher {
    pub fn new(
        rx: watch::Receiver<Option<ReloadConfig>>,
        registry: Arc<Mutex<StrategyRegistry>>,
    ) -> Self {
        Self { rx, registry }
    }

    /// Wait for the next reload signal, diff old vs new config, and apply it
    /// if the config actually changed.
    pub async fn watch_and_apply(
        &mut self,
        factory: &StrategyFactory,
    ) -> Result<(), HotReloadError> {
        self.rx
            .changed()
            .await
            .map_err(|_| HotReloadError::WatcherClosed)?;

        let reload = {
            let guard = self.rx.borrow();
            match &*guard {
                None => return Ok(()),
                Some(rc) => rc.clone(),
            }
        };

        // Get old config from registry
        let old_config = {
            let reg = self.registry.lock().unwrap();
            // Access the entry config — we use list + internal check
            reg.get_config(&reload.strategy_id)
        };

        let diff = ConfigDiff::diff(
            old_config.as_ref().unwrap_or(&serde_json::Value::Null),
            &reload.new_config,
        );

        if diff.is_empty() {
            return Ok(()); // no changes — skip
        }

        let new_strategy = factory
            .create(&reload.strategy_id, reload.new_config.clone())
            .map_err(|e| HotReloadError::BuildError(e.to_string()))?;

        {
            let reg = self.registry.lock().unwrap();
            reg.hot_swap(&reload.strategy_id, new_strategy, reload.new_config)?;
        }

        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ConfigDiff::diff ─────────────────────────────────────────────────────

    #[test]
    fn diff_detects_added_field() {
        let old = serde_json::json!({"a": 1});
        let new = serde_json::json!({"a": 1, "b": 2});
        let d = ConfigDiff::diff(&old, &new);
        assert_eq!(d.added, vec!["b"]);
        assert!(d.removed.is_empty());
        assert!(d.changed.is_empty());
    }

    #[test]
    fn diff_detects_removed_field() {
        let old = serde_json::json!({"a": 1, "b": 2});
        let new = serde_json::json!({"a": 1});
        let d = ConfigDiff::diff(&old, &new);
        assert_eq!(d.removed, vec!["b"]);
        assert!(d.added.is_empty());
        assert!(d.changed.is_empty());
    }

    #[test]
    fn diff_detects_changed_field() {
        let old = serde_json::json!({"a": 1});
        let new = serde_json::json!({"a": 2});
        let d = ConfigDiff::diff(&old, &new);
        assert_eq!(d.changed, vec!["a"]);
    }

    #[test]
    fn diff_is_empty_for_identical_configs() {
        let cfg = serde_json::json!({"x": 42});
        let d = ConfigDiff::diff(&cfg, &cfg);
        assert!(d.is_empty());
    }

    #[test]
    fn diff_non_object_values_changed() {
        let d = ConfigDiff::diff(&serde_json::json!(1), &serde_json::json!(2));
        assert_eq!(d.changed, vec!["<root>"]);
    }

    #[test]
    fn diff_non_object_values_same() {
        let d = ConfigDiff::diff(&serde_json::json!(42), &serde_json::json!(42));
        assert!(d.is_empty());
    }

    // ── push + watch_and_apply ────────────────────────────────────────────────

    #[tokio::test]
    async fn push_and_watch_applies_config() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        use domain::{InstrumentId, Side, Venue};

        use crate::core::{
            registry::{RegistryError, StrategyFactory, StrategyRegistry},
            r#trait::{Signal, Strategy, StrategyContext, StrategyError},
        };

        // A stub strategy whose name encodes its config value
        struct ConfigStub(String);
        impl Strategy for ConfigStub {
            fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
                Ok(Some(Signal::new(
                    ctx.instrument.clone(),
                    Side::Buy,
                    1.0,
                    None,
                    ctx.ts_ms,
                    self.0.clone(),
                    HashMap::new(),
                )))
            }
            fn name(&self) -> &str {
                &self.0
            }
        }

        // Registry with one entry
        let registry = Arc::new(Mutex::new(StrategyRegistry::new()));
        {
            let reg = registry.lock().unwrap();
            reg.register(
                "my_strat".into(),
                Arc::new(ConfigStub("v1".into())),
                serde_json::json!({"version": 1}),
            );
        }

        // Factory that creates ConfigStub from config
        let mut factory = StrategyFactory::new();
        factory.register_builder("my_strat".into(), |config| {
            let ver = config["version"].as_u64().unwrap_or(0).to_string();
            Ok(Arc::new(ConfigStub(format!("v{ver}"))) as Arc<dyn Strategy>)
        });

        let (handle, rx) = HotReloadHandle::new();
        let mut watcher = HotReloadWatcher::new(rx, Arc::clone(&registry));

        // Push a reload
        handle.push_reload(ReloadConfig {
            strategy_id: "my_strat".into(),
            new_config: serde_json::json!({"version": 2}),
            version: 2,
        });

        watcher.watch_and_apply(&factory).await.unwrap();

        let s = registry.lock().unwrap().get("my_strat").unwrap();
        assert_eq!(s.name(), "v2");
    }

    #[tokio::test]
    async fn watch_skips_when_config_unchanged() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        use domain::{InstrumentId, Side, Venue};

        use crate::core::{
            registry::{StrategyFactory, StrategyRegistry},
            r#trait::{Signal, Strategy, StrategyContext, StrategyError},
        };

        struct Stub;
        impl Strategy for Stub {
            fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
                Ok(Some(Signal::new(
                    ctx.instrument.clone(),
                    Side::Buy,
                    1.0,
                    None,
                    ctx.ts_ms,
                    "stub".into(),
                    HashMap::new(),
                )))
            }
            fn name(&self) -> &str {
                "stub"
            }
        }

        let registry = Arc::new(Mutex::new(StrategyRegistry::new()));
        {
            let reg = registry.lock().unwrap();
            reg.register("s".into(), Arc::new(Stub), serde_json::json!({"x": 1}));
        }

        let factory = StrategyFactory::new(); // no builders needed — should not be called

        let (handle, rx) = HotReloadHandle::new();
        let mut watcher = HotReloadWatcher::new(rx, Arc::clone(&registry));

        handle.push_reload(ReloadConfig {
            strategy_id: "s".into(),
            new_config: serde_json::json!({"x": 1}), // same config
            version: 1,
        });

        // Should return Ok without calling factory.create
        watcher.watch_and_apply(&factory).await.unwrap();

        // Strategy should still be the original stub
        let s = registry.lock().unwrap().get("s").unwrap();
        assert_eq!(s.name(), "stub");
    }
}
