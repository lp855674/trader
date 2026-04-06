use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceConfig {
    pub name: String,
    pub source_type: String,
    pub params: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub l1_capacity_mb: u64,
    pub l2_capacity_mb: u64,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    pub z_threshold: f64,
    pub max_gap_ms: u64,
    pub min_volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPipelineConfig {
    pub sources: Vec<DataSourceConfig>,
    pub cache: CacheConfig,
    pub quality: QualityConfig,
    pub default_interval_ms: u64,
}

pub struct DataConfigLoader;

impl DataConfigLoader {
    pub fn from_json(json: &str) -> Result<DataPipelineConfig, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn validate(config: &DataPipelineConfig) -> Result<(), String> {
        if config.cache.l1_capacity_mb >= config.cache.l2_capacity_mb {
            return Err(format!(
                "l1_capacity_mb ({}) must be less than l2_capacity_mb ({})",
                config.cache.l1_capacity_mb, config.cache.l2_capacity_mb
            ));
        }
        if config.quality.z_threshold <= 0.0 {
            return Err(format!(
                "z_threshold ({}) must be > 0",
                config.quality.z_threshold
            ));
        }
        if config.quality.max_gap_ms == 0 {
            return Err("max_gap_ms must be > 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config_json() -> &'static str {
        r#"{
            "sources": [
                {"name": "btc", "source_type": "paper", "params": {}}
            ],
            "cache": {"l1_capacity_mb": 100, "l2_capacity_mb": 1000, "ttl_ms": 60000},
            "quality": {"z_threshold": 3.0, "max_gap_ms": 60000, "min_volume": 0.0},
            "default_interval_ms": 60000
        }"#
    }

    #[test]
    fn parse_valid_config() {
        let config = DataConfigLoader::from_json(valid_config_json()).unwrap();
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].name, "btc");
        assert_eq!(config.cache.l1_capacity_mb, 100);
    }

    #[test]
    fn validate_passes_on_good_config() {
        let config = DataConfigLoader::from_json(valid_config_json()).unwrap();
        assert!(DataConfigLoader::validate(&config).is_ok());
    }

    #[test]
    fn validate_fails_when_l1_ge_l2() {
        let mut config = DataConfigLoader::from_json(valid_config_json()).unwrap();
        config.cache.l1_capacity_mb = 1000;
        config.cache.l2_capacity_mb = 500;
        assert!(DataConfigLoader::validate(&config).is_err());
    }

    #[test]
    fn validate_fails_on_zero_z_threshold() {
        let mut config = DataConfigLoader::from_json(valid_config_json()).unwrap();
        config.quality.z_threshold = -1.0;
        assert!(DataConfigLoader::validate(&config).is_err());
    }

    #[test]
    fn validate_fails_on_zero_max_gap() {
        let mut config = DataConfigLoader::from_json(valid_config_json()).unwrap();
        config.quality.max_gap_ms = 0;
        assert!(DataConfigLoader::validate(&config).is_err());
    }
}
