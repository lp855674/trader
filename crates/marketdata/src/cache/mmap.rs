// ── MmapConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MmapConfig {
    pub file_path: String,
    pub max_size_bytes: usize,
}

// ── MmapCache ─────────────────────────────────────────────────────────────────

pub struct MmapCache {
    pub data: Vec<u8>,
    config: MmapConfig,
}

impl MmapCache {
    pub fn new(config: MmapConfig) -> Self {
        let size = config.max_size_bytes;
        Self {
            data: vec![0u8; size],
            config,
        }
    }

    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), String> {
        let end = offset + data.len();
        if end > self.data.len() {
            return Err(format!(
                "write out of bounds: offset={} len={} capacity={}",
                offset,
                data.len(),
                self.data.len()
            ));
        }
        self.data[offset..end].copy_from_slice(data);
        Ok(())
    }

    pub fn read(&self, offset: usize, len: usize) -> Result<&[u8], String> {
        let end = offset + len;
        if end > self.data.len() {
            return Err(format!(
                "read out of bounds: offset={} len={} capacity={}",
                offset, len, self.data.len()
            ));
        }
        Ok(&self.data[offset..end])
    }

    pub fn flush(&self) -> Result<(), String> {
        // No-op in stub
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read() {
        let cfg = MmapConfig {
            file_path: "/tmp/test.mmap".to_string(),
            max_size_bytes: 1024,
        };
        let mut cache = MmapCache::new(cfg);
        cache.write(0, &[1, 2, 3, 4]).unwrap();
        let read = cache.read(0, 4).unwrap();
        assert_eq!(read, &[1, 2, 3, 4]);
    }

    #[test]
    fn out_of_bounds_returns_error() {
        let cfg = MmapConfig {
            file_path: "/tmp/test.mmap".to_string(),
            max_size_bytes: 10,
        };
        let mut cache = MmapCache::new(cfg);
        assert!(cache.write(8, &[1, 2, 3, 4]).is_err());
    }
}
