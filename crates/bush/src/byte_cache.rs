//! A bounded least-recently-used cache for preview resources.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A byte-bounded cache with least-recently-used eviction.
///
/// Bush will use this shape for decoded frames, GPU uploads, and raster overlays. Callers supply
/// the exact retained-byte cost; entries larger than the whole budget are rejected rather than
/// evicting useful cached work and still failing to fit.
#[derive(Debug)]
pub struct ByteCache<K, V> {
    max_bytes: usize,
    used_bytes: usize,
    entries: HashMap<K, Entry<V>>,
    least_to_most_recent: VecDeque<K>,
}

#[derive(Debug)]
struct Entry<V> {
    value: V,
    bytes: usize,
}

/// Invalid cache configuration or insertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheError {
    ZeroCapacity,
    EntryExceedsCapacity {
        entry_bytes: usize,
        capacity_bytes: usize,
    },
}

/// The result of a cache insertion.
#[derive(Debug, Eq, PartialEq)]
pub struct CacheInsert<K> {
    /// Keys evicted to satisfy the byte budget, from least to most recently used.
    pub evicted: Vec<K>,
    /// Whether this insertion replaced an entry with the same key.
    pub replaced: bool,
}

impl<K, V> ByteCache<K, V>
where
    K: Clone + Eq + Hash,
{
    /// Creates a cache whose retained values can never exceed `max_bytes`.
    pub fn new(max_bytes: usize) -> Result<Self, CacheError> {
        if max_bytes == 0 {
            return Err(CacheError::ZeroCapacity);
        }

        Ok(Self {
            max_bytes,
            used_bytes: 0,
            entries: HashMap::new(),
            least_to_most_recent: VecDeque::new(),
        })
    }

    /// Returns the configured hard capacity in bytes.
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Returns bytes retained by the cache.
    pub const fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Returns the number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether there are no retained entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reads a value and promotes it to most-recently-used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if !self.entries.contains_key(key) {
            return None;
        }
        self.promote(key);
        self.entries.get(key).map(|entry| &entry.value)
    }

    /// Inserts `value`, evicting least-recently-used entries only as needed.
    pub fn insert(&mut self, key: K, value: V, bytes: usize) -> Result<CacheInsert<K>, CacheError> {
        if bytes > self.max_bytes {
            return Err(CacheError::EntryExceedsCapacity {
                entry_bytes: bytes,
                capacity_bytes: self.max_bytes,
            });
        }

        let previous = self.entries.remove(&key);
        let replaced = previous.is_some();
        if let Some(previous) = previous {
            self.used_bytes -= previous.bytes;
            self.remove_from_order(&key);
        }

        let mut evicted = Vec::new();
        while self.used_bytes.saturating_add(bytes) > self.max_bytes {
            let oldest = self
                .least_to_most_recent
                .pop_front()
                .expect("cache order is non-empty while retained bytes need eviction");
            let entry = self
                .entries
                .remove(&oldest)
                .expect("cache order only contains retained cache keys");
            self.used_bytes -= entry.bytes;
            evicted.push(oldest);
        }

        self.used_bytes += bytes;
        self.least_to_most_recent.push_back(key.clone());
        self.entries.insert(key, Entry { value, bytes });

        Ok(CacheInsert { evicted, replaced })
    }

    fn promote(&mut self, key: &K) {
        self.remove_from_order(key);
        self.least_to_most_recent.push_back(key.clone());
    }

    fn remove_from_order(&mut self, key: &K) {
        if let Some(index) = self
            .least_to_most_recent
            .iter()
            .position(|candidate| candidate == key)
        {
            self.least_to_most_recent.remove(index);
        }
    }
}
