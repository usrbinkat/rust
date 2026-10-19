use super::HashMap;
use crate::alloc::Allocator;

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
}
