//! Indexed access implemented beside the private owning table. No iterator
//! advancement, hashing, equality callback, or vacant-group scan is involved,
//! except in [`HashMap::next_full_bucket`] and [`HashTable::next_full_bucket`],
//! which scan exactly the control groups of the range they are given.
use crate::alloc::Allocator;
use crate::control::Group;
use crate::{HashMap, HashTable};

/// The number of control bytes one group operation inspects on this target:
/// 16 with SSE2 or LSX, 8 with NEON or the 64-bit generic implementation, 4
/// with the 32-bit generic implementation. Execution detail only: the
/// buckets a range scan returns do not depend on it.
#[inline]
#[must_use]
pub const fn bucket_group_width() -> usize {
    Group::WIDTH
}

impl<K, V, S, A: Allocator> HashMap<K, V, S, A> {
    /// Number of bucket indices, including vacant and deleted buckets.
    ///
    /// Unlike insertion capacity, this does not decrease when removal leaves
    /// deleted control bytes. Indices remain stable while the map is borrowed
    /// immutably; callers must not retain them across mutation.
    #[inline]
    pub fn num_buckets(&self) -> usize {
        self.table.num_buckets()
    }

    /// Inspect exactly one bucket index, without scanning for another entry.
    ///
    /// Returns `None` for vacant, deleted, or out-of-range buckets. The shared
    /// borrow prevents mutation and relocation while returned references live.
    /// Keys are immutable, preserving the map's hash/equality invariants.
    #[inline]
    pub fn get_bucket(&self, index: usize) -> Option<(&K, &V)> {
        self.table
            .get_bucket(index)
            .map(|(key, value)| (key, value))
    }

    /// The least occupied bucket index in `start..end`, `end` taken as at
    /// most [`Self::num_buckets`], or `None` when every bucket of the range
    /// is vacant or deleted.
    ///
    /// The same index as `(start..end).find(|&i| self.get_bucket(i).is_some())`,
    /// found by loading the range's control bytes a group at a time
    /// ([`bucket_group_width`]) instead of one index at a time. It inspects
    /// no control byte past the group holding `end - 1`, and no bucket's
    /// element.
    #[inline]
    pub fn next_full_bucket(&self, start: usize, end: usize) -> Option<usize> {
        self.table.next_full_bucket(start, end)
    }
}

impl<T, A: Allocator> HashTable<T, A> {
    /// The least occupied bucket index in `start..end`, `end` taken as at
    /// most [`Self::num_buckets`], or `None` when every bucket of the range
    /// is vacant or deleted.
    ///
    /// The same index as `(start..end).find(|&i| self.get_bucket(i).is_some())`,
    /// found by loading the range's control bytes a group at a time
    /// ([`bucket_group_width`]) instead of one index at a time. It inspects
    /// no control byte past the group holding `end - 1`, and no bucket's
    /// element.
    #[inline]
    pub fn next_full_bucket(&self, start: usize, end: usize) -> Option<usize> {
        self.raw.next_full_bucket(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::bucket_group_width;
    use crate::{HashMap, HashTable};
    use core::hash::{BuildHasherDefault, Hasher};
    use stdalloc::vec::Vec;

    /// Every key hashes alike: one collision cluster, whose removals leave
    /// deleted control bytes.
    #[derive(Default)]
    struct Colliding;
    impl Hasher for Colliding {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _: &[u8]) {}
    }

    /// Keys spread over the table by a fixed multiplier, so its layout is
    /// reproducible.
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

    /// The scalar reference: the least occupied index in `start..end`.
    fn scalar<K, V, S>(map: &HashMap<K, V, S>, start: usize, end: usize) -> Option<usize> {
        (start..end.min(map.num_buckets())).find(|&i| map.get_bucket(i).is_some())
    }

    /// The group scan equals the scalar reference over every start and the
    /// ends around it: empty, one bucket, a group less one, a group, a
    /// 64-control window, the table's end and past it; and enumerating the
    /// table by chained group scans yields every occupied index the scalar
    /// scan yields, in the same order.
    fn agrees<K, V, S>(name: &str, map: &HashMap<K, V, S>) {
        let buckets = map.num_buckets();
        let width = bucket_group_width();
        for start in 0..=buckets {
            for end in [
                start,
                start + 1,
                start + width - 1,
                start + width,
                start + 64,
                buckets,
                buckets + width,
                usize::MAX,
            ] {
                assert_eq!(
                    map.next_full_bucket(start, end),
                    scalar(map, start, end),
                    "{name}: {start}..{end} of {buckets} buckets"
                );
            }
        }
        let mut chained = Vec::new();
        let mut at = 0;
        while let Some(index) = map.next_full_bucket(at, buckets) {
            chained.push(index);
            at = index + 1;
        }
        let every: Vec<usize> = (0..buckets)
            .filter(|&i| map.get_bucket(i).is_some())
            .collect();
        assert_eq!(chained, every, "{name}: enumeration");
        assert_eq!(chained.len(), map.len(), "{name}: occupied count");
    }

    #[test]
    fn next_full_bucket_refines_the_scalar_scan() {
        // Unallocated, and allocated with nothing in it.
        let unallocated: HashMap<u64, u64, BuildHasherDefault<Spread>> = HashMap::default();
        agrees("unallocated", &unallocated);
        let vacant: HashMap<u64, u64, BuildHasherDefault<Spread>> =
            HashMap::with_capacity_and_hasher(1000, BuildHasherDefault::default());
        agrees("allocated and vacant", &vacant);

        // Tables smaller than a group, a group, and many groups, each full to
        // capacity, with entries on every window boundary they reach.
        for count in [1u64, 3, 7, 14, 28, 100, 900] {
            let mut map: HashMap<u64, u64, BuildHasherDefault<Spread>> = HashMap::default();
            for key in 0..count {
                map.insert(key, key);
            }
            agrees(&format!("{count} spread"), &map);
        }

        // A collision cluster: full, then all but its last removed, leaving
        // deleted controls before the one entry; then every other removed.
        let mut cluster: HashMap<u64, u64, BuildHasherDefault<Colliding>> = HashMap::default();
        for key in 0..1024 {
            cluster.insert(key, key);
        }
        agrees("collision cluster", &cluster);
        let mut tombstoned: HashMap<u64, u64, BuildHasherDefault<Colliding>> = HashMap::default();
        for key in 0..1024 {
            tombstoned.insert(key, key);
        }
        for key in (0..1024).step_by(2) {
            cluster.remove(&key);
        }
        agrees("collision cluster, every other removed", &cluster);
        for key in 0..1023 {
            tombstoned.remove(&key);
        }
        agrees("collision cluster, all but the last removed", &tombstoned);
    }

    #[test]
    fn next_full_bucket_on_a_hash_table_is_the_maps() {
        let hash = |value: &u64| value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut table = HashTable::new();
        assert_eq!(table.next_full_bucket(0, usize::MAX), None);
        for value in 0..300u64 {
            table.insert_unique(hash(&value), value, hash);
        }
        let buckets = table.num_buckets();
        for start in 0..=buckets {
            for end in [start + 1, start + 64, buckets, usize::MAX] {
                assert_eq!(
                    table.next_full_bucket(start, end),
                    (start..end.min(buckets)).find(|&i| table.get_bucket(i).is_some()),
                    "{start}..{end} of {buckets} buckets"
                );
            }
        }
    }
}
