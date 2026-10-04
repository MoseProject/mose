use bush::{ByteCache, CacheError};

#[test]
fn cache_rejects_zero_capacity_and_oversized_entry() {
    assert_eq!(
        ByteCache::<u8, ()>::new(0).unwrap_err(),
        CacheError::ZeroCapacity
    );

    let mut cache = ByteCache::new(4).unwrap();
    assert_eq!(
        cache.insert(1, "oversized", 5),
        Err(CacheError::EntryExceedsCapacity {
            entry_bytes: 5,
            capacity_bytes: 4,
        })
    );
    assert!(cache.is_empty());
}

#[test]
fn cache_evicts_least_recently_used_entries_to_stay_bounded() {
    let mut cache = ByteCache::new(6).unwrap();
    cache.insert(1, "one", 2).unwrap();
    cache.insert(2, "two", 2).unwrap();
    cache.get(&1);

    let result = cache.insert(3, "three", 3).unwrap();

    assert_eq!(result.evicted, vec![2]);
    assert_eq!(cache.used_bytes(), 5);
    assert_eq!(cache.get(&1), Some(&"one"));
    assert_eq!(cache.get(&2), None);
    assert_eq!(cache.get(&3), Some(&"three"));
}

#[test]
fn replacing_an_entry_updates_its_accounting_without_eviction() {
    let mut cache = ByteCache::new(4).unwrap();
    cache.insert(1, "old", 3).unwrap();

    let result = cache.insert(1, "new", 2).unwrap();

    assert!(result.replaced);
    assert!(result.evicted.is_empty());
    assert_eq!(cache.used_bytes(), 2);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get(&1), Some(&"new"));
}

#[test]
fn the_hard_capacity_is_reported() {
    let cache: bush::ByteCache<u32, u32> = bush::ByteCache::new(1024).unwrap();
    assert_eq!(cache.max_bytes(), 1024);
}
