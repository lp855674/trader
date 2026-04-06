/// A rule applied to a config field.
pub struct FieldRule {
    pub field: String,
    pub description: String,
    pub check: Box<dyn Fn(&serde_json::Value) -> bool + Send + Sync>,
}

#[derive(Default)]
pub struct ConfigValidator {
    rules: Vec<FieldRule>,
}

#[derive(Debug)]
pub struct ValidationError {
    pub field: String,
    pub description: String,
}

impl ConfigValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_rule(
        &mut self,
        field: &str,
        description: &str,
        check: impl Fn(&serde_json::Value) -> bool + Send + Sync + 'static,
    ) {
        self.rules.push(FieldRule {
            field: field.to_string(),
            description: description.to_string(),
            check: Box::new(check),
        });
    }

    pub fn validate(&self, config: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for rule in &self.rules {
            let field_val = config.get(&rule.field).unwrap_or(&serde_json::Value::Null);
            if !(rule.check)(field_val) {
                errors.push(ValidationError {
                    field: rule.field.clone(),
                    description: rule.description.clone(),
                });
            }
        }
        errors
    }

    pub fn is_valid(&self, config: &serde_json::Value) -> bool {
        self.validate(config).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_config_passes() {
        let mut v = ConfigValidator::new();
        v.add_rule("timeout", "must be a positive number", |val| {
            val.as_u64().map(|n| n > 0).unwrap_or(false)
        });
        let cfg = serde_json::json!({"timeout": 5000});
        assert!(v.is_valid(&cfg));
    }

    #[test]
    fn invalid_config_reports_errors() {
        let mut v = ConfigValidator::new();
        v.add_rule("venue", "must not be empty", |val| {
            val.as_str().map(|s| !s.is_empty()).unwrap_or(false)
        });
        let cfg = serde_json::json!({"venue": ""});
        let errs = v.validate(&cfg);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].field, "venue");
    }

    #[test]
    fn missing_field_fails_rule() {
        let mut v = ConfigValidator::new();
        v.add_rule("api_key", "must be present", |val| !val.is_null());
        let cfg = serde_json::json!({});
        assert!(!v.is_valid(&cfg));
    }
}
