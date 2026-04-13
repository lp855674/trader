// Strategy Registry & Factory
//
// Provides hot-swappable strategy registration, versioning, rollback, and
// a factory pattern for creating strategies from config.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use thiserror::Error;

use super::r#trait::{Strategy, StrategyError};

// ─── RegistryError ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Strategy not found: {0}")]
    NotFound(String),

    #[error("Invalid config: {0}")]
    InvalidConfig(String),

    #[error("Version not found: {0}")]
    VersionNotFound(u32),

    #[error("Build failed: {0}")]
    BuildFailed(String),
}

// ─── StrategyVersion ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StrategyVersion {
    pub version: u32,
    pub name: String,
    pub created_at_ms: i64,
}

// ─── RegistryEntry ────────────────────────────────────────────────────────────

pub struct RegistryEntry {
    pub current: Arc<dyn Strategy>,
    pub versions: Vec<(StrategyVersion, Arc<dyn Strategy>)>,
    pub config: serde_json::Value,
}

// ─── StrategyRegistry ────────────────────────────────────────────────────────

/// Thread-safe registry mapping strategy IDs to their current and historical
/// versions.
pub struct StrategyRegistry {
    entries: Mutex<HashMap<String, RegistryEntry>>,
}

impl StrategyRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Register a new strategy under `id`. If an entry already exists it is
    /// overwritten.
    pub fn register(&self, id: String, strategy: Arc<dyn Strategy>, config: serde_json::Value) {
        let mut map = self.entries.lock().unwrap();
        map.insert(
            id,
            RegistryEntry {
                current: strategy,
                versions: Vec::new(),
                config,
            },
        );
    }

    /// Retrieve the current strategy for `id`, if registered.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Strategy>> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .map(|e| Arc::clone(&e.current))
    }

    /// Atomically replace the strategy for `id` with `new_strategy`, pushing
    /// the old version into the version history.
    pub fn hot_swap(
        &self,
        id: &str,
        new_strategy: Arc<dyn Strategy>,
        new_config: serde_json::Value,
    ) -> Result<(), RegistryError> {
        let mut map = self.entries.lock().unwrap();
        let entry = map
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.to_owned()))?;

        let old_version = StrategyVersion {
            version: entry.current.version(),
            name: entry.current.name().to_owned(),
            created_at_ms: 0,
        };
        let old_strategy = Arc::clone(&entry.current);
        entry.versions.push((old_version, old_strategy));

        entry.current = new_strategy;
        entry.config = new_config;
        Ok(())
    }

    /// Restore the strategy at the given version number.
    pub fn rollback(&self, id: &str, version: u32) -> Result<(), RegistryError> {
        let mut map = self.entries.lock().unwrap();
        let entry = map
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.to_owned()))?;

        let pos = entry
            .versions
            .iter()
            .position(|(sv, _)| sv.version == version)
            .ok_or(RegistryError::VersionNotFound(version))?;

        let (_, restored) = entry.versions.remove(pos);
        entry.current = restored;
        Ok(())
    }

    /// Basic JSON schema validation: ensure all fields listed in
    /// `schema["required"]` exist as top-level keys in `config`.
    pub fn validate_config(
        config: &serde_json::Value,
        schema: &serde_json::Value,
    ) -> Result<(), RegistryError> {
        let required = match schema.get("required") {
            Some(serde_json::Value::Array(arr)) => arr,
            Some(_) => {
                return Err(RegistryError::InvalidConfig(
                    "schema `required` field must be an array".into(),
                ));
            }
            None => return Ok(()), // no required fields
        };

        for field in required {
            let key = field.as_str().ok_or_else(|| {
                RegistryError::InvalidConfig("required entry must be a string".into())
            })?;
            if config.get(key).is_none() {
                return Err(RegistryError::InvalidConfig(format!(
                    "missing required field: {key}"
                )));
            }
        }
        Ok(())
    }

    /// List all registered strategy IDs.
    pub fn list(&self) -> Vec<String> {
        self.entries.lock().unwrap().keys().cloned().collect()
    }

    /// Retrieve the current config for `id`, if registered.
    pub fn get_config(&self, id: &str) -> Option<serde_json::Value> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .map(|e| e.config.clone())
    }
}

impl Default for StrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── StrategyFactory ─────────────────────────────────────────────────────────

type BuilderFn =
    Box<dyn Fn(serde_json::Value) -> Result<Arc<dyn Strategy>, RegistryError> + Send + Sync>;

/// Factory that creates strategy instances from a type name and config.
pub struct StrategyFactory {
    builders: HashMap<String, BuilderFn>,
}

impl StrategyFactory {
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
        }
    }

    /// Register a builder function for the given type name.
    pub fn register_builder(
        &mut self,
        type_name: String,
        builder: impl Fn(serde_json::Value) -> Result<Arc<dyn Strategy>, RegistryError>
        + Send
        + Sync
        + 'static,
    ) {
        self.builders.insert(type_name, Box::new(builder));
    }

    /// Create a strategy instance from the type name and config.
    pub fn create(
        &self,
        type_name: &str,
        config: serde_json::Value,
    ) -> Result<Arc<dyn Strategy>, RegistryError> {
        let builder = self
            .builders
            .get(type_name)
            .ok_or_else(|| RegistryError::NotFound(type_name.to_owned()))?;
        builder(config)
    }
}

impl Default for StrategyFactory {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use domain::{InstrumentId, Venue};

    use super::*;
    use crate::core::r#trait::{Signal, StrategyContext, StrategyError};

    // ── helper strategy ───────────────────────────────────────────────────────

    struct Stub {
        id: String,
        ver: u32,
    }

    impl Stub {
        fn new(id: &str, ver: u32) -> Arc<Self> {
            Arc::new(Self { id: id.into(), ver })
        }
    }

    impl Strategy for Stub {
        fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
            Ok(Some(Signal::new(
                ctx.instrument.clone(),
                domain::Side::Buy,
                1.0,
                None,
                ctx.ts_ms,
                self.id.clone(),
                HashMap::new(),
            )))
        }

        fn name(&self) -> &str {
            &self.id
        }

        fn version(&self) -> u32 {
            self.ver
        }
    }

    fn ctx() -> StrategyContext {
        StrategyContext::new(InstrumentId::new(Venue::Crypto, "BTC"), 0)
    }

    // ── register & get ───────────────────────────────────────────────────────

    #[test]
    fn register_and_get() {
        let reg = StrategyRegistry::new();
        reg.register("s1".into(), Stub::new("s1", 1), serde_json::json!({}));
        let s = reg.get("s1").unwrap();
        assert_eq!(s.name(), "s1");
    }

    #[test]
    fn get_returns_none_for_unknown() {
        let reg = StrategyRegistry::new();
        assert!(reg.get("unknown").is_none());
    }

    // ── hot_swap ─────────────────────────────────────────────────────────────

    #[test]
    fn hot_swap_replaces_strategy() {
        let reg = StrategyRegistry::new();
        reg.register("s1".into(), Stub::new("s1-v1", 1), serde_json::json!({}));
        reg.hot_swap("s1", Stub::new("s1-v2", 2), serde_json::json!({"v": 2}))
            .unwrap();
        let s = reg.get("s1").unwrap();
        assert_eq!(s.name(), "s1-v2");
    }

    #[test]
    fn hot_swap_unknown_id_errors() {
        let reg = StrategyRegistry::new();
        let result = reg.hot_swap("nope", Stub::new("x", 1), serde_json::json!({}));
        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    // ── rollback ─────────────────────────────────────────────────────────────

    #[test]
    fn rollback_restores_previous_version() {
        let reg = StrategyRegistry::new();
        reg.register("s1".into(), Stub::new("s1-v1", 1), serde_json::json!({}));
        reg.hot_swap("s1", Stub::new("s1-v2", 2), serde_json::json!({}))
            .unwrap();
        // current is v2; version history has v1
        reg.rollback("s1", 1).unwrap();
        let s = reg.get("s1").unwrap();
        assert_eq!(s.name(), "s1-v1");
    }

    #[test]
    fn rollback_missing_version_errors() {
        let reg = StrategyRegistry::new();
        reg.register("s1".into(), Stub::new("s1", 1), serde_json::json!({}));
        assert!(matches!(
            reg.rollback("s1", 99),
            Err(RegistryError::VersionNotFound(99))
        ));
    }

    // ── validate_config ──────────────────────────────────────────────────────

    #[test]
    fn validate_config_passes_when_fields_present() {
        let config = serde_json::json!({"threshold": 0.5, "window": 20});
        let schema = serde_json::json!({"required": ["threshold", "window"]});
        assert!(StrategyRegistry::validate_config(&config, &schema).is_ok());
    }

    #[test]
    fn validate_config_fails_when_field_missing() {
        let config = serde_json::json!({"threshold": 0.5});
        let schema = serde_json::json!({"required": ["threshold", "window"]});
        assert!(matches!(
            StrategyRegistry::validate_config(&config, &schema),
            Err(RegistryError::InvalidConfig(_))
        ));
    }

    // ── list ─────────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_all_ids() {
        let reg = StrategyRegistry::new();
        reg.register("a".into(), Stub::new("a", 1), serde_json::json!({}));
        reg.register("b".into(), Stub::new("b", 1), serde_json::json!({}));
        let mut ids = reg.list();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);
    }

    // ── factory ──────────────────────────────────────────────────────────────

    #[test]
    fn factory_creates_from_type_name() {
        let mut factory = StrategyFactory::new();
        factory.register_builder("stub".into(), |_config| Ok(Stub::new("built", 1)));
        let s = factory.create("stub", serde_json::json!({})).unwrap();
        assert_eq!(s.name(), "built");
    }

    #[test]
    fn factory_unknown_type_errors() {
        let factory = StrategyFactory::new();
        assert!(matches!(
            factory.create("unknown", serde_json::json!({})),
            Err(RegistryError::NotFound(_))
        ));
    }
}
