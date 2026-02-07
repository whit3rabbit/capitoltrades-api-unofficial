//! In-memory TTL cache backed by `DashMap` for concurrent access.

use dashmap::DashMap;
use std::time::{Duration, Instant};

/// A single cached value with its expiration time.
struct CacheEntry {
    value: String,
    expires_at: Instant,
}

/// Thread-safe in-memory cache with time-to-live expiration.
///
/// Entries are stored as serialized JSON strings. Expired entries are
/// lazily evicted on the next `get` call for that key.
pub struct MemoryCache {
    store: DashMap<String, CacheEntry>,
    ttl: Duration,
}

impl MemoryCache {
    /// Creates a new cache with the given time-to-live for entries.
    pub fn new(ttl: Duration) -> Self {
        Self {
            store: DashMap::new(),
            ttl,
        }
    }

    /// Returns the cached value for `key`, or `None` if missing or expired.
    pub fn get(&self, key: &str) -> Option<String> {
        let entry = self.store.get(key)?;
        if Instant::now() > entry.expires_at {
            drop(entry);
            self.store.remove(key);
            return None;
        }
        Some(entry.value.clone())
    }

    /// Inserts or overwrites a cache entry. The entry expires after the configured TTL.
    pub fn set(&self, key: String, value: String) {
        self.store.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    /// Removes all entries from the cache.
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
