/// JSON schema-like validation without external deps.

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Object,
    Array,
}

#[derive(Debug, Clone)]
pub struct FieldSchema {
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

#[derive(Default)]
pub struct SchemaValidator {
    fields: Vec<FieldSchema>,
}

#[derive(Debug)]
pub struct SchemaError {
    pub field: String,
    pub reason: String,
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_field(&mut self, field: FieldSchema) {
        self.fields.push(field);
    }

    pub fn validate(&self, config: &serde_json::Value) -> Vec<SchemaError> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let val = config.get(&field.name);
            match val {
                None if field.required => errors.push(SchemaError {
                    field: field.name.clone(),
                    reason: "required field missing".to_string(),
                }),
                None => continue,
                Some(v) => {
                    let type_ok = match field.field_type {
                        FieldType::String => v.is_string(),
                        FieldType::Number => v.is_number(),
                        FieldType::Boolean => v.is_boolean(),
                        FieldType::Object => v.is_object(),
                        FieldType::Array => v.is_array(),
                    };
                    if !type_ok {
                        errors.push(SchemaError {
                            field: field.name.clone(),
                            reason: format!("expected {:?}", field.field_type),
                        });
                    }
                    if let Some(n) = v.as_f64() {
                        if let Some(min) = field.min_value {
                            if n < min {
                                errors.push(SchemaError {
                                    field: field.name.clone(),
                                    reason: format!("value {} below minimum {}", n, min),
                                });
                            }
                        }
                        if let Some(max) = field.max_value {
                            if n > max {
                                errors.push(SchemaError {
                                    field: field.name.clone(),
                                    reason: format!("value {} above maximum {}", n, max),
                                });
                            }
                        }
                    }
                }
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_field_missing() {
        let mut sv = SchemaValidator::new();
        sv.add_field(FieldSchema { name: "venue".to_string(), field_type: FieldType::String, required: true, min_value: None, max_value: None });
        let errs = sv.validate(&serde_json::json!({}));
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn range_validation() {
        let mut sv = SchemaValidator::new();
        sv.add_field(FieldSchema { name: "timeout".to_string(), field_type: FieldType::Number, required: true, min_value: Some(100.0), max_value: Some(10000.0) });
        let errs = sv.validate(&serde_json::json!({"timeout": 50}));
        assert!(!errs.is_empty());
        let ok = sv.validate(&serde_json::json!({"timeout": 5000}));
        assert!(ok.is_empty());
    }
}
