//! Contiguous inline entries with a nonzero-hash niche and one COW slice.
use super::HasherPolicy;
use crate::Mutation;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    num::NonZeroU64,
    sync::Arc,
};

#[derive(Clone, Copy)]
struct Entry<K, V> {
    hash: NonZeroU64,
    key: K,
    value: V,
}

// Zero and MAX may share a cached hash and home slot. Equality is always checked;
// this sacrifices one fingerprint value, never key identity or reachability.
fn fingerprint(hash: u64) -> NonZeroU64 {
    NonZeroU64::new(hash).unwrap_or(NonZeroU64::MAX)
}

type BuilderMarker<S, HP> = PhantomData<fn() -> (S, HP)>;

/// A single immutable slice of inline entries; cloning shares the slice, and
/// writes copy it when retained. `LOAD` is a maximum fill percentage in 20..=90.
/// The hash niche can eliminate Option's separate discriminant; unlike boxed
/// records, updating a binding needs no per-record allocation. Copying a shared
/// slice nevertheless clones every live K/V, including owned heap allocations.
///
/// `SPAN` selects contiguous-span versus indexed-wrap probing for reads. Both
/// use identical hashes and full key equality. `HP` selects shared/inline builder
/// storage. This research map is not an atomic transaction or the selected Store.
pub struct InlineSliceMap<
    K,
    V,
    S = RandomState,
    const LOAD: usize = 70,
    HP: HasherPolicy<S> = super::SharedHasher,
    const SPAN: bool = true,
> {
    entries: Arc<[Option<Entry<K, V>>]>,
    len: usize,
    hasher: HP::Stored,
    marker: BuilderMarker<S, HP>,
}

impl<K, V, S, const L: usize, HP: HasherPolicy<S>, const SPAN: bool> Clone
    for InlineSliceMap<K, V, S, L, HP, SPAN>
{
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            len: self.len,
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
        const SPAN: bool,
    > Default for InlineSliceMap<K, V, S, L, HP, SPAN>
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
        const SPAN: bool,
    > InlineSliceMap<K, V, S, L, HP, SPAN>
{
    /// Construct an empty table. Invalid fill thresholds panic before allocation.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        Self {
            entries: vec![None; 16].into(),
            len: 0,
            hasher: HP::store(hasher),
            marker: PhantomData,
        }
    }

    fn find<Q: Eq + ?Sized>(
        entries: &[Option<Entry<K, V>>],
        key: &Q,
        hash: NonZeroU64,
    ) -> (usize, bool)
    where
        K: Borrow<Q>,
    {
        let mask = entries.len() - 1;
        let mut slot = hash.get() as usize & mask;
        loop {
            match &entries[slot] {
                None => return (slot, false),
                Some(e) if e.hash == hash && e.key.borrow() == key => return (slot, true),
                _ => slot = (slot + 1) & mask,
            }
        }
    }

    /// Read a compatible borrowed key, such as str for a stored String.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
        if !SPAN {
            let mask = self.entries.len() - 1;
            let mut slot = hash.get() as usize & mask;
            loop {
                match &self.entries[slot] {
                    None => return None,
                    Some(e) if e.hash == hash && e.key.borrow() == key => return Some(&e.value),
                    _ => slot = (slot + 1) & mask,
                }
            }
        }
        let start = hash.get() as usize & (self.entries.len() - 1);
        for slot in self.entries[start..]
            .iter()
            .chain(self.entries[..start].iter())
        {
            match slot {
                None => return None,
                Some(e) if e.hash == hash && e.key.borrow() == key => return Some(&e.value),
                _ => {}
            }
        }
        None
    }

    /// Insert or replace a value, preserving the existing canonical key.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let hash = fingerprint(HP::original(&self.hasher).hash_one(&key));
        let (mut slot, found) = Self::find(&self.entries, &key, hash);
        if found {
            let entry = Arc::make_mut(&mut self.entries)[slot].as_mut().unwrap();
            return Some(std::mem::replace(&mut entry.value, value));
        }
        if self.len + 1 > self.entries.len() * L / 100 {
            self.grow();
            slot = Self::find(&self.entries, &key, hash).0;
        }
        Arc::make_mut(&mut self.entries)[slot] = Some(Entry { hash, key, value });
        self.len += 1;
        None
    }

    /// Apply ordered mutations, acquiring the writable slice once between grows.
    /// Publication atomicity must be supplied by the caller's staging wrapper.
    pub fn apply(&mut self, operations: &[Mutation<K, V>]) {
        let mut next = 0;
        while next < operations.len() {
            let entries = Arc::make_mut(&mut self.entries);
            while next < operations.len() {
                let key = match &operations[next] {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
                let (slot, found) = Self::find(entries, key, hash);
                match &operations[next] {
                    Mutation::Put(k, v) => {
                        if found {
                            entries[slot].as_mut().unwrap().value = v.clone();
                        } else {
                            if self.len + 1 > entries.len() * L / 100 {
                                break;
                            }
                            entries[slot] = Some(Entry {
                                hash,
                                key: k.clone(),
                                value: v.clone(),
                            });
                            self.len += 1;
                        }
                    }
                    Mutation::Delete(_) => {
                        if found {
                            Self::remove_at(entries, slot);
                            self.len -= 1;
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
        let capacity = self
            .entries
            .len()
            .checked_mul(2)
            .expect("capacity exhausted");
        let mut old = std::mem::replace(&mut self.entries, vec![None; capacity].into());
        let entries = Arc::get_mut(&mut self.entries).unwrap();
        if let Some(unique) = Arc::get_mut(&mut old) {
            for slot in unique {
                if let Some(entry) = slot.take() {
                    Self::place(entries, entry);
                }
            }
        } else {
            for entry in old.iter().flatten() {
                Self::place(entries, entry.clone());
            }
        }
    }

    fn place(entries: &mut [Option<Entry<K, V>>], entry: Entry<K, V>) {
        let mask = entries.len() - 1;
        let mut slot = entry.hash.get() as usize & mask;
        while entries[slot].is_some() {
            slot = (slot + 1) & mask;
        }
        entries[slot] = Some(entry);
    }

    /// Remove a borrowed key; shifting closes the probe chain without tombstones.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
        let (slot, found) = Self::find(&self.entries, key, hash);
        if !found {
            return None;
        }
        let removed = Self::remove_at(Arc::make_mut(&mut self.entries), slot);
        self.len -= 1;
        Some(removed.value)
    }

    fn remove_at(entries: &mut [Option<Entry<K, V>>], mut hole: usize) -> Entry<K, V> {
        let removed = entries[hole].take().unwrap();
        let mask = entries.len() - 1;
        let mut scan = (hole + 1) & mask;
        while let Some(entry) = &entries[scan] {
            let home = entry.hash.get() as usize & mask;
            if scan.wrapping_sub(home) & mask >= hole.wrapping_sub(home) & mask {
                entries[hole] = entries[scan].take();
                hole = scan;
            }
            scan = (scan + 1) & mask;
        }
        removed
    }

    /// Number of live bindings.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no keys are live.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Visit every binding in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries
            .iter()
            .flatten()
            .map(|entry| (&entry.key, &entry.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_slot_uses_hash_niche_instead_of_extra_discriminant() {
        type Slot = Option<Entry<u64, [u64; 4]>>;
        assert_eq!(
            std::mem::size_of::<Slot>(),
            std::mem::size_of::<Entry<u64, [u64; 4]>>()
        );
        #[cfg(target_pointer_width = "64")]
        {
            type Tagged = Option<super::super::Entry<u64, [u64; 4]>>;
            assert_eq!(std::mem::size_of::<Slot>(), 48);
            assert_eq!(std::mem::size_of::<Tagged>(), 56);
        }
        assert_eq!(fingerprint(0), fingerprint(u64::MAX));
        assert_ne!(fingerprint(0), fingerprint(1));
    }
}
