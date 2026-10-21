//! Generic, original map-layout experiments. These are unstable research APIs, not
//! the selected snapshot engine and not claims of superiority. Map mutations are not
//! transactions. The comparison wrapper supplies complete revision publication and
//! the ordered bounded change journal. No game types or identity-key shortcuts are used.

use crate::Mutation;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    sync::Arc,
};

#[path = "experimental/inline_slice.rs"]
mod inline_slice;
pub use inline_slice::InlineSliceMap;
#[path = "experimental/sharded_inline.rs"]
mod sharded_inline;
pub use sharded_inline::ShardedInlineMap;
#[path = "experimental/packed_index.rs"]
mod packed_index;
pub use packed_index::PackedIndexMap;
#[path = "experimental/compact_index.rs"]
mod compact_index;
pub use compact_index::CompactIndexMap;

#[derive(Clone)]
struct Entry<K, V> {
    hash: u64,
    key: K,
    value: V,
}
type OwnedPage<K, V> = Vec<Option<Entry<K, V>>>;
type Page<K, V> = Arc<OwnedPage<K, V>>;

/// Linear probing with contiguous entries and optional copy-on-write physical pages.
/// `PAGE=0` uses one flat page. Nonzero `PAGE` must be a power of two. `LOAD` is the
/// maximum occupancy percentage (20..=90). Cloning shares pages; writes clone touched
/// pages. The default hasher is the standard keyed builder; callers may choose another.
pub struct ProbeMap<K, V, S = RandomState, const LOAD: usize = 70, const PAGE: usize = 0> {
    pages: Vec<Page<K, V>>,
    len: usize,
    mask: usize,
    hasher: Arc<S>,
}
impl<K, V, S, const L: usize, const P: usize> Clone for ProbeMap<K, V, S, L, P> {
    fn clone(&self) -> Self {
        Self {
            pages: self.pages.clone(),
            len: self.len,
            mask: self.mask,
            hasher: self.hasher.clone(),
        }
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Default, const L: usize, const P: usize>
    Default for ProbeMap<K, V, S, L, P>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher, const L: usize, const P: usize>
    ProbeMap<K, V, S, L, P>
{
    /// Construct an empty map. Invalid constant layout parameters panic before allocation.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        assert!(P == 0 || P.is_power_of_two());
        let capacity = 16.max(P);
        let width = if P == 0 { capacity } else { P };
        let empty = Arc::new(vec![None; width]);
        Self {
            pages: vec![empty; capacity / width],
            len: 0,
            mask: capacity - 1,
            hasher: Arc::new(hasher),
        }
    }
    // P is a const layout selector; both branches specialize at compile time.
    #[allow(clippy::manual_checked_ops)]
    fn slot(&self, i: usize) -> &Option<Entry<K, V>> {
        if P == 0 {
            &self.pages[0][i]
        } else {
            &self.pages[i / P][i % P]
        }
    }
    #[allow(clippy::manual_checked_ops)]
    fn slot_mut(&mut self, i: usize) -> &mut Option<Entry<K, V>> {
        if P == 0 {
            &mut Arc::make_mut(&mut self.pages[0])[i]
        } else {
            &mut Arc::make_mut(&mut self.pages[i / P])[i % P]
        }
    }
    fn find<Q: Hash + Eq + ?Sized>(&self, key: &Q, hash: u64) -> (usize, bool)
    where
        K: Borrow<Q>,
    {
        self.find_by(hash, |stored| stored.borrow() == key)
    }
    fn find_by(&self, hash: u64, matches: impl Fn(&K) -> bool) -> (usize, bool) {
        let mut i = hash as usize & self.mask;
        loop {
            match self.slot(i) {
                None => return (i, false),
                Some(e) if e.hash == hash && matches(&e.key) => return (i, true),
                _ => i = (i + 1) & self.mask,
            }
        }
    }
    /// Read a key, including a compatible borrowed key such as `str` for `String`.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.get_by(self.hasher.hash_one(key), |stored| stored.borrow() == key)
    }
    fn get_by(&self, hash: u64, matches: impl Fn(&K) -> bool) -> Option<&V> {
        self.entry_by(hash, matches).map(|entry| &entry.value)
    }
    fn entry_by(&self, hash: u64, matches: impl Fn(&K) -> bool) -> Option<&Entry<K, V>> {
        let start = hash as usize & self.mask;
        if P == 0 {
            let page = &self.pages[0];
            // Walk each contiguous span directly. The iterator establishes the
            // bounds once, and a hit returns its value without indexing it again.
            for slot in page[start..].iter().chain(page[..start].iter()) {
                match slot {
                    None => return None,
                    Some(e) if e.hash == hash && matches(&e.key) => return Some(e),
                    _ => {}
                }
            }
            return None;
        }
        let mut i = start;
        loop {
            let page = &self.pages[i / P];
            for slot in &page[i % P..] {
                match slot {
                    None => return None,
                    Some(e) if e.hash == hash && matches(&e.key) => return Some(e),
                    _ => {}
                }
            }
            i = (((i / P) + 1) * P) & self.mask;
        }
    }
    /// Insert or replace; return the previous value. Existing stored keys are preserved.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let hash = self.hasher.hash_one(&key);
        let (mut i, found) = self.find(&key, hash);
        if found {
            return Some(std::mem::replace(
                &mut self.slot_mut(i).as_mut().unwrap().value,
                value,
            ));
        }
        if self.len + 1 > (self.mask + 1) * L / 100 {
            self.grow();
            i = self.find(&key, hash).0;
        }
        *self.slot_mut(i) = Some(Entry { hash, key, value });
        self.len += 1;
        None
    }
    /// Apply ordered mutations. The flat variant acquires its writable page once per
    /// batch instead of checking reference counts for every value-only replacement.
    /// This is a mutable map operation, not atomic revision publication.
    pub fn apply(&mut self, operations: &[Mutation<K, V>]) {
        if P != 0 {
            for op in operations {
                match op {
                    Mutation::Put(k, v) => {
                        self.insert(k.clone(), v.clone());
                    }
                    Mutation::Delete(k) => {
                        self.remove(k);
                    }
                }
            }
            return;
        }
        let mut next = 0;
        while next < operations.len() {
            let mask = self.mask;
            let page = Arc::make_mut(&mut self.pages[0]);
            while next < operations.len() {
                let key = match &operations[next] {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let hash = self.hasher.hash_one(key);
                let mut i = hash as usize & mask;
                while let Some(e) = &page[i] {
                    if e.hash == hash && &e.key == key {
                        break;
                    }
                    i = (i + 1) & mask;
                }
                match &operations[next] {
                    Mutation::Put(k, v) => {
                        if let Some(e) = &mut page[i] {
                            e.value = v.clone();
                        } else {
                            if self.len + 1 > (mask + 1) * L / 100 {
                                break;
                            }
                            page[i] = Some(Entry {
                                hash,
                                key: k.clone(),
                                value: v.clone(),
                            });
                            self.len += 1;
                        }
                    }
                    Mutation::Delete(_) => {
                        if let Some(removed) = page[i].take() {
                            self.len -= 1;
                            let mut hole = i;
                            let mut scan = (hole + 1) & mask;
                            while let Some(e) = &page[scan] {
                                let home = e.hash as usize & mask;
                                if scan.wrapping_sub(home) & mask >= hole.wrapping_sub(home) & mask
                                {
                                    page[hole] = page[scan].take();
                                    hole = scan;
                                }
                                scan = (scan + 1) & mask;
                            }
                            drop(removed);
                        }
                    }
                }
                next += 1;
            }
            if next < operations.len() {
                self.grow();
            }
        }
    }
    fn grow(&mut self) {
        let capacity = (self.mask + 1).checked_mul(2).expect("capacity exhausted");
        let width = if P == 0 { capacity } else { P };
        let empty = Arc::new(vec![None; width]);
        let mut grown = Self {
            pages: vec![empty; capacity / width],
            len: self.len,
            mask: capacity - 1,
            hasher: self.hasher.clone(),
        };
        // Complete every potentially panicking clone before moving unique pages.
        let prepared: Vec<Option<OwnedPage<K, V>>> = self
            .pages
            .iter_mut()
            .map(|page| {
                if Arc::get_mut(page).is_some() {
                    None
                } else {
                    Some((**page).clone())
                }
            })
            .collect();
        for (page, ready) in self.pages.iter_mut().zip(prepared) {
            let entries = ready.unwrap_or_else(|| {
                std::mem::take(Arc::get_mut(page).expect("unique page prepared before moving"))
            });
            for e in entries.into_iter().flatten() {
                let mut i = e.hash as usize & grown.mask;
                while grown.slot(i).is_some() {
                    i = (i + 1) & grown.mask;
                }
                *grown.slot_mut(i) = Some(e);
            }
        }
        // Install the complete map before releasing old user keys/values.
        std::mem::swap(self, &mut grown);
    }
    /// Remove a key. Backward shifting preserves lookup reachability without tombstones.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let hash = self.hasher.hash_one(key);
        let (hole, found) = self.find(key, hash);
        if !found {
            return None;
        }
        if P != 0 {
            self.prepare_removal(hole);
        }
        Some(self.remove_at(hole).value)
    }
    // Only used for a nonzero const page width. Detach the entire affected
    // cluster before opening its first hole; later shifts cannot clone values.
    #[allow(clippy::manual_checked_ops)]
    fn prepare_removal(&mut self, hole: usize) {
        let mut i = hole;
        let mut previous = usize::MAX;
        while self.slot(i).is_some() {
            let page = i / P;
            if page != previous {
                Arc::make_mut(&mut self.pages[page]);
                previous = page;
            }
            i = (i + 1) & self.mask;
        }
    }
    fn remove_at(&mut self, mut hole: usize) -> Entry<K, V> {
        let removed = self.slot_mut(hole).take().unwrap();
        self.len -= 1;
        let mut scan = (hole + 1) & self.mask;
        while let Some(e) = self.slot(scan) {
            let home = e.hash as usize & self.mask;
            if scan.wrapping_sub(home) & self.mask >= hole.wrapping_sub(home) & self.mask {
                let moved = self.slot_mut(scan).take();
                *self.slot_mut(hole) = moved;
                hole = scan;
            }
            scan = (scan + 1) & self.mask;
        }
        removed
    }
    /// Number of live keys.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no keys are live.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Visit every live binding in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.pages
            .iter()
            .flat_map(|p| p.iter())
            .filter_map(|e| e.as_ref())
            .map(|e| (&e.key, &e.value))
    }
}

/// Copy-on-write probing pages with independently shared keys and values. A page
/// copy clones reference counts rather than every untouched heap allocation. Updating
/// a binding keeps its canonical key allocation. This trades additional pointer reads
/// and record allocations for cheaper retained-page copies; it is not a selected default.
/// As with `ProbeMap`, mutations are not transactions. Values with interior mutability
/// remain the caller's responsibility. Removed/replaced values are returned as shared Arcs.
pub struct SharedMap<K, V, S = RandomState, const LOAD: usize = 70, const PAGE: usize = 16> {
    inner: ProbeMap<Arc<K>, Arc<V>, S, LOAD, PAGE>,
}
impl<K, V, S, const L: usize, const P: usize> Clone for SharedMap<K, V, S, L, P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<K: Hash + Eq, V, S: BuildHasher + Default, const L: usize, const P: usize> Default
    for SharedMap<K, V, S, L, P>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<K: Hash + Eq, V, S: BuildHasher, const L: usize, const P: usize> SharedMap<K, V, S, L, P> {
    /// Construct a shared-record map with the selected hasher and const layout.
    pub fn with_hasher(hasher: S) -> Self {
        Self {
            inner: ProbeMap::with_hasher(hasher),
        }
    }
    /// Look up a compatible borrowed key without allocating or cloning it.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.inner
            .get_by(self.inner.hasher.hash_one(key), |k| {
                k.as_ref().borrow() == key
            })
            .map(AsRef::as_ref)
    }
    fn place(&mut self, mut i: usize, hash: u64, key: Arc<K>, value: Arc<V>) {
        if self.inner.len + 1 > (self.inner.mask + 1) * L / 100 {
            self.inner.grow();
            i = self.inner.find_by(hash, |k| k.as_ref() == key.as_ref()).0;
        }
        *self.inner.slot_mut(i) = Some(Entry { hash, key, value });
        self.inner.len += 1;
    }
    /// Insert or replace, retaining a canonical stored key and returning the old value.
    pub fn insert(&mut self, key: K, value: V) -> Option<Arc<V>> {
        let hash = self.inner.hasher.hash_one(&key);
        let (i, found) = self.inner.find_by(hash, |stored| stored.as_ref() == &key);
        let value = Arc::new(value);
        if found {
            Some(std::mem::replace(
                &mut self.inner.slot_mut(i).as_mut().unwrap().value,
                value,
            ))
        } else {
            self.place(i, hash, Arc::new(key), value);
            None
        }
    }
    /// Remove a compatible borrowed key; retained snapshots keep the old allocation.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<Arc<V>>
    where
        K: Borrow<Q>,
    {
        let (i, found) = self.inner.find_by(self.inner.hasher.hash_one(key), |k| {
            k.as_ref().borrow() == key
        });
        found.then(|| self.inner.remove_at(i).value)
    }
    /// Apply borrowed ordered mutations. Existing keys and untouched values are shared.
    pub fn apply(&mut self, operations: &[Mutation<K, V>])
    where
        K: Clone,
        V: Clone,
    {
        for op in operations {
            match op {
                Mutation::Put(k, v) => {
                    let hash = self.inner.hasher.hash_one(k);
                    let (i, found) = self.inner.find_by(hash, |stored| stored.as_ref() == k);
                    let value = Arc::new(v.clone());
                    if found {
                        self.inner.slot_mut(i).as_mut().unwrap().value = value;
                    } else {
                        self.place(i, hash, Arc::new(k.clone()), value);
                    }
                }
                Mutation::Delete(k) => {
                    self.remove(k);
                }
            }
        }
    }
    /// Number of live keys.
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    /// Whether no keys are live.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    /// Visit all live bindings in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter().map(|(k, v)| (k.as_ref(), v.as_ref()))
    }
}

/// One immutable key/value allocation retained by a `RecordMap` snapshot. Values
/// with interior mutability still require application discipline, as with `Arc<T>`.
pub struct SharedRecord<K, V> {
    key: K,
    value: V,
}
impl<K, V> SharedRecord<K, V> {
    /// Stored key of this immutable record.
    pub fn key(&self) -> &K {
        &self.key
    }
    /// Stored value of this immutable record.
    pub fn value(&self) -> &V {
        &self.value
    }
}
struct RecordHandle<K, V>(Arc<SharedRecord<K, V>>);
impl<K, V> Clone for RecordHandle<K, V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<K: Hash, V> Hash for RecordHandle<K, V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.key.hash(state);
    }
}
impl<K: PartialEq, V> PartialEq for RecordHandle<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0.key == other.0.key
    }
}
impl<K: Eq, V> Eq for RecordHandle<K, V> {}

/// Probing pages whose slots contain a cached hash and one shared record handle.
/// Key and value occupy the same allocation, reducing slot width and reference
/// count traffic relative to `SharedMap`. Replacement clones the canonical stored
/// key into the new record rather than sharing its existing allocation. This is an explicit locality/
/// allocation tradeoff, not a selected default or a transactional mutation API.
pub struct RecordMap<K, V, S = RandomState, const LOAD: usize = 70, const PAGE: usize = 16> {
    inner: ProbeMap<RecordHandle<K, V>, (), S, LOAD, PAGE>,
}
impl<K, V, S, const L: usize, const P: usize> Clone for RecordMap<K, V, S, L, P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<K: Hash + Eq, V, S: BuildHasher + Default, const L: usize, const P: usize> Default
    for RecordMap<K, V, S, L, P>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<K: Hash + Eq, V, S: BuildHasher, const L: usize, const P: usize> RecordMap<K, V, S, L, P> {
    /// Construct a map with a chosen hash builder and the selected const layout.
    pub fn with_hasher(hasher: S) -> Self {
        Self {
            inner: ProbeMap::with_hasher(hasher),
        }
    }
    /// Look up a compatible borrowed key without allocation or reference-count work.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.inner
            .entry_by(self.inner.hasher.hash_one(key), |r| r.0.key.borrow() == key)
            .map(|entry| &entry.key.0.value)
    }
    /// Insert or replace an immutable record, preserving the canonical stored key.
    pub fn insert(&mut self, key: K, value: V) -> Option<Arc<SharedRecord<K, V>>>
    where
        K: Clone,
    {
        let hash = self.inner.hasher.hash_one(&key);
        let (i, found) = self.inner.find_by(hash, |r| r.0.key == key);
        let key = if found {
            self.inner.slot(i).as_ref().unwrap().key.0.key.clone()
        } else {
            key
        };
        self.put_at(i, found, hash, key, value)
    }
    fn put_at(
        &mut self,
        mut i: usize,
        found: bool,
        hash: u64,
        key: K,
        value: V,
    ) -> Option<Arc<SharedRecord<K, V>>> {
        let record = RecordHandle(Arc::new(SharedRecord { key, value }));
        if found {
            return Some(
                std::mem::replace(&mut self.inner.slot_mut(i).as_mut().unwrap().key, record).0,
            );
        }
        if self.inner.len + 1 > (self.inner.mask + 1) * L / 100 {
            self.inner.grow();
            i = self.inner.find_by(hash, |r| r.0.key == record.0.key).0;
        }
        *self.inner.slot_mut(i) = Some(Entry {
            hash,
            key: record,
            value: (),
        });
        self.inner.len += 1;
        None
    }
    /// Remove a compatible borrowed key, returning its independently retained record.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<Arc<SharedRecord<K, V>>>
    where
        K: Borrow<Q>,
    {
        let (i, found) = self
            .inner
            .find_by(self.inner.hasher.hash_one(key), |r| r.0.key.borrow() == key);
        found.then(|| self.inner.remove_at(i).key.0)
    }
    /// Apply borrowed ordered mutations. Existing keys retain their stored identity;
    /// one canonical key/value copy is needed for each new immutable record.
    pub fn apply(&mut self, operations: &[Mutation<K, V>])
    where
        K: Clone,
        V: Clone,
    {
        for op in operations {
            match op {
                Mutation::Put(k, v) => {
                    let hash = self.inner.hasher.hash_one(k);
                    let (i, found) = self.inner.find_by(hash, |r| &r.0.key == k);
                    let key = if found {
                        self.inner.slot(i).as_ref().unwrap().key.0.key.clone()
                    } else {
                        k.clone()
                    };
                    self.put_at(i, found, hash, key, v.clone());
                }
                Mutation::Delete(k) => {
                    self.remove(k);
                }
            }
        }
    }
    /// Number of live bindings.
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    /// Whether there are no live bindings.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    /// Visit live bindings in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter().map(|(r, ())| (&r.0.key, &r.0.value))
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod layout_tests {
    use super::*;
    #[test]
    fn record_slot_reduces_shared_slot_width_on_64_bit_targets() {
        assert_eq!(
            std::mem::size_of::<Option<Entry<RecordHandle<u64, [u64; 4]>, ()>>>(),
            16
        );
        assert_eq!(
            std::mem::size_of::<Option<Entry<Arc<u64>, Arc<[u64; 4]>>>>(),
            24
        );
    }
}

/// Separate hash and key/value arrays, using backward-shift deletion. This layout
/// provides a second independent locality tradeoff. Cloning copies the arrays; callers
/// can wrap it in `Arc` for O(1) immutable snapshots between staged updates.
pub struct SplitMap<K, V, S = RandomState, const LOAD: usize = 70> {
    hashes: Vec<u64>,
    entries: Vec<Option<(K, V)>>,
    len: usize,
    hasher: Arc<S>,
}
impl<K: Clone, V: Clone, S, const L: usize> Clone for SplitMap<K, V, S, L> {
    fn clone(&self) -> Self {
        Self {
            hashes: self.hashes.clone(),
            entries: self.entries.clone(),
            len: self.len,
            hasher: self.hasher.clone(),
        }
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Default, const L: usize> Default
    for SplitMap<K, V, S, L>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher, const L: usize> SplitMap<K, V, S, L> {
    /// Construct an empty map with a selected hash builder. Invalid occupancy panics.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        Self {
            hashes: vec![0; 16],
            entries: vec![None; 16],
            len: 0,
            hasher: Arc::new(hasher),
        }
    }
    fn hash<Q: Hash + ?Sized>(&self, key: &Q) -> u64 {
        let hash = self.hasher.hash_one(key);
        if hash == 0 {
            u64::MAX
        } else {
            hash
        }
    }
    fn find<Q: Eq + ?Sized>(&self, key: &Q, hash: u64) -> (usize, bool)
    where
        K: Borrow<Q>,
    {
        let mask = self.hashes.len() - 1;
        let mut i = hash as usize & mask;
        loop {
            let h = self.hashes[i];
            if h == 0 {
                return (i, false);
            }
            if h == hash && self.entries[i].as_ref().unwrap().0.borrow() == key {
                return (i, true);
            }
            i = (i + 1) & mask;
        }
    }
    /// Look up a key in the immutable current contents.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let (i, found) = self.find(key, self.hash(key));
        if found {
            self.entries[i].as_ref().map(|(_, v)| v)
        } else {
            None
        }
    }
    /// Insert or replace a value, preserving an existing key allocation.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let hash = self.hash(&key);
        let (mut i, found) = self.find(&key, hash);
        if found {
            return Some(std::mem::replace(
                &mut self.entries[i].as_mut().unwrap().1,
                value,
            ));
        }
        if self.len + 1 > self.hashes.len() * L / 100 {
            self.grow();
            i = self.find(&key, hash).0;
        }
        self.hashes[i] = hash;
        self.entries[i] = Some((key, value));
        self.len += 1;
        None
    }
    fn grow(&mut self) {
        let capacity = self
            .hashes
            .len()
            .checked_mul(2)
            .expect("capacity exhausted");
        let mask = capacity - 1;
        let old_hashes = std::mem::replace(&mut self.hashes, vec![0; capacity]);
        let old_entries = std::mem::replace(&mut self.entries, vec![None; capacity]);
        for (hash, entry) in old_hashes.into_iter().zip(old_entries) {
            if let Some(entry) = entry {
                let mut i = hash as usize & mask;
                while self.hashes[i] != 0 {
                    i = (i + 1) & mask;
                }
                self.hashes[i] = hash;
                self.entries[i] = Some(entry);
            }
        }
    }
    /// Remove a binding and close its probe-chain hole.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let (mut hole, found) = self.find(key, self.hash(key));
        if !found {
            return None;
        }
        self.hashes[hole] = 0;
        let (key, value) = self.entries[hole].take().unwrap();
        self.len -= 1;
        let mask = self.hashes.len() - 1;
        let mut scan = (hole + 1) & mask;
        while self.hashes[scan] != 0 {
            let home = self.hashes[scan] as usize & mask;
            if scan.wrapping_sub(home) & mask >= hole.wrapping_sub(home) & mask {
                self.hashes[hole] = self.hashes[scan];
                self.hashes[scan] = 0;
                self.entries[hole] = self.entries[scan].take();
                hole = scan;
            }
            scan = (scan + 1) & mask;
        }
        drop(key);
        Some(value)
    }
    /// Live bindings.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no bindings are live.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Iterate all live bindings, in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries
            .iter()
            .filter_map(|e| e.as_ref())
            .map(|(k, v)| (k, v))
    }
}

#[path = "experimental/array_record.rs"]
mod array_record;
pub use array_record::{
    ArrayRecordMap, CopyKeys, HasherPolicy, KeyPolicy, OwnedHasher, SharedHasher, SharedKeys,
};
