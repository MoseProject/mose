//! Baseline microbenchmarks for Bush's editor-first scheduling and cache primitives.
//!
//! Run with `cargo bench -p bush` or `make bench-bush`. These measurements are intentionally
//! separate from decoder and GPU measurements: they establish the CPU overhead that every
//! interactive preview command adds before platform-specific media work starts.

use std::hint::black_box;

use bush::{ByteCache, Generation, LatestIntent};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_latest_intent_replacement(c: &mut Criterion) {
    let mut group = c.benchmark_group("latest_intent");

    for replacement_count in [1usize, 10, 100, 1_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(replacement_count),
            &replacement_count,
            |b, &replacement_count| {
                b.iter(|| {
                    let mut intent = LatestIntent::new();
                    for value in 0..replacement_count {
                        let generation = Generation(value as u64);
                        black_box(intent.replace(black_box(generation), black_box(value)));
                    }
                    black_box(intent.take())
                });
            },
        );
    }

    group.finish();
}

fn bench_cache_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_cache");

    for entry_count in [8usize, 64, 512] {
        group.bench_with_input(
            BenchmarkId::new("hot_get", entry_count),
            &entry_count,
            |b, &entry_count| {
                let mut cache = ByteCache::new(entry_count * 64).unwrap();
                for key in 0..entry_count {
                    cache.insert(key, key, 64).unwrap();
                }
                let hot_key = entry_count - 1;
                b.iter(|| black_box(cache.get(black_box(&hot_key)).copied()));
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_latest_intent_replacement, bench_cache_access);
criterion_main!(benches);
