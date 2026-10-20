//! Indexed access implemented beside the private owning table. No iterator
//! advancement, hashing, equality callback, or vacant-group scan is involved.
use crate::alloc::Allocator;
use crate::HashMap;

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
}
