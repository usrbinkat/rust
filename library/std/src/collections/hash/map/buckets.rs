use super::HashMap;
use crate::alloc::Allocator;
use crate::hash::{BuildHasher, Hash};

impl<K, V, S, A: Allocator> HashMap<K, V, S, A> {
    /// Returns the number of physical bucket indices, including vacant and
    /// deleted buckets. Removal does not reduce this count without resizing.
    #[unstable(feature = "hashmap_bucket_access", issue = "none")]
    #[inline]
    pub fn num_buckets(&self) -> usize {
        self.base.num_buckets()
    }

    /// Inspects one physical bucket without hashing, comparing, or scanning.
    /// Returns `None` for vacant, deleted, and out-of-range indices.
    /// Indices must not be retained across map mutation.
    #[unstable(feature = "hashmap_bucket_access", issue = "none")]
    #[inline]
    pub fn get_bucket(&self, index: usize) -> Option<(&K, &V)> {
        self.base.get_bucket(index)
    }

    /// Returns the least occupied bucket index in `start..end`, `end` taken
    /// as at most [`Self::num_buckets`], or `None` when every bucket of the
    /// range is vacant or deleted: the index the per-index scan
    /// `(start..end).find(|&i| self.get_bucket(i).is_some())` returns, found
    /// by loading the range's control bytes a group at a time. Inspects no
    /// bucket's element.
    #[unstable(feature = "hashmap_bucket_access", issue = "none")]
    #[inline]
    pub fn next_full_bucket(&self, start: usize, end: usize) -> Option<usize> {
        self.base.next_full_bucket(start, end)
    }
}

// hashbrown declares `HashMap::allocation_size` under these bounds; the
// forwarding method keeps the same shape.
impl<K, V, S, A: Allocator> HashMap<K, V, S, A>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Returns the backing table's allocation size, including control bytes.
    /// This measures requested allocation space, not allocator overhead.
    #[unstable(feature = "hashmap_bucket_access", issue = "none")]
    #[inline]
    pub fn allocation_size(&self) -> usize {
        self.base.allocation_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{BuildHasherDefault, Hasher};

    #[derive(Default)]
    struct Collisions;
    impl Hasher for Collisions {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _: &[u8]) {}
    }

    #[test]
    fn bucket_extent_survives_collision_cluster_deletion() {
        let mut map =
            HashMap::with_capacity_and_hasher(64, BuildHasherDefault::<Collisions>::default());
        let capacity = map.capacity();
        for key in 0..capacity {
            map.insert(key, key + 1);
        }
        let buckets = map.num_buckets();
        let allocation = map.allocation_size();
        for key in 0..capacity - 1 {
            map.remove(&key);
        }
        assert_eq!(map.num_buckets(), buckets);
        assert_eq!(map.allocation_size(), allocation);
        assert!(map.capacity() < capacity);
        let mut found = 0;
        for index in 0..buckets {
            if let Some((key, value)) = map.get_bucket(index) {
                assert_eq!((*key, *value), (capacity - 1, capacity));
                found += 1;
            }
        }
        assert_eq!(found, 1);
        assert_eq!(map.get_bucket(buckets), None);
        assert_eq!(map.get_bucket(usize::MAX), None);
    }

    /// The group scan finds the occupied bucket the per-index scan finds,
    /// from every start, to ends within a group, a 64-control window, the
    /// table's end and past it, on a collision cluster before and after its
    /// deletions leave deleted controls before its one remaining entry.
    #[test]
    fn next_full_bucket_matches_the_per_index_scan() {
        let mut map =
            HashMap::with_capacity_and_hasher(64, BuildHasherDefault::<Collisions>::default());
        let capacity = map.capacity();
        for key in 0..capacity {
            map.insert(key, key + 1);
        }
        let check = |map: &HashMap<usize, usize, BuildHasherDefault<Collisions>>| {
            let buckets = map.num_buckets();
            for start in 0..=buckets {
                for end in [start, start + 1, start + 15, start + 64, buckets, usize::MAX] {
                    assert_eq!(
                        map.next_full_bucket(start, end),
                        (start..end.min(buckets)).find(|&i| map.get_bucket(i).is_some()),
                        "{start}..{end} of {buckets} buckets"
                    );
                }
            }
        };
        check(&map);
        for key in 0..capacity - 1 {
            map.remove(&key);
        }
        check(&map);
    }
}
