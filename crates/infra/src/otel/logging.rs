use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp_ms: u64,
    pub fields: HashMap<String, String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Default)]
pub struct StructuredLogger {
    entries: Vec<LogEntry>,
}

impl StructuredLogger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&mut self, level: LogLevel, msg: &str, fields: HashMap<String, String>) {
        self.entries.push(LogEntry {
            level,
            message: msg.to_string(),
            timestamp_ms: now_ms(),
            fields,
        });
    }

    pub fn entries_at_level(&self, level: &LogLevel) -> Vec<&LogEntry> {
        self.entries.iter().filter(|e| &e.level == level).collect()
    }

    pub fn to_json_lines(&self) -> String {
        self.entries
            .iter()
            .map(|e| {
                let fields_json = e
                    .fields
                    .iter()
                    .map(|(k, v)| format!("\"{}\":\"{}\"", k, v))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{{\"level\":\"{}\",\"msg\":\"{}\",\"ts\":{},\"fields\":{{{}}}}}",
                    e.level.as_str(),
                    e.message,
                    e.timestamp_ms,
                    fields_json
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_and_query_by_level() {
        let mut logger = StructuredLogger::new();
        logger.log(LogLevel::Info, "started", HashMap::new());
        logger.log(LogLevel::Warn, "slow query", HashMap::new());
        logger.log(LogLevel::Info, "done", HashMap::new());
        assert_eq!(logger.entries_at_level(&LogLevel::Info).len(), 2);
        assert_eq!(logger.entries_at_level(&LogLevel::Warn).len(), 1);
        assert_eq!(logger.entries_at_level(&LogLevel::Error).len(), 0);
    }

    #[test]
    fn to_json_lines_contains_level() {
        let mut logger = StructuredLogger::new();
        let mut fields = HashMap::new();
        fields.insert("service".to_string(), "exec".to_string());
        logger.log(LogLevel::Error, "crash", fields);
        let json = logger.to_json_lines();
        assert!(json.contains("ERROR"));
        assert!(json.contains("crash"));
        assert!(json.contains("exec"));
    }
}
