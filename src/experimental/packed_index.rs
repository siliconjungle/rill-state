//! Dense inline rows with packed cached-hash/row-index control words.
use super::{HasherPolicy, SharedHasher};
use crate::Mutation;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    sync::Arc,
};

#[derive(Clone)]
struct Table<K, V> {
    controls: Vec<u64>,
    rows: Vec<(K, V)>,
}
fn row(word: u64) -> usize {
    ((word as u32) - 1) as usize
}
fn hash(word: u64) -> u32 {
    (word >> 32) as u32
}
fn packed(hash: u32, index: usize) -> u64 {
    let index = u32::try_from(index).expect("packed row range exhausted");
    (u64::from(hash) << 32) | u64::from(index.checked_add(1).expect("packed row range exhausted"))
}
fn fill<const L: usize>(capacity: usize) -> usize {
    // Avoid overflow even at the supported capacity limit.
    capacity / 100 * L + capacity % 100 * L / 100
}
impl<K: Eq, V> Table<K, V> {
    fn find<Q: Eq + ?Sized>(&self, key: &Q, cached: u32) -> (usize, bool)
    where
        K: Borrow<Q>,
    {
        let mask = self.controls.len() - 1;
        let mut slot = cached as usize & mask;
        loop {
            let word = self.controls[slot];
            if word == 0 {
                return (slot, false);
            }
            if hash(word) == cached && self.rows[row(word)].0.borrow() == key {
                return (slot, true);
            }
            slot = (slot + 1) & mask;
        }
    }
    fn grow(&mut self) {
        let capacity = self
            .controls
            .len()
            .checked_mul(2)
            .expect("capacity exhausted");
        // The cached 32 bits must include every home-slot bit. On 32-bit hosts
        // allocation arithmetic can reject growth earlier than this bound.
        assert!(
            capacity as u64 <= 1u64 << 32,
            "packed index capacity exhausted"
        );
        let mut controls = vec![0; capacity];
        let mask = capacity - 1;
        for &word in &self.controls {
            if word != 0 {
                let mut slot = hash(word) as usize & mask;
                while controls[slot] != 0 {
                    slot = (slot + 1) & mask;
                }
                controls[slot] = word;
            }
        }
        self.controls = controls;
    }
    fn append<const L: usize>(&mut self, key: K, value: V, cached: u32, mut slot: usize) {
        assert!(
            self.rows.len() < u32::MAX as usize,
            "packed row range exhausted"
        );
        if self.rows.len() + 1 > fill::<L>(self.controls.len()) {
            self.grow();
            // This key is already known absent. No user Eq/Hash call is needed.
            let mask = self.controls.len() - 1;
            slot = cached as usize & mask;
            while self.controls[slot] != 0 {
                slot = (slot + 1) & mask;
            }
        }
        let word = packed(cached, self.rows.len());
        self.rows.push((key, value));
        self.controls[slot] = word;
    }
    fn moved_slot<S: BuildHasher>(&self, slot: usize, hasher: &S) -> Option<usize>
    where
        K: Hash,
    {
        let index = row(self.controls[slot]);
        let last = self.rows.len() - 1;
        if index == last {
            return None;
        }
        let last_key = &self.rows[last].0;
        let last_hash = hasher.hash_one(last_key) as u32;
        let (last_slot, present) = self.find(last_key, last_hash);
        assert!(
            present && row(self.controls[last_slot]) == last,
            "stored key changed hash/equality"
        );
        Some(last_slot)
    }
    fn erase(&mut self, mut hole: usize, moved_slot: Option<usize>) -> (K, V) {
        let index = row(self.controls[hole]);
        if let Some(slot) = moved_slot {
            self.controls[slot] = packed(hash(self.controls[slot]), index);
        }
        self.controls[hole] = 0;
        let mask = self.controls.len() - 1;
        let mut scan = (hole + 1) & mask;
        while self.controls[scan] != 0 {
            let home = hash(self.controls[scan]) as usize & mask;
            if scan.wrapping_sub(home) & mask >= hole.wrapping_sub(home) & mask {
                self.controls[hole] = self.controls[scan];
                self.controls[scan] = 0;
                hole = scan;
            }
            scan = (scan + 1) & mask;
        }
        // No user code is invoked between changing indices and moving rows.
        // The caller drops the removed key/value only after this is consistent.
        self.rows.swap_remove(index)
    }
}
type BuilderMarker<S, HP> = PhantomData<fn() -> (S, HP)>;

/// Experimental whole-table COW map with dense rows and 8-byte index words.
/// Each word caches only the low 32 bits of the builder hash, plus a row index.
/// Full key equality preserves identity even when these cached hashes collide.
/// Truncation is a performance/security tradeoff, not a unique key encoding.
///
/// Row count is at most u32::MAX and index capacity at most 2^32 (allocations may
/// fail earlier). LOAD is a maximum fill percentage in 20..=90. Iteration follows
/// dense storage order, which swap removal changes. Snapshots share the entire
/// table; a write with retained owners clones every row and the index. Mutations
/// are not transactions: a panic in a batch can leave earlier operations applied.
/// DETACH_ONCE acquires the writable table once for each nonempty batch; the
/// default detaches lazily per mutation. Empty batches never detach.
pub struct PackedIndexMap<
    K,
    V,
    S = RandomState,
    const LOAD: usize = 70,
    HP: HasherPolicy<S> = SharedHasher,
    const DETACH_ONCE: bool = false,
> {
    table: Arc<Table<K, V>>,
    hasher: HP::Stored,
    marker: BuilderMarker<S, HP>,
}
impl<K, V, S, const L: usize, HP: HasherPolicy<S>, const B: bool> Clone
    for PackedIndexMap<K, V, S, L, HP, B>
{
    fn clone(&self) -> Self {
        Self {
            table: self.table.clone(),
            hasher: self.hasher.clone(),
            marker: PhantomData,
        }
    }
}
impl<
        K: Clone + Hash + Eq,
        V: Clone,
        S: BuildHasher + Default,
        const L: usize,
        HP: HasherPolicy<S>,
        const B: bool,
    > Default for PackedIndexMap<K, V, S, L, HP, B>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<
        K: Clone + Hash + Eq,
        V: Clone,
        S: BuildHasher,
        const L: usize,
        HP: HasherPolicy<S>,
        const B: bool,
    > PackedIndexMap<K, V, S, L, HP, B>
{
    /// Construct an empty map with the given hash builder.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        Self {
            table: Arc::new(Table {
                controls: vec![0; 16],
                rows: Vec::new(),
            }),
            hasher: HP::store(hasher),
            marker: PhantomData,
        }
    }
    /// Read a compatible borrowed key, without constructing a stored key.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let cached = HP::original(&self.hasher).hash_one(key) as u32;
        let (slot, found) = self.table.find(key, cached);
        found.then(|| &self.table.rows[row(self.table.controls[slot])].1)
    }
    /// Insert or replace, retaining the original canonical stored key.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let cached = HP::original(&self.hasher).hash_one(&key) as u32;
        let (slot, found) = self.table.find(&key, cached);
        let table = Arc::make_mut(&mut self.table);
        if found {
            Some(std::mem::replace(
                &mut table.rows[row(table.controls[slot])].1,
                value,
            ))
        } else {
            table.append::<L>(key, value, cached, slot);
            None
        }
    }
    /// Remove and repair dense/probing indices before dropping the stored key.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let cached = HP::original(&self.hasher).hash_one(key) as u32;
        let (slot, found) = self.table.find(key, cached);
        if !found {
            return None;
        }
        // A user hash/equality panic here occurs before any mutation.
        let moved_slot = self.table.moved_slot(slot, HP::original(&self.hasher));
        let (removed_key, value) = Arc::make_mut(&mut self.table).erase(slot, moved_slot);
        drop(removed_key);
        Some(value)
    }
    /// Apply an ordered batch. Existing-key puts clone only incoming values,
    /// rather than cloning and discarding a replacement key.
    pub fn apply(&mut self, ops: &[Mutation<K, V>]) {
        if B {
            if ops.is_empty() {
                return;
            }
            let hasher = HP::original(&self.hasher);
            // This policy deliberately detaches for any nonempty batch, even
            // one containing only absent deletes. Include that cost in timing.
            let table = Arc::make_mut(&mut self.table);
            for op in ops {
                let key = match op {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let cached = hasher.hash_one(key) as u32;
                let (slot, found) = table.find(key, cached);
                match op {
                    Mutation::Put(key, value) => {
                        if found {
                            let value = value.clone();
                            let old = std::mem::replace(
                                &mut table.rows[row(table.controls[slot])].1,
                                value,
                            );
                            drop(old);
                        } else {
                            let key = key.clone();
                            let value = value.clone();
                            table.append::<L>(key, value, cached, slot);
                        }
                    }
                    Mutation::Delete(_) => {
                        if found {
                            let moved = table.moved_slot(slot, hasher);
                            let removed = table.erase(slot, moved);
                            drop(removed);
                        }
                    }
                }
            }
            return;
        }
        for op in ops {
            match op {
                Mutation::Put(key, value) => {
                    let cached = HP::original(&self.hasher).hash_one(key) as u32;
                    let (slot, found) = self.table.find(key, cached);
                    if found {
                        let value = value.clone();
                        let table = Arc::make_mut(&mut self.table);
                        let old =
                            std::mem::replace(&mut table.rows[row(table.controls[slot])].1, value);
                        drop(old);
                    } else {
                        let key = key.clone();
                        let value = value.clone();
                        Arc::make_mut(&mut self.table).append::<L>(key, value, cached, slot);
                    }
                }
                Mutation::Delete(key) => {
                    self.remove(key);
                }
            }
        }
    }
    /// Number of live rows.
    pub fn len(&self) -> usize {
        self.table.rows.len()
    }
    /// Whether no rows remain.
    pub fn is_empty(&self) -> bool {
        self.table.rows.is_empty()
    }
    /// Iterate complete live rows without scanning empty index slots.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&K, &V)> {
        self.table.rows.iter().map(|(key, value)| (key, value))
    }
}

#[cfg(test)]
mod layout {
    use super::*;
    #[test]
    fn control_extremes_and_dense_row_size() {
        assert_eq!(std::mem::size_of::<(u64, [u64; 4])>(), 40);
        for cached in [0, 1, u32::MAX] {
            for index in [0, 1, u32::MAX as usize - 1] {
                let word = packed(cached, index);
                assert_ne!(word, 0);
                assert_eq!(hash(word), cached);
                assert_eq!(row(word), index);
            }
        }
        assert!(std::panic::catch_unwind(|| packed(0, u32::MAX as usize)).is_err());
    }
}
