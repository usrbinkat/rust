//! Enumerating a table by bucket index in windows of 64 control bytes, as a
//! bounded consumer admits and scans it (one window per admitted unit): each
//! window's first occupied bucket found by the scalar per-index scan
//! (`get_bucket`) or by the group scan (`next_full_bucket`), across vacant,
//! sparse, half-full, full and tombstoned tables.
use criterion::{BenchmarkId, Criterion};
use hashbrown::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::hint::black_box;

/// Control bytes one admitted unit inspects.
const WINDOW: usize = 64;

#[derive(Default)]
struct Spread(u64);
impl Hasher for Spread {
    fn finish(&self) -> u64 {
        self.0.wrapping_mul(0x9e37_79b9_7f4a_7c15)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 << 8) | u64::from(byte);
        }
    }
}

#[derive(Default)]
struct Colliding;
impl Hasher for Colliding {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {}
}

/// Every occupied bucket, a window at a time from the first bucket: a
/// window's first occupied bucket ends it, the next window starting after
/// it; the scan ends once `len` entries are found.
fn enumerate<K, V, S>(
    map: &HashMap<K, V, S>,
    first: impl Fn(&HashMap<K, V, S>, usize, usize) -> Option<usize>,
) -> usize {
    let (buckets, mut start, mut found) = (map.num_buckets(), 0, 0);
    while found < map.len() && start < buckets {
        let end = start + WINDOW.min(buckets - start);
        match first(map, start, end) {
            Some(bucket) => {
                found += 1;
                start = bucket + 1;
            }
            None => start = end,
        }
    }
    found
}

fn scalar<K, V, S>(map: &HashMap<K, V, S>, start: usize, end: usize) -> Option<usize> {
    (start..end).find(|&i| map.get_bucket(i).is_some())
}

fn group<K, V, S>(map: &HashMap<K, V, S>, start: usize, end: usize) -> Option<usize> {
    map.next_full_bucket(start, end)
}

fn spread(capacity: usize, entries: u64) -> HashMap<u64, u64, BuildHasherDefault<Spread>> {
    let mut map = HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default());
    for key in 0..entries {
        map.insert(key, key);
    }
    map
}

pub(crate) fn register_benches(c: &mut Criterion) {
    const CAPACITY: usize = 1 << 14;
    let mut tombstoned: HashMap<u64, u64, BuildHasherDefault<Colliding>> = HashMap::default();
    for key in 0..1024 {
        tombstoned.insert(key, key);
    }
    for key in 0..1023 {
        tombstoned.remove(&key);
    }
    let tables = [
        ("vacant", spread(CAPACITY, 0)),
        ("one entry", spread(CAPACITY, 1)),
        ("sparse 1%", spread(CAPACITY, CAPACITY as u64 / 100)),
        ("half", spread(CAPACITY, CAPACITY as u64 / 2)),
        ("full", spread(CAPACITY, CAPACITY as u64)),
    ];
    let mut scan = c.benchmark_group("bucket_scan");
    for (name, map) in &tables {
        assert_eq!(enumerate(map, scalar), enumerate(map, group), "{name}");
        scan.bench_with_input(BenchmarkId::new("scalar", name), map, |b, map| {
            b.iter(|| black_box(enumerate(black_box(map), scalar)));
        });
        scan.bench_with_input(BenchmarkId::new("group", name), map, |b, map| {
            b.iter(|| black_box(enumerate(black_box(map), group)));
        });
    }
    assert_eq!(
        enumerate(&tombstoned, scalar),
        enumerate(&tombstoned, group)
    );
    scan.bench_function(BenchmarkId::new("scalar", "tombstoned"), |b| {
        b.iter(|| black_box(enumerate(black_box(&tombstoned), scalar)));
    });
    scan.bench_function(BenchmarkId::new("group", "tombstoned"), |b| {
        b.iter(|| black_box(enumerate(black_box(&tombstoned), group)));
    });
    scan.finish();
}
