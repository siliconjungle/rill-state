//! Fixed-size COW pages for compact immutable records. Safe Rust throughout.
use super::SharedRecord;
use crate::Mutation;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    sync::Arc,
};

/// Selects how a canonical key is stored inside each immutable record. Implementors
/// must preserve the key's Hash/Eq behavior and return that same logical key.
pub trait KeyPolicy<K: Clone> {
    /// Canonical key representation, copied when a record is replaced.
    type Stored: Clone;
    /// Store a newly inserted key.
    fn store(key: K) -> Self::Stored;
    /// Borrow its original logical key.
    fn original(key: &Self::Stored) -> &K;
}
/// Clone the canonical key into each replacement record.
#[derive(Clone, Copy, Default, Debug)]
pub struct CopyKeys;
impl<K: Clone> KeyPolicy<K> for CopyKeys {
    type Stored = K;
    fn store(key: K) -> K {
        key
    }
    fn original(key: &K) -> &K {
        key
    }
}
/// Share the canonical key allocation between replacement records.
#[derive(Clone, Copy, Default, Debug)]
pub struct SharedKeys;
impl<K: Clone> KeyPolicy<K> for SharedKeys {
    type Stored = Arc<K>;
    fn store(key: K) -> Arc<K> {
        Arc::new(key)
    }
    fn original(key: &Arc<K>) -> &K {
        key.as_ref()
    }
}
struct Slot<K, V> {
    hash: u64,
    record: Option<Arc<SharedRecord<K, V>>>,
}
impl<K, V> Clone for Slot<K, V> {
    fn clone(&self) -> Self {
        Self {
            hash: self.hash,
            record: self.record.clone(),
        }
    }
}
impl<K, V> Slot<K, V> {
    fn empty() -> Self {
        Self {
            hash: 0,
            record: None,
        }
    }
}
type Page<K, V, const P: usize> = Arc<[Slot<K, V>; P]>;

/// Linear probing over fixed-size Arc arrays. Pages have no Vec header or separate
/// payload allocation. Each slot has a full cached hash and one immutable record.
/// `PAGE` must be a power of two in 1..=4096; `LOAD` is a maximum fill percentage
/// in 20..=90. Cloning shares pages and writes detach touched pages. `CopyKeys`
/// and `SharedKeys` expose the key-copy/indirection tradeoff explicitly. This
/// experimental mutable map is not an atomic transaction or the selected Store.
pub struct ArrayRecordMap<
    K: Clone,
    V,
    S = RandomState,
    const LOAD: usize = 70,
    const PAGE: usize = 16,
    KP: KeyPolicy<K> = CopyKeys,
> {
    pages: Vec<Page<KP::Stored, V, PAGE>>,
    len: usize,
    mask: usize,
    hasher: Arc<S>,
    marker: PhantomData<fn() -> (K, KP)>,
}
impl<K: Clone, V, S, const L: usize, const P: usize, KP: KeyPolicy<K>> Clone
    for ArrayRecordMap<K, V, S, L, P, KP>
{
    fn clone(&self) -> Self {
        Self {
            pages: self.pages.clone(),
            len: self.len,
            mask: self.mask,
            hasher: self.hasher.clone(),
            marker: PhantomData,
        }
    }
}
impl<
        K: Clone + Hash + Eq,
        V,
        S: BuildHasher + Default,
        const L: usize,
        const P: usize,
        KP: KeyPolicy<K>,
    > Default for ArrayRecordMap<K, V, S, L, P, KP>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<K: Clone + Hash + Eq, V, S: BuildHasher, const L: usize, const P: usize, KP: KeyPolicy<K>>
    ArrayRecordMap<K, V, S, L, P, KP>
{
    /// Construct an empty map; invalid const parameters panic before allocation.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        assert!(P > 0 && P.is_power_of_two() && P <= 4096);
        let capacity = P.max(16);
        let empty = Arc::new(std::array::from_fn(|_| Slot::empty()));
        Self {
            pages: vec![empty; capacity / P],
            len: 0,
            mask: capacity - 1,
            hasher: Arc::new(hasher),
            marker: PhantomData,
        }
    }
    fn slot(&self, i: usize) -> &Slot<KP::Stored, V> {
        &self.pages[i / P][i % P]
    }
    fn slot_mut(&mut self, i: usize) -> &mut Slot<KP::Stored, V> {
        &mut Arc::make_mut(&mut self.pages[i / P])[i % P]
    }
    fn find<Q: Eq + ?Sized>(&self, key: &Q, hash: u64) -> (usize, bool)
    where
        K: Borrow<Q>,
    {
        let mut i = hash as usize & self.mask;
        loop {
            let slot = self.slot(i);
            match &slot.record {
                None => return (i, false),
                Some(record) if slot.hash == hash && KP::original(&record.key).borrow() == key => {
                    return (i, true)
                }
                _ => i = (i + 1) & self.mask,
            }
        }
    }
    /// Lookup a compatible borrowed key without allocation or refcount changes.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let hash = self.hasher.hash_one(key);
        let mut i = hash as usize & self.mask;
        loop {
            let page = &self.pages[i / P];
            for slot in &page[i % P..] {
                match &slot.record {
                    None => return None,
                    Some(record)
                        if slot.hash == hash && KP::original(&record.key).borrow() == key =>
                    {
                        return Some(&record.value)
                    }
                    _ => {}
                }
            }
            i = (((i / P) + 1) * P) & self.mask;
        }
    }
    /// Insert or replace, preserving the existing canonical key representation.
    pub fn insert(&mut self, key: K, value: V) -> Option<Arc<SharedRecord<KP::Stored, V>>> {
        let hash = self.hasher.hash_one(&key);
        let (i, found) = self.find(&key, hash);
        let key = if found {
            self.slot(i).record.as_ref().unwrap().key.clone()
        } else {
            KP::store(key)
        };
        self.write_at(i, found, hash, key, value)
    }
    fn write_at(
        &mut self,
        mut i: usize,
        found: bool,
        hash: u64,
        key: KP::Stored,
        value: V,
    ) -> Option<Arc<SharedRecord<KP::Stored, V>>> {
        let record = Arc::new(SharedRecord { key, value });
        if found {
            return self.slot_mut(i).record.replace(record);
        }
        if self.len + 1 > (self.mask + 1) * L / 100 {
            self.grow();
            i = self.find(KP::original(&record.key), hash).0;
        }
        *self.slot_mut(i) = Slot {
            hash,
            record: Some(record),
        };
        self.len += 1;
        None
    }
    fn grow(&mut self) {
        let capacity = (self.mask + 1).checked_mul(2).expect("capacity exhausted");
        let empty = Arc::new(std::array::from_fn(|_| Slot::empty()));
        let old = std::mem::replace(&mut self.pages, vec![empty; capacity / P]);
        self.mask = capacity - 1;
        for page in old {
            let slots = Arc::try_unwrap(page).unwrap_or_else(|page| (*page).clone());
            for slot in slots {
                if slot.record.is_some() {
                    let mut i = slot.hash as usize & self.mask;
                    while self.slot(i).record.is_some() {
                        i = (i + 1) & self.mask;
                    }
                    *self.slot_mut(i) = slot;
                }
            }
        }
    }
    /// Remove a compatible borrowed key; old snapshots/record handles remain valid.
    pub fn remove<Q: Hash + Eq + ?Sized>(
        &mut self,
        key: &Q,
    ) -> Option<Arc<SharedRecord<KP::Stored, V>>>
    where
        K: Borrow<Q>,
    {
        let (mut hole, found) = self.find(key, self.hasher.hash_one(key));
        if !found {
            return None;
        }
        let removed = self.slot_mut(hole).record.take().unwrap();
        self.len -= 1;
        let mut scan = (hole + 1) & self.mask;
        while self.slot(scan).record.is_some() {
            let home = self.slot(scan).hash as usize & self.mask;
            if scan.wrapping_sub(home) & self.mask >= hole.wrapping_sub(home) & self.mask {
                let moved = std::mem::replace(self.slot_mut(scan), Slot::empty());
                *self.slot_mut(hole) = moved;
                hole = scan;
            }
            scan = (scan + 1) & self.mask;
        }
        Some(removed)
    }
    /// Apply ordered borrowed mutations. The caller supplies atomic staging if needed.
    pub fn apply(&mut self, operations: &[Mutation<K, V>])
    where
        V: Clone,
    {
        for operation in operations {
            match operation {
                Mutation::Put(key, value) => {
                    let hash = self.hasher.hash_one(key);
                    let (i, found) = self.find(key, hash);
                    let stored = if found {
                        self.slot(i).record.as_ref().unwrap().key.clone()
                    } else {
                        KP::store(key.clone())
                    };
                    self.write_at(i, found, hash, stored, value.clone());
                }
                Mutation::Delete(key) => {
                    self.remove(key);
                }
            }
        }
    }
    /// Number of live bindings.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether there are no live bindings.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Visit live bindings in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.pages
            .iter()
            .flat_map(|page| page.iter())
            .filter_map(|slot| slot.record.as_ref())
            .map(|record| (KP::original(&record.key), &record.value))
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;
    #[test]
    fn array_page_has_no_per_page_vec_header() {
        assert_eq!(
            std::mem::size_of::<Slot<String, Vec<u8>>>(),
            2 * std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<[Slot<String, Vec<u8>>; 16]>(),
            16 * std::mem::size_of::<Slot<String, Vec<u8>>>()
        );
    }
}
