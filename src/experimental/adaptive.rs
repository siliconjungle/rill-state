//! Data-driven flat/paged COW research. Raw batches are not transactions.
use super::{RecordHandle, RecordMap, SharedRecord};
use crate::Mutation;
use std::{
    borrow::Borrow,
    collections::{hash_map::RandomState, HashMap},
    hash::{BuildHasher, Hash},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

type RecordList<K, V> = Box<[Arc<SharedRecord<K, V>>]>;
struct ScanCache<K, V> {
    records: OnceLock<RecordList<K, V>>,
    visits: AtomicUsize,
}
impl<K, V> ScanCache<K, V> {
    fn new() -> Self {
        Self {
            records: OnceLock::new(),
            visits: AtomicUsize::new(0),
        }
    }
    fn len(&self) -> usize {
        self.records.get().map_or(0, |r| r.len())
    }
    fn entries(
        &self,
        after: usize,
        make: impl FnOnce() -> RecordList<K, V>,
    ) -> Option<&[Arc<SharedRecord<K, V>>]> {
        if after == 0 {
            return None;
        }
        if let Some(rows) = self.records.get() {
            return Some(rows);
        }
        if self.visits.fetch_add(1, Ordering::Relaxed) < after - 1 {
            return None;
        }
        Some(self.records.get_or_init(make))
    }
}

/// Unstable flat representation policy used by `AdaptiveMap` research.
/// Implementations must preserve canonical keys, borrowed values and immutable
/// independently retained roots. The built-in cache clones only Arc handles.
pub trait FlatMode<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Clone> {
    /// Owned flat representation; cloning prepares an independent writable root.
    type Data: Clone;
    /// Complete traversal requests before caching shared records; zero disables it.
    const SCAN_AFTER: usize;
    /// Whether layout transitions can transfer immutable shared records.
    const SHARED_TRANSITIONS: bool = false;
    /// Optionally permute derived scan handles without replacing or duplicating
    /// any record. Lookup and primary storage are unaffected; construction cost
    /// belongs to the scan that fills the cache.
    fn order_scan_records(_records: &mut [Arc<SharedRecord<K, V>>]) {}
    /// Construct empty flat storage with the supplied builder.
    fn new(hasher: S) -> Self::Data;
    /// Build flat storage from owned canonical rows without cloning their values.
    fn from_entries(hasher: S, entries: impl Iterator<Item = (K, V)>) -> Self::Data;
    /// Build from unique canonical records. Default policies clone their K/V.
    fn from_shared_records(
        hasher: S,
        records: impl Iterator<Item = Arc<SharedRecord<K, V>>>,
    ) -> Self::Data {
        Self::from_entries(hasher, records.map(|r| (r.key.clone(), r.value.clone())))
    }
    /// Visit all unique primary shared records when SHARED_TRANSITIONS is true.
    /// Policies enabling that flag must override this method.
    fn visit_shared_records(_data: &Self::Data, _f: impl FnMut(&Arc<SharedRecord<K, V>>)) {
        panic!("flat policy does not expose shared records")
    }
    /// Borrow the builder for a staged flat-to-paged conversion.
    fn hasher(data: &Self::Data) -> &S;
    /// Number of live bindings.
    fn len(data: &Self::Data) -> usize;
    /// Borrow a primary stored value without cloning it.
    fn get<'a, Q: Hash + Eq + ?Sized>(data: &'a Self::Data, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>;
    /// Apply ordered raw operations, retaining canonical key metadata.
    fn apply(data: &mut Self::Data, ops: &[Mutation<K, V>]);
    /// Traverse primary storage without triggering a derived cache.
    fn visit_primary<'a>(data: &'a Self::Data, f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a;
    /// Traverse all primary instances, optionally through dense shared handles.
    fn visit<'a>(data: &'a Self::Data, f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a;
    /// Number of shared handles in the derived traversal cache.
    fn cache_len(data: &Self::Data) -> usize;
}
/// Direct flat values; no traversal cache and no per-record allocation.
pub struct InlineFlat;
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Clone> FlatMode<K, V, S> for InlineFlat {
    type Data = HashMap<K, V, S>;
    const SCAN_AFTER: usize = 0;
    fn new(hasher: S) -> Self::Data {
        HashMap::with_hasher(hasher)
    }
    fn from_entries(hasher: S, entries: impl Iterator<Item = (K, V)>) -> Self::Data {
        let mut data = HashMap::with_hasher(hasher);
        data.extend(entries);
        data
    }
    fn hasher(data: &Self::Data) -> &S {
        data.hasher()
    }
    fn len(data: &Self::Data) -> usize {
        data.len()
    }
    #[inline]
    fn get<'a, Q: Hash + Eq + ?Sized>(data: &'a Self::Data, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
    {
        data.get(key)
    }
    fn apply(data: &mut Self::Data, ops: &[Mutation<K, V>]) {
        for op in ops {
            match op {
                Mutation::Put(k, v) => {
                    if let Some(old) = data.get_mut(k) {
                        *old = v.clone();
                    } else {
                        data.insert(k.clone(), v.clone());
                    }
                }
                Mutation::Delete(k) => {
                    data.remove(k);
                }
            }
        }
    }
    fn visit_primary<'a>(data: &'a Self::Data, mut f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        for (k, v) in data {
            f(k, v);
        }
    }
    fn visit<'a>(data: &'a Self::Data, f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        Self::visit_primary(data, f);
    }
    fn cache_len(_: &Self::Data) -> usize {
        0
    }
}
/// Shared immutable flat records. AFTER=0 is the matching cache-disabled
/// control; AFTER=2 materializes dense handles on the second traversal request.
/// Lookup keys are duplicated. This changes allocation/indirection costs and
/// must not be presented as the same representation as `InlineFlat`.
pub struct SharedFlat<const AFTER: usize>;
/// Shared-flat control with Arc-preserving layout transitions. Unchanged payloads
/// are not cloned during conversion; allocating lookup keys can still be cloned.
/// This opt-in research policy is not the selected Store backend.
pub struct ReuseSharedFlat<const AFTER: usize>;
/// After-two-scans cache ordered by immutable record address shifted by SHIFT.
/// REUSE selects Arc-preserving transitions; false retains copying controls.
/// SHIFT=0 orders exact addresses; SHIFT=12 groups 4096-byte address buckets.
/// This is a traversal-locality experiment, not a hardware layout guarantee.
/// Key equality, lookup, canonical values and unspecified iteration order remain
/// unchanged. Sorting is charged to cache construction and clones no K or V.
pub struct AddressOrderedFlat<const REUSE: bool, const SHIFT: u32>;
/// Implementation data for the unstable shared-record flat policy.
pub struct SharedFlatData<K, V, S> {
    map: HashMap<K, Arc<SharedRecord<K, V>>, S>,
    cache: ScanCache<K, V>,
}
impl<K: Clone, V, S: Clone> Clone for SharedFlatData<K, V, S> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            cache: ScanCache::new(),
        }
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Clone, const A: usize> FlatMode<K, V, S>
    for SharedFlat<A>
{
    type Data = SharedFlatData<K, V, S>;
    const SCAN_AFTER: usize = A;
    fn new(hasher: S) -> Self::Data {
        assert!(A == 0 || A == 2);
        SharedFlatData {
            map: HashMap::with_hasher(hasher),
            cache: ScanCache::new(),
        }
    }
    fn from_entries(hasher: S, entries: impl Iterator<Item = (K, V)>) -> Self::Data {
        use std::collections::hash_map::Entry;
        let mut data = Self::new(hasher);
        data.map.reserve(entries.size_hint().0);
        for (key, value) in entries {
            match data.map.entry(key) {
                Entry::Vacant(slot) => {
                    let key = slot.key().clone();
                    slot.insert(Arc::new(SharedRecord { key, value }));
                }
                Entry::Occupied(mut slot) => {
                    let key = slot.get().key().clone();
                    slot.insert(Arc::new(SharedRecord { key, value }));
                }
            }
        }
        data
    }
    fn hasher(data: &Self::Data) -> &S {
        data.map.hasher()
    }
    fn len(data: &Self::Data) -> usize {
        data.map.len()
    }
    #[inline]
    fn get<'a, Q: Hash + Eq + ?Sized>(data: &'a Self::Data, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
    {
        data.map.get(key).map(|r| r.value())
    }
    fn apply(data: &mut Self::Data, ops: &[Mutation<K, V>]) {
        data.cache = ScanCache::new();
        for op in ops {
            match op {
                Mutation::Put(k, v) => {
                    if let Some(old) = data.map.get_mut(k) {
                        let next = Arc::new(SharedRecord {
                            key: old.key().clone(),
                            value: v.clone(),
                        });
                        *old = next;
                    } else {
                        let key = k.clone();
                        let record = Arc::new(SharedRecord {
                            key: key.clone(),
                            value: v.clone(),
                        });
                        data.map.insert(key, record);
                    }
                }
                Mutation::Delete(k) => {
                    data.map.remove(k);
                }
            }
        }
    }
    fn visit_primary<'a>(data: &'a Self::Data, mut f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        for r in data.map.values() {
            f(r.key(), r.value());
        }
    }
    fn visit<'a>(data: &'a Self::Data, mut f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        if let Some(rows) = data
            .cache
            .entries(A, || data.map.values().cloned().collect())
        {
            for r in rows {
                f(r.key(), r.value());
            }
        } else {
            Self::visit_primary(data, f);
        }
    }
    fn cache_len(data: &Self::Data) -> usize {
        data.cache.len()
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Clone, const A: usize> FlatMode<K, V, S>
    for ReuseSharedFlat<A>
{
    type Data = SharedFlatData<K, V, S>;
    const SCAN_AFTER: usize = A;
    const SHARED_TRANSITIONS: bool = true;
    fn new(hasher: S) -> Self::Data {
        SharedFlat::<A>::new(hasher)
    }
    fn from_entries(hasher: S, entries: impl Iterator<Item = (K, V)>) -> Self::Data {
        SharedFlat::<A>::from_entries(hasher, entries)
    }
    fn from_shared_records(
        hasher: S,
        records: impl Iterator<Item = Arc<SharedRecord<K, V>>>,
    ) -> Self::Data {
        use std::collections::hash_map::Entry;
        let mut data = Self::new(hasher);
        data.map.reserve(records.size_hint().0);
        for record in records {
            match data.map.entry(record.key.clone()) {
                Entry::Vacant(slot) => {
                    slot.insert(record);
                }
                Entry::Occupied(_) => panic!("shared transition requires unique canonical keys"),
            }
        }
        data
    }
    fn visit_shared_records(data: &Self::Data, mut f: impl FnMut(&Arc<SharedRecord<K, V>>)) {
        for record in data.map.values() {
            f(record);
        }
    }
    fn hasher(data: &Self::Data) -> &S {
        SharedFlat::<A>::hasher(data)
    }
    fn len(data: &Self::Data) -> usize {
        SharedFlat::<A>::len(data)
    }
    fn get<'a, Q: Hash + Eq + ?Sized>(data: &'a Self::Data, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
    {
        SharedFlat::<A>::get(data, key)
    }
    fn apply(data: &mut Self::Data, ops: &[Mutation<K, V>]) {
        SharedFlat::<A>::apply(data, ops);
    }
    fn visit_primary<'a>(data: &'a Self::Data, f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        SharedFlat::<A>::visit_primary(data, f);
    }
    fn visit<'a>(data: &'a Self::Data, f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        SharedFlat::<A>::visit(data, f);
    }
    fn cache_len(data: &Self::Data) -> usize {
        SharedFlat::<A>::cache_len(data)
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher + Clone, const R: bool, const SHIFT: u32>
    FlatMode<K, V, S> for AddressOrderedFlat<R, SHIFT>
{
    type Data = SharedFlatData<K, V, S>;
    const SCAN_AFTER: usize = 2;
    const SHARED_TRANSITIONS: bool = R;
    fn order_scan_records(records: &mut [Arc<SharedRecord<K, V>>]) {
        assert!(
            SHIFT < usize::BITS,
            "record address shift exceeds pointer width"
        );
        records.sort_unstable_by_key(|record| (Arc::as_ptr(record) as usize) >> SHIFT);
    }
    fn new(hasher: S) -> Self::Data {
        assert!(
            SHIFT < usize::BITS,
            "record address shift exceeds pointer width"
        );
        SharedFlat::<2>::new(hasher)
    }
    fn from_entries(hasher: S, entries: impl Iterator<Item = (K, V)>) -> Self::Data {
        assert!(
            SHIFT < usize::BITS,
            "record address shift exceeds pointer width"
        );
        SharedFlat::<2>::from_entries(hasher, entries)
    }
    fn from_shared_records(
        hasher: S,
        records: impl Iterator<Item = Arc<SharedRecord<K, V>>>,
    ) -> Self::Data {
        assert!(
            SHIFT < usize::BITS,
            "record address shift exceeds pointer width"
        );
        if R {
            ReuseSharedFlat::<2>::from_shared_records(hasher, records)
        } else {
            SharedFlat::<2>::from_shared_records(hasher, records)
        }
    }
    fn visit_shared_records(data: &Self::Data, mut f: impl FnMut(&Arc<SharedRecord<K, V>>)) {
        for record in data.map.values() {
            f(record);
        }
    }
    fn hasher(data: &Self::Data) -> &S {
        SharedFlat::<2>::hasher(data)
    }
    fn len(data: &Self::Data) -> usize {
        SharedFlat::<2>::len(data)
    }
    fn get<'a, Q: Hash + Eq + ?Sized>(data: &'a Self::Data, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
    {
        SharedFlat::<2>::get(data, key)
    }
    fn apply(data: &mut Self::Data, ops: &[Mutation<K, V>]) {
        SharedFlat::<2>::apply(data, ops);
    }
    fn visit_primary<'a>(data: &'a Self::Data, f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        SharedFlat::<2>::visit_primary(data, f);
    }
    fn visit<'a>(data: &'a Self::Data, mut f: impl FnMut(&'a K, &'a V))
    where
        K: 'a,
        V: 'a,
    {
        if let Some(rows) = data.cache.entries(2, || {
            let mut rows: RecordList<K, V> = data.map.values().cloned().collect();
            <Self as FlatMode<K, V, S>>::order_scan_records(&mut rows);
            rows
        }) {
            for record in rows {
                f(record.key(), record.value());
            }
        } else {
            Self::visit_primary(data, f);
        }
    }
    fn cache_len(data: &Self::Data) -> usize {
        SharedFlat::<2>::cache_len(data)
    }
}
struct PagedData<K, V, S, const P: usize> {
    map: RecordMap<K, V, S, 70, P>,
    cache: ScanCache<K, V>,
}
impl<K, V, S, const P: usize> Clone for PagedData<K, V, S, P> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            cache: ScanCache::new(),
        }
    }
}
enum Root<
    K: Clone + Hash + Eq,
    V: Clone,
    S: BuildHasher + Clone,
    const P: usize,
    F: FlatMode<K, V, S>,
> {
    Flat(Arc<F::Data>),
    Paged(Arc<PagedData<K, V, S, P>>),
}
impl<
        K: Clone + Hash + Eq,
        V: Clone,
        S: BuildHasher + Clone,
        const P: usize,
        F: FlatMode<K, V, S>,
    > Clone for Root<K, V, S, P, F>
{
    fn clone(&self) -> Self {
        match self {
            Self::Flat(d) => Self::Flat(d.clone()),
            Self::Paged(d) => Self::Paged(d.clone()),
        }
    }
}
/// Experimental adaptive COW map. Starts flat, then converts after DELAY
/// qualifying nonempty batches. SMALL is a percent threshold on submitted op
/// count; DROP_ONLY optionally gates on `needs_drop::<K/V>()`, a heuristic only.
/// With RESET_LARGE, batches with at least half as many submitted operations as
/// current bindings return to flat storage. This uses submitted op count, not unique changed keys, and can be
/// wrong for hot/duplicate-heavy batches. Conversion and first-use cache costs
/// are real work, never trace lookahead.
///
/// Roots clone one Arc instead of copying a directory. Shared-record traversal
/// caches preserve the exact key/value instances and clone no K/V on reads.
/// Raw batches can partially apply if user code panics; the external revision
/// adapter must stage publication. Interior mutability remains the caller's
/// responsibility. This is not the selected Store backend or a performance claim.
pub struct AdaptiveMap<
    K: Clone + Hash + Eq,
    V: Clone,
    S: BuildHasher + Clone = RandomState,
    const SMALL: usize = 1,
    const DELAY: usize = 3,
    const DROP_ONLY: bool = true,
    const PAGE: usize = 16,
    F: FlatMode<K, V, S> = InlineFlat,
    const RESET_LARGE: bool = true,
> {
    root: Root<K, V, S, PAGE, F>,
    qualifying: usize,
}
impl<
        K: Clone + Hash + Eq,
        V: Clone,
        S: BuildHasher + Clone,
        const T: usize,
        const D: usize,
        const B: bool,
        const P: usize,
        F: FlatMode<K, V, S>,
        const R: bool,
    > Clone for AdaptiveMap<K, V, S, T, D, B, P, F, R>
{
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            qualifying: self.qualifying,
        }
    }
}
impl<
        K: Clone + Hash + Eq,
        V: Clone,
        S: BuildHasher + Clone + Default,
        const T: usize,
        const D: usize,
        const B: bool,
        const P: usize,
        F: FlatMode<K, V, S>,
        const R: bool,
    > Default for AdaptiveMap<K, V, S, T, D, B, P, F, R>
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<
        K: Clone + Hash + Eq,
        V: Clone,
        S: BuildHasher + Clone,
        const T: usize,
        const D: usize,
        const B: bool,
        const P: usize,
        F: FlatMode<K, V, S>,
        const R: bool,
    > AdaptiveMap<K, V, S, T, D, B, P, F, R>
{
    /// Construct empty storage; unsupported policy constants panic.
    pub fn with_hasher(hasher: S) -> Self {
        assert!([1, 10, 100].contains(&T));
        assert!([1, 3].contains(&D));
        assert!([16, 64].contains(&P));
        Self {
            root: Root::Flat(Arc::new(F::new(hasher))),
            qualifying: 0,
        }
    }
    /// Number of live bindings in this root.
    pub fn len(&self) -> usize {
        match &self.root {
            Root::Flat(d) => F::len(d),
            Root::Paged(d) => d.map.len(),
        }
    }
    /// Whether this root contains no bindings.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Whether this root currently uses paged storage.
    pub fn is_paged(&self) -> bool {
        matches!(self.root, Root::Paged(_))
    }
    /// Consecutive qualifying nonempty batches considered by the policy.
    pub fn qualifying_batches(&self) -> usize {
        self.qualifying
    }
    /// Shared handles retained by this root's optional traversal cache.
    pub fn cached_records(&self) -> usize {
        match &self.root {
            Root::Flat(d) => F::cache_len(d),
            Root::Paged(d) => d.cache.len(),
        }
    }
    #[inline]
    /// Borrow a primary value using a compatible borrowed key.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        match &self.root {
            Root::Flat(d) => F::get(d, key),
            Root::Paged(d) => d.map.get(key),
        }
    }
    /// Visit each binding once in unspecified order without cloning K or V.
    pub fn visit<'a>(&'a self, mut f: impl FnMut(&'a K, &'a V)) {
        match &self.root {
            Root::Flat(d) => F::visit(d, f),
            Root::Paged(d) => {
                // Parent-private handles provide the same records as get/iter.
                if let Some(rows) = d.cache.entries(F::SCAN_AFTER, || {
                    let mut rows: RecordList<K, V> =
                        d.map.inner.iter().map(|(r, ())| r.0.clone()).collect();
                    F::order_scan_records(&mut rows);
                    rows
                }) {
                    for r in rows {
                        f(r.key(), r.value());
                    }
                } else {
                    for (k, v) in d.map.iter() {
                        f(k, v);
                    }
                }
            }
        }
    }
    /// Apply ordered raw operations; callers stage roots for atomic publication.
    pub fn apply(&mut self, ops: &[Mutation<K, V>]) {
        if ops.is_empty() {
            return;
        }
        let n = self.len();
        let large = R && n > 0 && ops.len() >= n / 2 + n % 2;
        if large {
            self.qualifying = 0;
            if let Root::Paged(data) = &self.root {
                let hasher = data.map.inner.hasher.as_ref().clone();
                let flat = if F::SHARED_TRANSITIONS {
                    F::from_shared_records(hasher, data.map.inner.iter().map(|(r, ())| r.0.clone()))
                } else {
                    F::from_entries(hasher, data.map.iter().map(|(k, v)| (k.clone(), v.clone())))
                };
                self.root = Root::Flat(Arc::new(flat));
            }
        }
        if let Root::Flat(data) = &self.root {
            let n = F::len(data);
            let limit = n / 100 * T + n % 100 * T / 100;
            let qualifies = !large
                && n > 0
                && ops.len() <= limit
                && (!B || std::mem::needs_drop::<K>() || std::mem::needs_drop::<V>());
            self.qualifying = if qualifies {
                self.qualifying.saturating_add(1)
            } else {
                0
            };
            if self.qualifying >= D {
                let mut map = RecordMap::with_hasher(F::hasher(data).clone());
                if F::SHARED_TRANSITIONS {
                    F::visit_shared_records(data, |record| {
                        map.inner.insert(RecordHandle(record.clone()), ());
                    });
                } else {
                    F::visit_primary(data, |k, v| {
                        map.insert(k.clone(), v.clone());
                    });
                }
                self.root = Root::Paged(Arc::new(PagedData {
                    map,
                    cache: ScanCache::new(),
                }));
            }
        }
        match &mut self.root {
            Root::Flat(data) => F::apply(Arc::make_mut(data), ops),
            Root::Paged(data) => {
                let data = Arc::make_mut(data);
                data.cache = ScanCache::new();
                data.map.apply(ops);
            }
        }
    }
}
