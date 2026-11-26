//! Generational ids over any table: a deleted slot can be reused, and an id kept from before the
//! deletion is refused instead of silently naming the new occupant.

use crate::table::Table;

/// An index plus the generation of the slot when the id was issued.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenId {
    pub index: u32,
    pub gen: u32,
}

#[derive(Clone, Debug)]
pub struct Slot<T> {
    pub gen: u32,
    pub item: Option<T>,
}

/// A table of slots with a free list. The free list is per draft: it is rebuilt by `finish` in a
/// real design; here it is a plain `Vec` cloned with the table.
#[derive(Clone, Debug)]
pub struct GenTable<T: Clone + Send + Sync, Tb: Table<Slot<T>>> {
    slots: Tb,
    free: Vec<u32>,
    _p: std::marker::PhantomData<T>,
}

impl<T: Clone + Send + Sync, Tb: Table<Slot<T>>> GenTable<T, Tb> {
    pub fn new() -> Self {
        Self { slots: Tb::from_vec(Vec::new()), free: Vec::new(), _p: std::marker::PhantomData }
    }
    pub fn insert(&mut self, item: T) -> GenId {
        if let Some(i) = self.free.pop() {
            if let Some(s) = self.slots.get_mut(i as usize) {
                s.item = Some(item);
                return GenId { index: i, gen: s.gen };
            }
        }
        let index = self.slots.len() as u32;
        self.slots.push(Slot { gen: 0, item: Some(item) });
        GenId { index, gen: 0 }
    }
    pub fn get(&self, id: GenId) -> Option<&T> {
        let s = self.slots.get(id.index as usize)?;
        if s.gen == id.gen {
            s.item.as_ref()
        } else {
            None
        }
    }
    pub fn remove(&mut self, id: GenId) -> Option<T> {
        let s = self.slots.get_mut(id.index as usize)?;
        if s.gen != id.gen {
            return None;
        }
        let out = s.item.take();
        s.gen += 1;
        self.free.push(id.index);
        out
    }
}

impl<T: Clone + Send + Sync, Tb: Table<Slot<T>>> Default for GenTable<T, Tb> {
    fn default() -> Self {
        Self::new()
    }
}
