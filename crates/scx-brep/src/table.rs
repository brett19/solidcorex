// Written for solidcorex. Apache-2.0.
//! One table of records by id: slots in id order, a deleted slot left empty, shared by
//! reference count and copied whole on the first write (01 C5).
//!
//! The pin's `Arena` (`topology/src/topology.rs`, READ) is the same `Vec<Option<T>>` with a
//! live count. Copy-on-write in chunks of 64, for bodies past about 10k faces (C5), replaces
//! the inside of this type and nothing outside it: every caller goes through `get`,
//! `get_mut`, `push`, `remove` and `iter`.

use core::{iter::FusedIterator, marker::PhantomData};
use std::sync::Arc;

use crate::error::TableFull;
use crate::id::EntityKind;

/// A typed id that numbers the slots of one table.
pub(crate) trait Id: Copy {
    /// The kind it names.
    const KIND: EntityKind;
    /// The id of `slot`.
    fn from_slot(slot: u32) -> Self;
    /// Its slot.
    fn slot(self) -> usize;
}

/// Records by id, shared until written.
pub(crate) struct Table<I, T> {
    slots: Arc<Vec<Option<T>>>,
    live: usize,
    id: PhantomData<fn() -> I>,
}

impl<I, T> Clone for Table<I, T> {
    fn clone(&self) -> Self {
        Self {
            slots: Arc::clone(&self.slots),
            live: self.live,
            id: PhantomData,
        }
    }
}

impl<I, T> Default for Table<I, T> {
    fn default() -> Self {
        Self {
            slots: Arc::new(Vec::new()),
            live: 0,
            id: PhantomData,
        }
    }
}

impl<I, T: core::fmt::Debug> core::fmt::Debug for Table<I, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.slots.iter()).finish()
    }
}

impl<I: Id, T> Table<I, T> {
    /// The table holding `first` alone, at slot 0.
    pub(crate) fn with_first(first: T) -> Self {
        Self {
            slots: Arc::new(vec![Some(first)]),
            live: 1,
            id: PhantomData,
        }
    }

    /// The record at `id`.
    pub(crate) fn get(&self, id: I) -> Option<&T> {
        self.slots.get(id.slot())?.as_ref()
    }

    /// How many live records.
    #[cfg(test)]
    pub(crate) const fn len(&self) -> usize {
        self.live
    }

    /// How many slots, live or not: one past the highest id ever given.
    pub(crate) fn slots(&self) -> usize {
        self.slots.len()
    }

    /// The live records in id order.
    pub(crate) fn iter(&self) -> Live<'_, I, T> {
        Live {
            inner: self.slots.iter().enumerate(),
            left: self.live,
            id: PhantomData,
        }
    }

    /// The live records, to write, when no other table shares them; nothing otherwise.
    pub(crate) fn unique_mut(&mut self) -> impl Iterator<Item = &mut T> {
        Arc::get_mut(&mut self.slots)
            .into_iter()
            .flat_map(|slots| slots.iter_mut().flatten())
    }

    /// Whether `self` and `other` share their slots (no write has copied either).
    #[cfg(test)]
    pub(crate) fn shares(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.slots, &other.slots)
    }
}

impl<I: Id, T: Clone> Table<I, T> {
    /// The record at `id`, to write. The table is copied first if shared; a miss copies
    /// nothing.
    pub(crate) fn get_mut(&mut self, id: I) -> Option<&mut T> {
        self.get(id)?;
        Arc::make_mut(&mut self.slots).get_mut(id.slot())?.as_mut()
    }

    /// Leaves the next `count` ids unused, as if each had been given and its record removed
    /// (01 §2.2): the next push gets the id after them. Nothing is written when the ids would
    /// run past `u32`.
    pub(crate) fn skip(&mut self, count: u32) -> Result<(), TableFull> {
        let end = self.slots.len() + count as usize;
        if end > u32::MAX as usize {
            return Err(TableFull { kind: I::KIND });
        }
        if count > 0 {
            Arc::make_mut(&mut self.slots).resize_with(end, || None);
        }
        Ok(())
    }

    /// Stores `record` at the next id.
    pub(crate) fn push(&mut self, record: T) -> Result<I, TableFull> {
        let slot = u32::try_from(self.slots.len()).map_err(|_| TableFull { kind: I::KIND })?;
        Arc::make_mut(&mut self.slots).push(Some(record));
        self.live += 1;
        Ok(I::from_slot(slot))
    }

    /// Takes the record at `id`, leaving its slot empty for good.
    pub(crate) fn remove(&mut self, id: I) -> Option<T> {
        self.get(id)?;
        let taken = Arc::make_mut(&mut self.slots).get_mut(id.slot())?.take();
        self.live -= 1;
        taken
    }
}

/// The live records of a table in id order, each with its id.
pub(crate) struct Live<'a, I, T> {
    inner: core::iter::Enumerate<core::slice::Iter<'a, Option<T>>>,
    left: usize,
    id: PhantomData<fn() -> I>,
}

impl<I, T> Clone for Live<'_, I, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            left: self.left,
            id: PhantomData,
        }
    }
}

impl<'a, I: Id, T> Iterator for Live<'a, I, T> {
    type Item = (I, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        for (slot, record) in self.inner.by_ref() {
            if let Some(record) = record {
                self.left -= 1;
                return Some((I::from_slot(slot as u32), record));
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left, Some(self.left))
    }
}

impl<I: Id, T> DoubleEndedIterator for Live<'_, I, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while let Some((slot, record)) = self.inner.next_back() {
            if let Some(record) = record {
                self.left -= 1;
                return Some((I::from_slot(slot as u32), record));
            }
        }
        None
    }
}

impl<I: Id, T> ExactSizeIterator for Live<'_, I, T> {}

impl<I: Id, T> FusedIterator for Live<'_, I, T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::FaceId;

    #[test]
    fn ids_are_dense_and_never_reused() {
        let mut t: Table<FaceId, char> = Table::default();
        let a = t.push('a').unwrap_or(FaceId::from_raw(99));
        let b = t.push('b').unwrap_or(FaceId::from_raw(99));
        assert_eq!((a.raw(), b.raw()), (0, 1));
        assert_eq!(t.remove(a), Some('a'));
        assert_eq!(t.remove(a), None);
        let c = t.push('c').unwrap_or(FaceId::from_raw(99));
        assert_eq!(c.raw(), 2);
        assert_eq!((t.len(), t.slots()), (2, 3));
        let walk: Vec<_> = t.iter().map(|(id, v)| (id.raw(), *v)).collect();
        assert_eq!(walk, [(1, 'b'), (2, 'c')]);
        let back: Vec<_> = t.iter().rev().map(|(id, _)| id.raw()).collect();
        assert_eq!(back, [2, 1]);
        let mut it = t.iter();
        assert_eq!(it.len(), 2);
        it.next();
        assert_eq!(it.len(), 1);
    }

    #[test]
    fn a_clone_shares_until_written() {
        let mut t: Table<FaceId, u8> = Table::with_first(1);
        let u = t.clone();
        assert!(t.shares(&u));
        // A miss copies nothing.
        assert!(t.get_mut(FaceId::from_raw(5)).is_none());
        assert!(t.shares(&u));
        if let Some(v) = t.get_mut(FaceId::from_raw(0)) {
            *v = 2;
        }
        assert!(!t.shares(&u));
        assert_eq!(u.get(FaceId::from_raw(0)), Some(&1));
        assert_eq!(t.get(FaceId::from_raw(0)), Some(&2));
    }
}
