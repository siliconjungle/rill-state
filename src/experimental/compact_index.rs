//! Dense rows with adaptive 4/8-byte probing words and staged hash widening.
use super::{HasherPolicy, SharedHasher};
use crate::Mutation;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    sync::Arc,
};

trait Word: Copy + Default + PartialEq {
    fn cached(raw: u64) -> u32;
    fn hash(self) -> u32;
    fn row(self) -> usize;
    fn pack(hash: u32, row: usize) -> Self;
}
impl Word for u32 {
    fn cached(raw: u64) -> u32 {
        raw as u16 as u32
    }
    fn hash(self) -> u32 {
        self >> 16
    }
    fn row(self) -> usize {
        ((self as u16) - 1) as usize
    }
    fn pack(hash: u32, row: usize) -> Self {
        let low = u16::try_from(row)
            .expect("narrow row range exhausted")
            .checked_add(1)
            .expect("narrow row range exhausted");
        (hash << 16) | u32::from(low)
    }
}
impl Word for u64 {
    fn cached(raw: u64) -> u32 {
        raw as u32
    }
    fn hash(self) -> u32 {
        (self >> 32) as u32
    }
    fn row(self) -> usize {
        ((self as u32) - 1) as usize
    }
    fn pack(hash: u32, row: usize) -> Self {
        let low = u32::try_from(row)
            .expect("wide row range exhausted")
            .checked_add(1)
            .expect("wide row range exhausted");
        (u64::from(hash) << 32) | u64::from(low)
    }
}
fn find<W: Word, K: Borrow<Q>, V, Q: Eq + ?Sized>(
    words: &[W],
    rows: &[(K, V)],
    key: &Q,
    raw: u64,
) -> (usize, bool) {
    let cached = W::cached(raw);
    let mask = words.len() - 1;
    let mut slot = cached as usize & mask;
    loop {
        let word = words[slot];
        if word == W::default() {
            return (slot, false);
        }
        if word.hash() == cached && rows[word.row()].0.borrow() == key {
            return (slot, true);
        }
        slot = (slot + 1) & mask;
    }
}
fn get<'a, W: Word, K: Borrow<Q>, V, Q: Eq + ?Sized>(
    words: &[W],
    rows: &'a [(K, V)],
    key: &Q,
    raw: u64,
) -> Option<&'a V> {
    let cached = W::cached(raw);
    let mask = words.len() - 1;
    let mut slot = cached as usize & mask;
    loop {
        let word = words[slot];
        if word == W::default() {
            return None;
        }
        if word.hash() == cached {
            let entry = &rows[word.row()];
            if entry.0.borrow() == key {
                return Some(&entry.1);
            }
        }
        slot = (slot + 1) & mask;
    }
}
fn vacant<W: Word>(words: &[W], raw: u64) -> usize {
    let mask = words.len() - 1;
    let mut slot = W::cached(raw) as usize & mask;
    while words[slot] != W::default() {
        slot = (slot + 1) & mask;
    }
    slot
}
fn reindex<W: Word>(old: &[W], capacity: usize) -> Vec<W> {
    let mut words = vec![W::default(); capacity];
    for &word in old {
        if word != W::default() {
            let slot = vacant(&words, u64::from(word.hash()));
            words[slot] = word;
        }
    }
    words
}
fn erase<W: Word>(words: &mut [W], mut hole: usize, moved: Option<usize>) -> usize {
    let row = words[hole].row();
    if let Some(slot) = moved {
        words[slot] = W::pack(words[slot].hash(), row);
    }
    words[hole] = W::default();
    let mask = words.len() - 1;
    let mut scan = (hole + 1) & mask;
    while words[scan] != W::default() {
        let home = words[scan].hash() as usize & mask;
        if scan.wrapping_sub(home) & mask >= hole.wrapping_sub(home) & mask {
            words[hole] = words[scan];
            words[scan] = W::default();
            hole = scan;
        }
        scan = (scan + 1) & mask;
    }
    row
}
#[derive(Clone)]
enum Controls {
    Narrow(Box<[u32]>),
    Wide(Box<[u64]>),
}
impl Controls {
    fn len(&self) -> usize {
        match self {
            Self::Narrow(w) => w.len(),
            Self::Wide(w) => w.len(),
        }
    }
    fn word_bytes(&self) -> usize {
        match self {
            Self::Narrow(_) => 4,
            Self::Wide(_) => 8,
        }
    }
    fn row(&self, slot: usize) -> usize {
        match self {
            Self::Narrow(w) => w[slot].row(),
            Self::Wide(w) => w[slot].row(),
        }
    }
    fn vacant(&self, raw: u64) -> usize {
        match self {
            Self::Narrow(w) => vacant(w, raw),
            Self::Wide(w) => vacant(w, raw),
        }
    }
    fn insert(&mut self, slot: usize, raw: u64, row: usize) {
        match self {
            Self::Narrow(w) => w[slot] = u32::pack(u32::cached(raw), row),
            Self::Wide(w) => w[slot] = u64::pack(u64::cached(raw), row),
        }
    }
    fn grow<K: Hash, V, S: BuildHasher>(&mut self, rows: &[(K, V)], hasher: &S) {
        let capacity = self.len().checked_mul(2).expect("capacity exhausted");
        assert!(
            capacity as u64 <= 1u64 << 32,
            "wide index capacity exhausted"
        );
        let next = match self {
            Self::Narrow(old) if capacity <= 1usize << 16 => {
                Self::Narrow(reindex(old, capacity).into_boxed_slice())
            }
            Self::Narrow(_) => {
                // Reconstruct the additional hash bits from complete keys. The
                // installed controls remain unchanged if any user Hash panics.
                let mut words = vec![0u64; capacity];
                for (row, (key, _)) in rows.iter().enumerate() {
                    let raw = hasher.hash_one(key);
                    let slot = vacant(&words, raw);
                    words[slot] = u64::pack(u64::cached(raw), row);
                }
                Self::Wide(words.into_boxed_slice())
            }
            Self::Wide(old) => Self::Wide(reindex(old, capacity).into_boxed_slice()),
        };
        *self = next;
    }
}
#[derive(Clone)]
struct Table<K, V> {
    controls: Controls,
    rows: Vec<(K, V)>,
}
fn fill<const L: usize>(capacity: usize) -> usize {
    capacity / 100 * L + capacity % 100 * L / 100
}
impl<K: Eq, V> Table<K, V> {
    fn find<Q: Eq + ?Sized>(&self, key: &Q, raw: u64) -> (usize, bool)
    where
        K: Borrow<Q>,
    {
        match &self.controls {
            Controls::Narrow(w) => find(w, &self.rows, key, raw),
            Controls::Wide(w) => find(w, &self.rows, key, raw),
        }
    }
    fn get<Q: Eq + ?Sized>(&self, key: &Q, raw: u64) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        match &self.controls {
            Controls::Narrow(w) => get(w, &self.rows, key, raw),
            Controls::Wide(w) => get(w, &self.rows, key, raw),
        }
    }
    fn append<const L: usize, S: BuildHasher>(
        &mut self,
        key: K,
        value: V,
        raw: u64,
        mut slot: usize,
        hasher: &S,
    ) where
        K: Hash,
    {
        assert!(
            self.rows.len() < u32::MAX as usize,
            "wide row range exhausted"
        );
        if self.rows.len() + 1 > fill::<L>(self.controls.len()) {
            self.controls.grow(&self.rows, hasher);
            slot = self.controls.vacant(raw);
        }
        let row = self.rows.len();
        self.rows.push((key, value));
        self.controls.insert(slot, raw, row);
    }
    fn moved_slot<S: BuildHasher>(&self, slot: usize, hasher: &S) -> Option<usize>
    where
        K: Hash,
    {
        let index = self.controls.row(slot);
        let last = self.rows.len() - 1;
        if index == last {
            return None;
        }
        let key = &self.rows[last].0;
        let raw = hasher.hash_one(key);
        let (slot, present) = self.find(key, raw);
        assert!(
            present && self.controls.row(slot) == last,
            "stored key changed hash/equality"
        );
        Some(slot)
    }
    fn erase(&mut self, slot: usize, moved: Option<usize>) -> (K, V) {
        let row = match &mut self.controls {
            Controls::Narrow(w) => erase(w, slot, moved),
            Controls::Wide(w) => erase(w, slot, moved),
        };
        self.rows.swap_remove(row)
    }
}
type BuilderMarker<S, HP> = PhantomData<fn() -> (S, HP)>;

/// Experimental dense COW map with adaptive or always-wide index controls.
/// ADAPTIVE starts with four-byte words caching 16 hash bits and a 16-bit row
/// index. Before capacity exceeds 65536, it stages eight-byte words by hashing
/// every existing key once. A failed widening hash leaves the old table valid.
/// Cached hashes always require full key equality. The wide-only control uses
/// the same implementation and eight-byte words from construction.
///
/// LOAD is 20..=90. Maximum row count is u32::MAX and index capacity is 2^32;
/// allocation may fail earlier. Whole-table COW still clones every retained
/// owned payload. DETACH_ONCE acquires a writable table for every nonempty batch,
/// even one containing only absent deletes. Raw batches are not transactions.
pub struct CompactIndexMap<
    K,
    V,
    S = RandomState,
    const LOAD: usize = 70,
    HP: HasherPolicy<S> = SharedHasher,
    const DETACH_ONCE: bool = false,
    const ADAPTIVE: bool = true,
> {
    table: Arc<Table<K, V>>,
    hasher: HP::Stored,
    marker: BuilderMarker<S, HP>,
}
impl<K, V, S, const L: usize, HP: HasherPolicy<S>, const B: bool, const A: bool> Clone
    for CompactIndexMap<K, V, S, L, HP, B, A>
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
        const A: bool,
    > Default for CompactIndexMap<K, V, S, L, HP, B, A>
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
        const A: bool,
    > CompactIndexMap<K, V, S, L, HP, B, A>
{
    /// Construct an empty map with the given hash builder.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        Self {
            table: Arc::new(Table {
                controls: if A {
                    Controls::Narrow(vec![0; 16].into_boxed_slice())
                } else {
                    Controls::Wide(vec![0; 16].into_boxed_slice())
                },
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
        let cached = HP::original(&self.hasher).hash_one(key);
        self.table.get(key, cached)
    }
    /// Insert or replace, retaining the original canonical stored key.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let cached = HP::original(&self.hasher).hash_one(&key);
        let (slot, found) = self.table.find(&key, cached);
        let table = Arc::make_mut(&mut self.table);
        if found {
            Some(std::mem::replace(
                &mut table.rows[table.controls.row(slot)].1,
                value,
            ))
        } else {
            table.append::<L, _>(key, value, cached, slot, HP::original(&self.hasher));
            None
        }
    }
    /// Remove and repair dense/probing indices before dropping the stored key.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let cached = HP::original(&self.hasher).hash_one(key);
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
                let cached = hasher.hash_one(key);
                let (slot, found) = table.find(key, cached);
                match op {
                    Mutation::Put(key, value) => {
                        if found {
                            let value = value.clone();
                            let old = std::mem::replace(
                                &mut table.rows[table.controls.row(slot)].1,
                                value,
                            );
                            drop(old);
                        } else {
                            let key = key.clone();
                            let value = value.clone();
                            table.append::<L, _>(
                                key,
                                value,
                                cached,
                                slot,
                                HP::original(&self.hasher),
                            );
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
                    let cached = HP::original(&self.hasher).hash_one(key);
                    let (slot, found) = self.table.find(key, cached);
                    if found {
                        let value = value.clone();
                        let table = Arc::make_mut(&mut self.table);
                        let old =
                            std::mem::replace(&mut table.rows[table.controls.row(slot)].1, value);
                        drop(old);
                    } else {
                        let key = key.clone();
                        let value = value.clone();
                        Arc::make_mut(&mut self.table).append::<L, _>(
                            key,
                            value,
                            cached,
                            slot,
                            HP::original(&self.hasher),
                        );
                    }
                }
                Mutation::Delete(key) => {
                    self.remove(key);
                }
            }
        }
    }
    /// Bytes per index word in the currently installed representation.
    pub fn index_word_bytes(&self) -> usize {
        self.table.controls.word_bytes()
    }
    /// Number of index slots, including empty slots.
    pub fn index_capacity(&self) -> usize {
        self.table.controls.len()
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
    fn narrow_and_wide_words_keep_zero_empty_and_full_row_range() {
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(std::mem::size_of::<Controls>(), 24);
            assert_eq!(std::mem::size_of::<Table<u64, [u64; 4]>>(), 48);
        }
        for cached in [0, 1, u16::MAX as u32] {
            for row in [0, 1, u16::MAX as usize - 1] {
                let word = u32::pack(cached, row);
                assert_ne!(word, 0);
                assert_eq!(word.hash(), cached);
                assert_eq!(word.row(), row);
            }
        }
        for cached in [0, 1, u32::MAX] {
            for row in [0, 1, u32::MAX as usize - 1] {
                let word = u64::pack(cached, row);
                assert_ne!(word, 0);
                assert_eq!(word.hash(), cached);
                assert_eq!(word.row(), row);
            }
        }
        assert!(std::panic::catch_unwind(|| u32::pack(0, u16::MAX as usize)).is_err());
        assert!(std::panic::catch_unwind(|| u64::pack(0, u32::MAX as usize)).is_err());
    }
}
