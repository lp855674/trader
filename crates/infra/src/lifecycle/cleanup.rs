#[derive(Debug, Clone, PartialEq)]
pub enum ResourceType {
    Memory,
    FileHandle,
    NetworkConnection,
    DatabaseConn,
}

pub struct ResourceCleanup {
    registered: Vec<(ResourceType, String)>,
    cleaned: Vec<String>,
}

impl ResourceCleanup {
    pub fn new() -> Self {
        Self {
            registered: Vec::new(),
            cleaned: Vec::new(),
        }
    }

    pub fn register(&mut self, resource_type: ResourceType, name: &str) {
        self.registered.push((resource_type, name.to_string()));
    }

    pub fn cleanup_all(&mut self) -> Vec<String> {
        let names: Vec<String> = self.registered.iter().map(|(_, n)| n.clone()).collect();
        self.cleaned.extend(names.clone());
        self.registered.clear();
        names
    }

    pub fn pending_count(&self) -> usize {
        self.registered.len()
    }

    pub fn cleaned_count(&self) -> usize {
        self.cleaned.len()
    }
}

impl Default for ResourceCleanup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_all_clears_pending() {
        let mut rc = ResourceCleanup::new();
        rc.register(ResourceType::Memory, "heap_buf");
        rc.register(ResourceType::DatabaseConn, "conn_pool");
        assert_eq!(rc.pending_count(), 2);
        let cleaned = rc.cleanup_all();
        assert_eq!(cleaned.len(), 2);
        assert_eq!(rc.pending_count(), 0);
        assert_eq!(rc.cleaned_count(), 2);
    }
}
