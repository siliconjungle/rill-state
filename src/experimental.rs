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

#[derive(Clone)]
struct Entry<K, V> {
    hash: u64,
    key: K,
    value: V,
}
type Page<K, V> = Arc<Vec<Option<Entry<K, V>>>>;

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
        let mut i = hash as usize & self.mask;
        loop {
            match self.slot(i) {
                None => return (i, false),
                Some(e) if e.hash == hash && e.key.borrow() == key => return (i, true),
                _ => i = (i + 1) & self.mask,
            }
        }
    }
    /// Read a key, including a compatible borrowed key such as `str` for `String`.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let hash = self.hasher.hash_one(key);
        let (i, found) = self.find(key, hash);
        if found {
            self.slot(i).as_ref().map(|e| &e.value)
        } else {
            None
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
                        if page[i].take().is_some() {
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
        let old = std::mem::replace(&mut self.pages, vec![empty; capacity / width]);
        self.mask = capacity - 1;
        for page in old {
            let entries = Arc::try_unwrap(page).unwrap_or_else(|p| (*p).clone());
            for e in entries.into_iter().flatten() {
                let mut i = e.hash as usize & self.mask;
                while self.slot(i).is_some() {
                    i = (i + 1) & self.mask;
                }
                *self.slot_mut(i) = Some(e);
            }
        }
    }
    /// Remove a key. Backward shifting preserves lookup reachability without tombstones.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let hash = self.hasher.hash_one(key);
        let (mut hole, found) = self.find(key, hash);
        if !found {
            return None;
        }
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
        Some(removed.value)
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
        let (_, value) = self.entries[hole].take().unwrap();
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
