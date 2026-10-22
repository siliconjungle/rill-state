//! Safe grouped-byte COW map experiment. No revision/feed/transaction contract.
#![forbid(unsafe_code)]
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    sync::Arc,
};

const EMPTY: u8 = 0x80;
const DELETED: u8 = 0xfe;
const HIGH: u64 = 0x8080_8080_8080_8080;
const ONES: u64 = 0x0101_0101_0101_0101;
#[inline]
fn candidates(word: u64, byte: u8) -> u64 {
    // Word zero-byte detection may include an adjacent low-bit false positive.
    // Complete key equality is mandatory; EMPTY/DELETED have exact masks under
    // this encoding because their adjacent byte values are never installed.
    let x = word ^ (u64::from(byte) * ONES);
    x.wrapping_sub(ONES) & !x & HIGH
}
#[inline]
fn fingerprint(hash: u64) -> u8 {
    (hash >> 57) as u8
}
fn limit<const L: usize>(capacity: usize) -> usize {
    capacity / 100 * L + capacity % 100 * L / 100
}
#[derive(Clone)]
struct Table<K, V> {
    controls: Box<[u64]>,
    rows: Vec<Option<(K, V)>>,
    len: usize,
    deleted: usize,
}
#[derive(Debug)]
struct Search {
    found: Option<usize>,
    vacant: Option<usize>,
}
impl<K, V> Table<K, V> {
    fn empty(capacity: usize) -> Self {
        Self {
            controls: vec![u64::from(EMPTY) * ONES; capacity / 8].into_boxed_slice(),
            rows: std::iter::repeat_with(|| None).take(capacity).collect(),
            len: 0,
            deleted: 0,
        }
    }
    #[inline]
    fn control(&self, slot: usize) -> u8 {
        (self.controls[slot / 8] >> (slot % 8 * 8)) as u8
    }
    #[inline]
    fn set_control(&mut self, slot: usize, byte: u8) {
        let shift = slot % 8 * 8;
        let word = &mut self.controls[slot / 8];
        *word = (*word & !(0xffu64 << shift)) | (u64::from(byte) << shift);
    }
    #[inline]
    fn search<Q: Eq + ?Sized, const G: usize, const INSERT: bool>(
        &self,
        key: &Q,
        hash: u64,
    ) -> Search
    where
        K: Borrow<Q>,
    {
        let groups = self.rows.len() / G;
        let home = hash as usize & (groups - 1);
        let tag = fingerprint(hash);
        let mut vacant = None;
        for offset in 0..groups {
            let group = (home + offset) & (groups - 1);
            let mut empty = false;
            for w in 0..G / 8 {
                let word_index = group * (G / 8) + w;
                let word = self.controls[word_index];
                let mut matches = candidates(word, tag);
                while matches != 0 {
                    let slot = word_index * 8 + matches.trailing_zeros() as usize / 8;
                    if let Some((stored, _)) = &self.rows[slot] {
                        if stored.borrow() == key {
                            return Search {
                                found: Some(slot),
                                vacant,
                            };
                        }
                    }
                    matches &= matches - 1;
                }
                empty |= candidates(word, EMPTY) != 0;
                if INSERT && vacant.is_none() {
                    let available = word & HIGH;
                    if available != 0 {
                        vacant = Some(word_index * 8 + available.trailing_zeros() as usize / 8);
                    }
                }
            }
            if empty {
                return Search {
                    found: None,
                    vacant,
                };
            }
        }
        Search {
            found: None,
            vacant,
        }
    }
    fn vacant<const G: usize>(&self, hash: u64) -> usize {
        let groups = self.rows.len() / G;
        let home = hash as usize & (groups - 1);
        for offset in 0..groups {
            let group = (home + offset) & (groups - 1);
            for w in 0..G / 8 {
                let word_index = group * (G / 8) + w;
                let available = self.controls[word_index] & HIGH;
                if available != 0 {
                    return word_index * 8 + available.trailing_zeros() as usize / 8;
                }
            }
        }
        panic!("internal full-table invariant");
    }
}

/// Experimental whole-table COW map. LOAD is 25/50/70/85 and GROUP is 8/16/32.
/// Keys must retain stable Hash/Eq behavior; interior mutability is the caller's
/// responsibility. Raw operations are not transactional batches. Old roots are
/// immutable under ordinary Clone/Hash/Eq implementations, including during a
/// failed clone or reconstruction hash. User Drop failures may occur after an
/// operation has installed its result; the map remains structurally usable.
pub struct GroupByteMap<K, V, S = RandomState, const LOAD: usize = 70, const GROUP: usize = 8> {
    table: Arc<Table<K, V>>,
    hasher: S,
}
impl<K, V, S: Clone, const L: usize, const G: usize> Clone for GroupByteMap<K, V, S, L, G> {
    fn clone(&self) -> Self {
        Self {
            table: self.table.clone(),
            hasher: self.hasher.clone(),
        }
    }
}
impl<K, V, S: BuildHasher + Default, const L: usize, const G: usize> Default
    for GroupByteMap<K, V, S, L, G>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<K, V, S: BuildHasher, const L: usize, const G: usize> GroupByteMap<K, V, S, L, G> {
    pub fn with_hasher(hasher: S) -> Self {
        assert!([25, 50, 70, 85].contains(&L));
        assert!([8, 16, 32].contains(&G));
        Self {
            table: Arc::new(Table::empty(G * 2)),
            hasher,
        }
    }
    #[inline]
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.table
            .search::<Q, G, false>(key, self.hasher.hash_one(key))
            .found
            .map(|slot| &self.table.rows[slot].as_ref().unwrap().1)
    }
    pub fn len(&self) -> usize {
        self.table.len
    }
    pub fn is_empty(&self) -> bool {
        self.table.len == 0
    }
    pub fn capacity(&self) -> usize {
        self.table.rows.len()
    }
    pub fn deleted_slots(&self) -> usize {
        self.table.deleted
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.table
            .rows
            .iter()
            .filter_map(|row| row.as_ref().map(|(k, v)| (k, v)))
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher, const L: usize, const G: usize>
    GroupByteMap<K, V, S, L, G>
{
    fn rebuild(&mut self, capacity: usize) {
        // No movement occurs until every user hash, allocation and potentially
        // failing COW clone has succeeded. The final moves invoke no user code.
        let hashes: Vec<_> = self
            .table
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| row.as_ref().map(|(key, _)| (i, self.hasher.hash_one(key))))
            .collect();
        let mut next = Table::empty(capacity);
        let table = Arc::make_mut(&mut self.table);
        for (i, hash) in hashes {
            let slot = next.vacant::<G>(hash);
            next.rows[slot] = table.rows[i].take();
            next.set_control(slot, fingerprint(hash));
            next.len += 1;
        }
        *table = next;
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let hash = self.hasher.hash_one(&key);
        let mut result = self.table.search::<K, G, true>(&key, hash);
        if let Some(slot) = result.found {
            return Some(std::mem::replace(
                &mut Arc::make_mut(&mut self.table).rows[slot]
                    .as_mut()
                    .unwrap()
                    .1,
                value,
            ));
        }
        let capacity = self.capacity();
        if self.table.len + self.table.deleted + 1 > limit::<L>(capacity) {
            let next = if self.table.len < limit::<L>(capacity) / 2 {
                capacity
            } else {
                capacity.checked_mul(2).expect("capacity exhausted")
            };
            self.rebuild(next);
            result = self.table.search::<K, G, true>(&key, hash);
        }
        let slot = result.vacant.expect("internal vacant-slot invariant");
        let table = Arc::make_mut(&mut self.table);
        if table.control(slot) == DELETED {
            table.deleted -= 1;
        }
        assert!(table.rows[slot].is_none());
        table.rows[slot] = Some((key, value));
        table.set_control(slot, fingerprint(hash));
        table.len += 1;
        None
    }
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let slot = self
            .table
            .search::<Q, G, false>(key, self.hasher.hash_one(key))
            .found?;
        let table = Arc::make_mut(&mut self.table);
        let (stored, value) = table.rows[slot].take().unwrap();
        table.set_control(slot, DELETED);
        table.len -= 1;
        table.deleted += 1;
        drop(stored);
        Some(value)
    }
}

#[cfg(test)]
mod masks {
    use super::*;
    #[test]
    fn byte_filter_has_no_false_negatives_and_status_filters_are_exact() {
        let tags: Vec<_> = (0..128).chain([EMPTY, DELETED]).collect();
        for &a in &tags {
            for &b in &tags {
                let word = u64::from_le_bytes([a, b, a, b, a, b, a, b]);
                for tag in (0..128).chain([EMPTY, DELETED]) {
                    let mask = candidates(word, tag);
                    for (i, actual) in word.to_le_bytes().into_iter().enumerate() {
                        if actual == tag {
                            assert_ne!(mask & (0x80 << (i * 8)), 0);
                        }
                        if tag >= 128 {
                            assert_eq!(mask & (0x80 << (i * 8)) != 0, actual == tag);
                        }
                    }
                }
            }
        }
        let false_positive = u64::from_le_bytes([0, 1, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY]);
        assert_ne!(candidates(false_positive, 0) & 0x8000, 0);
    }
}
