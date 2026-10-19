use super::super::navigate::{SteppedItem, SteppedRange};
use super::{AllocatorClone, BTreeMap};
use core::fmt;

/// A forward traversal retaining individual tree transitions.
///
/// Creation does not descend the tree or allocate. Each call to `step`
/// performs at most one transition, even when admission always succeeds.
/// Dropping this shared cursor neither walks nor destroys the map.
#[must_use]
#[unstable(feature = "btree_stepped_cursor", issue = "none")]
pub struct SteppedCursor<'a, K, V> {
    range: SteppedRange<'a, K, V>,
}

/// The result of one admitted tree transition.
#[derive(Debug, PartialEq, Eq)]
#[unstable(feature = "btree_stepped_cursor", issue = "none")]
pub enum SteppedCursorStep<'a, K, V> {
    /// A descent, ascent, or edge transition was retained.
    Progress,
    /// One entry was yielded, in ascending key order.
    Entry(&'a K, &'a V),
    /// The terminal transition was admitted.
    Complete,
    /// The terminal transition had already completed.
    Finished,
}

impl<K, V, A: AllocatorClone> BTreeMap<K, V, A> {
    /// Creates an allocation-free cursor without performing initial descent.
    #[unstable(feature = "btree_stepped_cursor", issue = "none")]
    pub fn stepped_cursor(&self) -> SteppedCursor<'_, K, V> {
        SteppedCursor { range: SteppedRange::new(self.root.as_ref().map(|root| root.reborrow())) }
    }
}

impl<'a, K, V> SteppedCursor<'a, K, V> {
    /// Admits and performs one transition. An admission error leaves the
    /// cursor unchanged. Admission is called exactly once, except after
    /// completion, when `Finished` is returned without calling it.
    ///
    /// The callback's own execution cost is the caller's responsibility.
    #[unstable(feature = "btree_stepped_cursor", issue = "none")]
    pub fn step<E>(
        &mut self,
        admit: impl FnOnce() -> Result<(), E>,
    ) -> Result<SteppedCursorStep<'a, K, V>, E> {
        Ok(match self.range.step(admit)? {
            SteppedItem::Progress => SteppedCursorStep::Progress,
            SteppedItem::Entry(key, value) => SteppedCursorStep::Entry(key, value),
            SteppedItem::Complete => SteppedCursorStep::Complete,
            SteppedItem::Finished => SteppedCursorStep::Finished,
        })
    }
}

#[unstable(feature = "btree_stepped_cursor", issue = "none")]
impl<K, V> fmt::Debug for SteppedCursor<'_, K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SteppedCursor").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_preserves_every_transition_and_literal_entries() {
        for count in [0, 1, 11, 12, 128, 4096] {
            let mut map = BTreeMap::new();
            for key in (0..count).rev() {
                map.insert(key, key * 7 + 3);
            }
            let mut cursor = map.stepped_cursor();
            let mut reference = map.stepped_cursor();
            let mut yielded = 0;
            let mut completed = false;
            // Three transitions per entry and at most four more per node;
            // the tree has at most n+1 nodes, including an empty root.
            for _ in 0..8 * (count + 1) {
                for _ in 0..2 {
                    let mut called = false;
                    assert_eq!(
                        cursor.step(|| {
                            called = true;
                            Err(17)
                        }),
                        Err(17)
                    );
                    assert!(called);
                }
                let mut admissions = 0;
                let actual = cursor
                    .step(|| {
                        admissions += 1;
                        Ok::<_, ()>(())
                    })
                    .unwrap();
                assert_eq!(admissions, 1);
                assert_eq!(actual, reference.step(|| Ok::<_, ()>(())).unwrap());
                match actual {
                    SteppedCursorStep::Entry(key, value) => {
                        assert_eq!(*key, yielded);
                        assert_eq!(*value, yielded * 7 + 3);
                        yielded += 1;
                    }
                    SteppedCursorStep::Complete => {
                        completed = true;
                        break;
                    }
                    SteppedCursorStep::Progress => {}
                    SteppedCursorStep::Finished => panic!("premature finished"),
                }
            }
            assert!(completed);
            assert_eq!(yielded, count);
            assert_eq!(cursor.step(|| Err::<(), _>(17)), Ok(SteppedCursorStep::Finished));
        }
    }
}
