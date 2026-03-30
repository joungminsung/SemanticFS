use crate::types::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// L1: Query result cache (LRU in-memory)
pub struct QueryCache {
    entries: RwLock<HashMap<u64, CacheEntry<Vec<SearchResult>>>>,
    max_size: usize,
}

struct CacheEntry<T> {
    value: T,
    #[allow(dead_code)]
    created_at: Instant,
    last_accessed: Instant,
    file_ids: Vec<FileId>,  // For targeted invalidation
}

impl QueryCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            max_size,
        }
    }

    pub fn get(&self, query_hash: u64) -> Option<Vec<SearchResult>> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&query_hash) {
            entry.last_accessed = Instant::now();
            Some(entry.value.clone())
        } else {
            None
        }
    }

    pub fn put(&self, query_hash: u64, results: Vec<SearchResult>, file_ids: Vec<FileId>) {
        let mut entries = self.entries.write();

        // Evict LRU if at capacity
        if entries.len() >= self.max_size {
            if let Some((&oldest_key, _)) = entries.iter()
                .min_by_key(|(_, e)| e.last_accessed) {
                entries.remove(&oldest_key);
            }
        }

        let now = Instant::now();
        entries.insert(query_hash, CacheEntry {
            value: results,
            created_at: now,
            last_accessed: now,
            file_ids,
        });
    }

    /// Invalidate all cached queries that include the given file
    pub fn invalidate_file(&self, file_id: FileId) {
        let mut entries = self.entries.write();
        entries.retain(|_, entry| !entry.file_ids.contains(&file_id));
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    pub fn hit_rate_info(&self) -> (usize, usize) {
        // Returns (cache_size, max_size)
        (self.entries.read().len(), self.max_size)
    }
}

/// L2: Embedding cache (file hash -> embedding vector, disk-backed)
pub struct EmbeddingCache {
    entries: RwLock<HashMap<String, Vec<f32>>>,
}

impl EmbeddingCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn get(&self, file_hash: &str) -> Option<Vec<f32>> {
        self.entries.read().get(file_hash).cloned()
    }

    pub fn put(&self, file_hash: String, embedding: Vec<f32>) {
        self.entries.write().insert(file_hash, embedding);
    }

    pub fn remove(&self, file_hash: &str) {
        self.entries.write().remove(file_hash);
    }

    pub fn len(&self) -> usize {
        self.entries.read().len()
    }
}

/// L3: Parsed query cache (natural language -> ParsedQuery, TTL-based)
pub struct ParsedQueryCache {
    entries: RwLock<HashMap<String, (ParsedQueryCacheEntry, Instant)>>,
    ttl: Duration,
    max_size: usize,
}

#[derive(Clone, Debug)]
pub struct ParsedQueryCacheEntry {
    pub semantic_query: String,
    pub filters_json: String,  // Serialized filters
}

impl ParsedQueryCache {
    pub fn new(ttl_secs: u64, max_size: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
            max_size,
        }
    }

    pub fn get(&self, raw_query: &str) -> Option<ParsedQueryCacheEntry> {
        let entries = self.entries.read();
        if let Some((entry, created_at)) = entries.get(raw_query) {
            if created_at.elapsed() < self.ttl {
                return Some(entry.clone());
            }
        }
        None
    }

    pub fn put(&self, raw_query: String, entry: ParsedQueryCacheEntry) {
        let mut entries = self.entries.write();

        // Remove expired entries
        let ttl = self.ttl;
        entries.retain(|_, (_, created_at)| created_at.elapsed() < ttl);

        // Evict oldest if at capacity
        if entries.len() >= self.max_size {
            if let Some((oldest_key, _)) = entries.iter()
                .min_by_key(|(_, (_, created))| *created)
                .map(|(k, v)| (k.clone(), v.clone())) {
                entries.remove(&oldest_key);
            }
        }

        entries.insert(raw_query, (entry, Instant::now()));
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn len(&self) -> usize {
        self.entries.read().len()
    }
}

/// Combined cache manager
pub struct CacheManager {
    pub query_cache: QueryCache,
    pub embedding_cache: EmbeddingCache,
    pub parsed_query_cache: ParsedQueryCache,
}

impl CacheManager {
    pub fn new(query_cache_size: usize, parsed_query_ttl: u64, parsed_query_max: usize) -> Self {
        Self {
            query_cache: QueryCache::new(query_cache_size),
            embedding_cache: EmbeddingCache::new(),
            parsed_query_cache: ParsedQueryCache::new(parsed_query_ttl, parsed_query_max),
        }
    }

    pub fn default() -> Self {
        Self::new(1000, 300, 500)
    }

    /// Called when a file changes -- invalidates relevant caches
    pub fn on_file_changed(&self, file_id: FileId, old_hash: Option<&str>) {
        self.query_cache.invalidate_file(file_id);
        if let Some(hash) = old_hash {
            self.embedding_cache.remove(hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_query_cache_lru() {
        let cache = QueryCache::new(2);
        let r1 = vec![SearchResult { file_id: 1, path: PathBuf::from("/a"), name: "a".into(), score: 1.0, matched_chunks: vec![] }];
        let r2 = vec![SearchResult { file_id: 2, path: PathBuf::from("/b"), name: "b".into(), score: 1.0, matched_chunks: vec![] }];
        let r3 = vec![SearchResult { file_id: 3, path: PathBuf::from("/c"), name: "c".into(), score: 1.0, matched_chunks: vec![] }];

        cache.put(1, r1, vec![1]);
        cache.put(2, r2, vec![2]);
        // Access key 1 to make it recently used
        cache.get(1);
        // Insert key 3, should evict key 2
        cache.put(3, r3, vec![3]);

        assert!(cache.get(1).is_some());
        assert!(cache.get(2).is_none());
        assert!(cache.get(3).is_some());
    }

    #[test]
    fn test_query_cache_invalidation() {
        let cache = QueryCache::new(10);
        let results = vec![SearchResult { file_id: 1, path: PathBuf::from("/a"), name: "a".into(), score: 1.0, matched_chunks: vec![] }];
        cache.put(100, results, vec![1, 2]);
        cache.invalidate_file(1);
        assert!(cache.get(100).is_none());
    }

    #[test]
    fn test_embedding_cache() {
        let cache = EmbeddingCache::new();
        cache.put("hash1".into(), vec![1.0, 2.0, 3.0]);
        assert_eq!(cache.get("hash1").unwrap(), vec![1.0, 2.0, 3.0]);
        assert!(cache.get("hash2").is_none());
    }

    #[test]
    fn test_parsed_query_cache_ttl() {
        let cache = ParsedQueryCache::new(0, 10); // 0 second TTL
        cache.put("test".into(), ParsedQueryCacheEntry {
            semantic_query: "test".into(),
            filters_json: "[]".into(),
        });
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.get("test").is_none());
    }
}
