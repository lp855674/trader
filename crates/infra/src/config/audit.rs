/// Audit log for config changes.

#[derive(Debug, Clone)]
pub enum AuditAction {
    Read,
    Write,
    Delete,
    Rollback,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp_ms: u64,
    pub actor: String,
    pub action: AuditAction,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Default)]
pub struct AuditLogger {
    entries: Vec<AuditEntry>,
}

impl AuditLogger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log_write(&mut self, actor: &str, field: &str, old_val: Option<&str>, new_val: &str) {
        self.entries.push(AuditEntry {
            timestamp_ms: now_ms(),
            actor: actor.to_string(),
            action: AuditAction::Write,
            field: field.to_string(),
            old_value: old_val.map(|s| s.to_string()),
            new_value: Some(new_val.to_string()),
        });
    }

    pub fn log_read(&mut self, actor: &str, field: &str) {
        self.entries.push(AuditEntry {
            timestamp_ms: now_ms(),
            actor: actor.to_string(),
            action: AuditAction::Read,
            field: field.to_string(),
            old_value: None,
            new_value: None,
        });
    }

    pub fn log_rollback(&mut self, actor: &str) {
        self.entries.push(AuditEntry {
            timestamp_ms: now_ms(),
            actor: actor.to_string(),
            action: AuditAction::Rollback,
            field: "*".to_string(),
            old_value: None,
            new_value: None,
        });
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn entries_by_actor(&self, actor: &str) -> Vec<&AuditEntry> {
        self.entries.iter().filter(|e| e.actor == actor).collect()
    }

    pub fn write_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.action, AuditAction::Write))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_write_records_entry() {
        let mut al = AuditLogger::new();
        al.log_write("alice", "timeout", Some("1000"), "2000");
        assert_eq!(al.write_count(), 1);
        assert_eq!(al.entries()[0].field, "timeout");
    }

    #[test]
    fn filter_by_actor() {
        let mut al = AuditLogger::new();
        al.log_write("alice", "venue", None, "live");
        al.log_read("bob", "timeout");
        al.log_write("alice", "timeout", Some("1000"), "3000");
        assert_eq!(al.entries_by_actor("alice").len(), 2);
        assert_eq!(al.entries_by_actor("bob").len(), 1);
    }
}
