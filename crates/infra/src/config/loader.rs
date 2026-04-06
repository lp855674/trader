use std::collections::HashMap;

/// Simple JSON-based config loader with environment variable injection.
pub struct ConfigLoader {
    env_prefix: String,
    overrides: HashMap<String, String>,
}

impl ConfigLoader {
    pub fn new(env_prefix: &str) -> Self {
        Self {
            env_prefix: env_prefix.to_string(),
            overrides: HashMap::new(),
        }
    }

    /// Apply a command-line override (key=value form).
    pub fn add_override(&mut self, key: &str, value: &str) {
        self.overrides.insert(key.to_string(), value.to_string());
    }

    /// Load a JSON string, inject env vars (PREFIX_KEY → key), apply overrides.
    /// Returns a `serde_json::Value`.
    pub fn load_json(&self, json: &str) -> Result<serde_json::Value, serde_json::Error> {
        let mut value: serde_json::Value = serde_json::from_str(json)?;
        // Inject environment overrides
        for (key, val) in &self.overrides {
            inject_nested(&mut value, key, val);
        }
        Ok(value)
    }

    /// Read all environment variables with the configured prefix and expose them.
    pub fn env_vars(&self) -> HashMap<String, String> {
        std::env::vars()
            .filter(|(k, _)| k.starts_with(&self.env_prefix))
            .map(|(k, v)| (k[self.env_prefix.len()..].to_lowercase(), v))
            .collect()
    }
}

fn inject_nested(val: &mut serde_json::Value, path: &str, new_val: &str) {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    if parts.len() == 1 {
        if let serde_json::Value::Object(map) = val {
            map.insert(parts[0].to_string(), serde_json::Value::String(new_val.to_string()));
        }
    } else if let serde_json::Value::Object(map) = val {
        if let Some(child) = map.get_mut(parts[0]) {
            inject_nested(child, parts[1], new_val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_json_basic() {
        let loader = ConfigLoader::new("APP_");
        let v = loader.load_json(r#"{"venue":"paper","timeout":5000}"#).unwrap();
        assert_eq!(v["venue"], "paper");
    }

    #[test]
    fn override_applies() {
        let mut loader = ConfigLoader::new("APP_");
        loader.add_override("venue", "live");
        let v = loader.load_json(r#"{"venue":"paper"}"#).unwrap();
        assert_eq!(v["venue"], "live");
    }
}
