//! Inline COW tables behind an embedded, fixed-size directory.
use super::{HasherPolicy, SharedHasher};
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
struct Page<K, V> {
    entries: Arc<[Option<Entry<K, V>>]>,
    len: usize,
}
// Keep the outer count in sync even when a clone or destructor unwinds after
// earlier mutations in this table. This is basic validity, not batch rollback.
struct LengthGuard<'a, K, V> {
    page: &'a mut Page<K, V>,
    total: &'a mut usize,
    before: usize,
}
impl<K, V> Drop for LengthGuard<'_, K, V> {
    fn drop(&mut self) {
        *self.total = *self.total - self.before + self.page.len;
    }
}
impl<K, V> Clone for Page<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            len: self.len,
        }
    }
}
fn fingerprint(hash: u64) -> NonZeroU64 {
    NonZeroU64::new(hash).unwrap_or(NonZeroU64::MAX)
}

impl<K: Clone + Eq, V: Clone> Page<K, V> {
    fn empty() -> Self {
        Self {
            entries: vec![None; 16].into(),
            len: 0,
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
        let mut i = hash.get() as usize & mask;
        loop {
            match &entries[i] {
                None => return (i, false),
                Some(e) if e.hash == hash && e.key.borrow() == key => return (i, true),
                _ => i = (i + 1) & mask,
            }
        }
    }
    fn get<Q: Eq + ?Sized>(&self, key: &Q, hash: NonZeroU64) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let mask = self.entries.len() - 1;
        let mut i = hash.get() as usize & mask;
        loop {
            match &self.entries[i] {
                None => return None,
                Some(e) if e.hash == hash && e.key.borrow() == key => return Some(&e.value),
                _ => i = (i + 1) & mask,
            }
        }
    }
    fn insert<const L: usize>(&mut self, key: K, value: V, hash: NonZeroU64) -> Option<V> {
        let (mut i, found) = Self::find(&self.entries, &key, hash);
        if found {
            return Some(std::mem::replace(
                &mut Arc::make_mut(&mut self.entries)[i].as_mut().unwrap().value,
                value,
            ));
        }
        if self.len + 1 > self.entries.len() * L / 100 {
            self.grow();
            i = Self::find(&self.entries, &key, hash).0;
        }
        Arc::make_mut(&mut self.entries)[i] = Some(Entry { hash, key, value });
        self.len += 1;
        None
    }
    fn put_ref<const L: usize>(&mut self, key: &K, value: &V, hash: NonZeroU64) {
        let (i, found) = Self::find(&self.entries, key, hash);
        if found {
            Arc::make_mut(&mut self.entries)[i].as_mut().unwrap().value = value.clone();
        } else {
            self.insert::<L>(key.clone(), value.clone(), hash);
        }
    }
    fn remove<Q: Eq + ?Sized>(&mut self, key: &Q, hash: NonZeroU64) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let (i, found) = Self::find(&self.entries, key, hash);
        if !found {
            return None;
        }
        let removed = Self::remove_at(Arc::make_mut(&mut self.entries), i);
        self.len -= 1;
        Some(removed.value)
    }
    fn remove_at(entries: &mut [Option<Entry<K, V>>], mut hole: usize) -> Entry<K, V> {
        let removed = entries[hole].take().unwrap();
        let mask = entries.len() - 1;
        let mut scan = (hole + 1) & mask;
        while let Some(e) = &entries[scan] {
            let home = e.hash.get() as usize & mask;
            if scan.wrapping_sub(home) & mask >= hole.wrapping_sub(home) & mask {
                entries[hole] = entries[scan].take();
                hole = scan;
            }
            scan = (scan + 1) & mask;
        }
        removed
    }
    fn place(entries: &mut [Option<Entry<K, V>>], e: Entry<K, V>) {
        let mask = entries.len() - 1;
        let mut i = e.hash.get() as usize & mask;
        while entries[i].is_some() {
            i = (i + 1) & mask;
        }
        entries[i] = Some(e);
    }
    fn grow(&mut self) {
        let capacity = self
            .entries
            .len()
            .checked_mul(2)
            .expect("capacity exhausted");
        let mut grown: Arc<[Option<Entry<K, V>>]> = vec![None; capacity].into();
        let next = Arc::get_mut(&mut grown).unwrap();
        if let Some(unique) = Arc::get_mut(&mut self.entries) {
            for slot in unique {
                if let Some(e) = slot.take() {
                    Self::place(next, e);
                }
            }
        } else {
            for e in self.entries.iter().flatten() {
                Self::place(next, e.clone());
            }
        }
        // A failed shared-entry clone leaves the original table installed.
        self.entries = grown;
    }
    fn apply_group<const L: usize>(
        &mut self,
        ops: &[Mutation<K, V>],
        hashes: &[NonZeroU64],
        order: &[usize],
    ) {
        let mut cursor = 0;
        while cursor < order.len() {
            let entries = Arc::make_mut(&mut self.entries);
            while cursor < order.len() {
                let index = order[cursor];
                let hash = hashes[index];
                let key = match &ops[index] {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let (slot, found) = Self::find(entries, key, hash);
                match &ops[index] {
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
                            let removed = Self::remove_at(entries, slot);
                            self.len -= 1;
                            drop(removed);
                        }
                    }
                }
                cursor += 1;
            }
            if cursor < order.len() {
                self.grow();
            }
        }
    }
}
type PolicyMarker<S, HP> = PhantomData<fn() -> (S, HP)>;

/// Inline entries in independently growing COW tables. The fixed directory is
/// embedded in the map, avoiding a separately allocated directory on lookups.
/// Cloning a map clones PARTITIONS Arc handles; writes copy touched tables only.
/// This can still clone substantial owned payloads when many tables are touched.
///
/// PARTITIONS must be a power of two in 1..=4096; LOAD is maximum fill in 20..=90.
/// GROUPED counts and stably scatters input indices by partition, then acquires a
/// writable table once between grows. Hashing, counts, scratch allocations and
/// scattering are included in apply's cost. Per-key mutation order is preserved;
/// cloning/drop side effects across different keys have no ordering guarantee.
/// Mutations are not transactions; callers supply atomic revision staging.
pub struct ShardedInlineMap<
    K,
    V,
    S = RandomState,
    const LOAD: usize = 70,
    const PARTITIONS: usize = 64,
    HP: HasherPolicy<S> = SharedHasher,
    const GROUPED: bool = false,
> {
    pages: [Page<K, V>; PARTITIONS],
    len: usize,
    hasher: HP::Stored,
    marker: PolicyMarker<S, HP>,
}
impl<K, V, S, const L: usize, const N: usize, HP: HasherPolicy<S>, const G: bool> Clone
    for ShardedInlineMap<K, V, S, L, N, HP, G>
{
    fn clone(&self) -> Self {
        Self {
            pages: self.pages.clone(),
            len: self.len,
            hasher: self.hasher.clone(),
            marker: PhantomData,
        }
    }
}
impl<
        K: Clone + Eq + Hash,
        V: Clone,
        S: BuildHasher + Default,
        const L: usize,
        const N: usize,
        HP: HasherPolicy<S>,
        const G: bool,
    > Default for ShardedInlineMap<K, V, S, L, N, HP, G>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<
        K: Clone + Eq + Hash,
        V: Clone,
        S: BuildHasher,
        const L: usize,
        const N: usize,
        HP: HasherPolicy<S>,
        const G: bool,
    > ShardedInlineMap<K, V, S, L, N, HP, G>
{
    /// Construct with a builder. Invalid const parameters panic before allocation.
    pub fn with_hasher(hasher: S) -> Self {
        assert!((20..=90).contains(&L));
        assert!(N > 0 && N.is_power_of_two() && N <= 4096);
        let empty = Page::empty();
        Self {
            pages: std::array::from_fn(|_| empty.clone()),
            len: 0,
            hasher: HP::store(hasher),
            marker: PhantomData,
        }
    }
    fn part(hash: NonZeroU64) -> usize {
        ((hash.get() >> 32) as usize) & (N - 1)
    }
    /// Read a compatible borrowed key without constructing or cloning it.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
        self.pages[Self::part(hash)].get(key, hash)
    }
    /// Insert or replace while preserving the canonical stored key. If cloning
    /// during growth fails, all existing bindings remain installed.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let hash = fingerprint(HP::original(&self.hasher).hash_one(&key));
        let page = &mut self.pages[Self::part(hash)];
        let before = page.len;
        let guard = LengthGuard {
            page,
            total: &mut self.len,
            before,
        };
        guard.page.insert::<L>(key, value, hash)
    }
    /// Remove a compatible borrowed key and close its table's probe chain.
    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
        let page = &mut self.pages[Self::part(hash)];
        let before = page.len;
        let guard = LengthGuard {
            page,
            total: &mut self.len,
            before,
        };
        guard.page.remove(key, hash)
    }
    /// Apply mutations, optionally grouping by table while preserving each key's
    /// order. Returned views must be published by a separate atomic staging layer.
    /// A panic may leave a partial batch applied, with counts still consistent.
    pub fn apply(&mut self, ops: &[Mutation<K, V>]) {
        if ops.is_empty() {
            return;
        }
        if !G {
            for op in ops {
                let key = match op {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
                let page = &mut self.pages[Self::part(hash)];
                let before = page.len;
                let guard = LengthGuard {
                    page,
                    total: &mut self.len,
                    before,
                };
                match op {
                    Mutation::Put(k, v) => guard.page.put_ref::<L>(k, v, hash),
                    Mutation::Delete(k) => {
                        guard.page.remove(k, hash);
                    }
                }
            }
            return;
        }
        let mut counts = [0usize; N];
        let hashes: Vec<_> = ops
            .iter()
            .map(|op| {
                let key = match op {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let hash = fingerprint(HP::original(&self.hasher).hash_one(key));
                counts[Self::part(hash)] += 1;
                hash
            })
            .collect();
        let mut total = 0;
        for count in &mut counts {
            let n = *count;
            *count = total;
            total += n;
        }
        let mut next = counts;
        let mut order = vec![0usize; ops.len()];
        for (i, hash) in hashes.iter().enumerate() {
            let p = Self::part(*hash);
            order[next[p]] = i;
            next[p] += 1;
        }
        for (p, page) in self.pages.iter_mut().enumerate() {
            if counts[p] != next[p] {
                let before = page.len;
                let guard = LengthGuard {
                    page,
                    total: &mut self.len,
                    before,
                };
                guard
                    .page
                    .apply_group::<L>(ops, &hashes, &order[counts[p]..next[p]]);
            }
        }
    }
    /// Number of live bindings across all tables.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no bindings are live.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Visit every binding in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.pages
            .iter()
            .flat_map(|p| p.entries.iter().flatten())
            .map(|e| (&e.key, &e.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_is_embedded_and_numeric_slots_keep_the_hash_niche() {
        type One = ShardedInlineMap<u64, [u64; 4], RandomState, 70, 1>;
        type Sixteen = ShardedInlineMap<u64, [u64; 4], RandomState, 70, 16>;
        assert_eq!(
            std::mem::size_of::<Sixteen>() - std::mem::size_of::<One>(),
            15 * std::mem::size_of::<Page<u64, [u64; 4]>>()
        );
        assert_eq!(
            std::mem::size_of::<Option<Entry<u64, [u64; 4]>>>(),
            std::mem::size_of::<Entry<u64, [u64; 4]>>()
        );
    }
}
