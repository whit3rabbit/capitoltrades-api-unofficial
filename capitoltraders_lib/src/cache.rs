use dashmap::DashMap;
use std::time::{Duration, Instant};

struct CacheEntry {
    value: String,
    expires_at: Instant,
}

pub struct MemoryCache {
    store: DashMap<String, CacheEntry>,
    ttl: Duration,
}

impl MemoryCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            store: DashMap::new(),
            ttl,
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let entry = self.store.get(key)?;
        if Instant::now() > entry.expires_at {
            drop(entry);
            self.store.remove(key);
            return None;
        }
        Some(entry.value.clone())
    }

    pub fn set(&self, key: String, value: String) {
        self.store.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    pub fn clear(&self) {
        self.store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_set_and_get() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get("key1"), Some("value1".to_string()));
    }

    #[test]
    fn cache_miss() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn cache_expiration() {
        let cache = MemoryCache::new(Duration::from_millis(1));
        cache.set("key1".to_string(), "value1".to_string());
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn cache_overwrite() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set("key1".to_string(), "old".to_string());
        cache.set("key1".to_string(), "new".to_string());
        assert_eq!(cache.get("key1"), Some("new".to_string()));
    }

    #[test]
    fn cache_clear() {
        let cache = MemoryCache::new(Duration::from_secs(60));
        cache.set("a".to_string(), "1".to_string());
        cache.set("b".to_string(), "2".to_string());
        cache.clear();
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), None);
    }
}
