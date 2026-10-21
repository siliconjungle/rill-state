//! Versioned in-memory state with immutable snapshots and a bounded mutation journal.
//!
//! A store has one owner. Snapshots can be sent to readers and retained independently.
//! A successful commit publishes all mutations under one revision. The journal records
//! submitted operations in order (including repeated keys and deletes of absent keys),
//! not a coalesced state diff. Arbitrarily retained snapshots can retain arbitrary memory;
//! journal limits do not bound memory held by the application.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{hash_map::RandomState, VecDeque};
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A mutation; multiple operations on a key are applied in submission order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mutation<K, V> {
    /// Insert a key or replace its previous value.
    Put(K, V),
    /// Remove a key if present.
    Delete(K),
}

/// An opaque, process-local cursor bound to one store lineage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    store: u64,
    revision: u64,
}
impl Cursor {
    /// Monotonic revision within this cursor's store. This alone is not a transferable cursor.
    pub fn revision(self) -> u64 {
        self.revision
    }
}

/// Limits and layout for a store.
#[derive(Clone, Debug)]
pub struct Config {
    /// Power-of-two number of hash partitions (1..=1,048,576).
    pub partitions: usize,
    /// Power-of-two partitions per copy-on-write directory branch, at most `partitions`.
    pub branch_size: usize,
    /// Maximum journal batches retained, including empty commits.
    pub history_batches: usize,
    /// Maximum total submitted operations retained. Oversized batches cause a journal gap.
    pub history_operations: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            partitions: 4096,
            branch_size: 64,
            history_batches: 64,
            history_operations: 1_000_000,
        }
    }
}

/// An invalid layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidConfig;
impl std::fmt::Display for InvalidConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("partition/branch sizes must be powers of two, branch <= partitions <= 1048576")
    }
}
impl std::error::Error for InvalidConfig {}

#[derive(Clone)]
struct Entry<K, V> {
    hash: u64,
    key: K,
    value: V,
}
type Page<K, V> = Arc<Vec<Entry<K, V>>>;
type Branch<K, V> = Arc<Vec<Option<Page<K, V>>>>;

struct Root<K, V, S> {
    branches: Vec<Branch<K, V>>,
    len: usize,
    hasher: Arc<S>,
}
impl<K, V, S> Clone for Root<K, V, S> {
    fn clone(&self) -> Self {
        Self {
            branches: self.branches.clone(),
            len: self.len,
            hasher: self.hasher.clone(),
        }
    }
}

/// A cheap, independently retained, immutable read view of one complete revision.
pub struct Snapshot<K, V, S = RandomState> {
    root: Arc<Root<K, V, S>>,
    cursor: Cursor,
    mask: usize,
    shift: u32,
}
impl<K, V, S> Clone for Snapshot<K, V, S> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            cursor: self.cursor,
            mask: self.mask,
            shift: self.shift,
        }
    }
}
impl<K, V, S> Snapshot<K, V, S> {
    /// Cursor at this view's complete revision.
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }
    /// Number of live keys.
    pub fn len(&self) -> usize {
        self.root.len
    }
    /// Whether there are no live keys.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Visit all entries. Order is unspecified and can change between revisions.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.root
            .branches
            .iter()
            .flat_map(|b| b.iter())
            .filter_map(|p| p.as_ref())
            .flat_map(|p| p.iter())
            .map(|e| (&e.key, &e.value))
    }
}
impl<K: Hash + Eq, V, S: BuildHasher> Snapshot<K, V, S> {
    /// Get a value in this immutable revision.
    pub fn get(&self, key: &K) -> Option<&V> {
        let hash = hash_key(&*self.root.hasher, key);
        let slot = hash as usize & self.mask;
        let page =
            self.root.branches[slot >> self.shift][slot & ((1 << self.shift) - 1)].as_ref()?;
        page.iter()
            .find(|e| e.hash == hash && &e.key == key)
            .map(|e| &e.value)
    }
}
fn hash_key<K: Hash, S: BuildHasher>(state: &S, key: &K) -> u64 {
    let mut h = state.build_hasher();
    key.hash(&mut h);
    h.finish()
}

/// One committed batch of ordered mutations.
#[derive(Clone, Debug)]
pub struct Batch<K, V> {
    cursor: Cursor,
    operations: Arc<[Mutation<K, V>]>,
}
impl<K, V> Batch<K, V> {
    /// Cursor after all operations in this batch.
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }
    /// Exact submitted operations, in order.
    pub fn operations(&self) -> &[Mutation<K, V>] {
        &self.operations
    }
}

/// A change request could not be served completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedError {
    /// This cursor belongs to another store.
    ForeignCursor,
    /// This revision is newer than the store.
    FutureCursor,
    /// History has been evicted; install a fresh snapshot and use its cursor.
    Gap {
        /// Oldest revision from which a complete change feed remains available.
        oldest: Cursor,
        /// Current complete revision.
        current: Cursor,
    },
}
impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FeedError {}

/// Revision counter exhausted; the store was not modified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevisionExhausted;
impl std::fmt::Display for RevisionExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("revision exhausted")
    }
}
impl std::error::Error for RevisionExhausted {}

/// A single-writer state owner. Readers retain independent [`Snapshot`] values.
pub struct Store<K, V, S = RandomState> {
    current: Snapshot<K, V, S>,
    config: Config,
    history: VecDeque<Arc<Batch<K, V>>>,
    retained_operations: usize,
}
impl<K: Clone + Hash + Eq, V: Clone> Default for Store<K, V> {
    fn default() -> Self {
        Self::new(Config::default()).expect("default config valid")
    }
}
impl<K: Clone + Hash + Eq, V: Clone> Store<K, V> {
    /// Create a store with a keyed, randomly seeded hash function.
    pub fn new(config: Config) -> Result<Self, InvalidConfig> {
        Self::with_hasher(config, RandomState::new())
    }
}
impl<K, V, S> Store<K, V, S> {
    /// Clone the complete current revision, without copying entries.
    pub fn snapshot(&self) -> Snapshot<K, V, S> {
        self.current.clone()
    }
    /// Current journal batch and operation counts. Pinned external batches are not counted.
    pub fn retained_history(&self) -> (usize, usize) {
        (self.history.len(), self.retained_operations)
    }
    /// Get all batches after the cursor, or explicitly report why this cannot be done.
    ///
    /// Borrow the result for allocation-free processing, or clone each `Arc<Batch>` to
    /// retain it beyond the next writer operation. Such retained memory belongs to the caller.
    pub fn changes_since(
        &self,
        cursor: Cursor,
    ) -> Result<impl ExactSizeIterator<Item = &Arc<Batch<K, V>>>, FeedError> {
        let current = self.current.cursor;
        if cursor.store != current.store {
            return Err(FeedError::ForeignCursor);
        }
        if cursor.revision > current.revision {
            return Err(FeedError::FutureCursor);
        }
        let oldest = self
            .history
            .front()
            .map_or(current.revision, |b| b.cursor.revision - 1);
        if cursor.revision < oldest {
            return Err(FeedError::Gap {
                oldest: Cursor {
                    store: current.store,
                    revision: oldest,
                },
                current,
            });
        }
        let skip = (cursor.revision - oldest) as usize;
        Ok(self.history.iter().skip(skip))
    }
}
impl<K: Clone + Hash + Eq, V: Clone, S: BuildHasher> Store<K, V, S> {
    /// Create a store with a caller-selected hasher. Weak hashers require trusted keys.
    pub fn with_hasher(config: Config, hasher: S) -> Result<Self, InvalidConfig> {
        if !config.partitions.is_power_of_two()
            || config.partitions > 1 << 20
            || !config.branch_size.is_power_of_two()
            || config.branch_size > config.partitions
        {
            return Err(InvalidConfig);
        }
        let branch = Arc::new(vec![None; config.branch_size]);
        let root = Arc::new(Root {
            branches: vec![branch; config.partitions / config.branch_size],
            len: 0,
            hasher: Arc::new(hasher),
        });
        let store = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("store identity exhausted");
        Ok(Self {
            current: Snapshot {
                root,
                cursor: Cursor { store, revision: 0 },
                mask: config.partitions - 1,
                shift: config.branch_size.trailing_zeros(),
            },
            config,
            history: VecDeque::new(),
            retained_operations: 0,
        })
    }
    /// Apply and publish a complete batch. Empty batches also advance the revision.
    ///
    /// Work is staged independently; a panicking key/value clone or iterator cannot
    /// publish a partially applied revision. Values with interior mutability still require
    /// the caller to respect snapshot immutability, just as with `Arc<T>`.
    pub fn commit(
        &mut self,
        operations: impl IntoIterator<Item = Mutation<K, V>>,
    ) -> Result<Cursor, RevisionExhausted> {
        let revision = self
            .current
            .cursor
            .revision
            .checked_add(1)
            .ok_or(RevisionExhausted)?;
        let operations: Arc<[Mutation<K, V>]> = operations.into_iter().collect::<Vec<_>>().into();
        let mut next = self.current.clone();
        if !operations.is_empty() {
            let root = Arc::make_mut(&mut next.root);
            for op in operations.iter() {
                let key = match op {
                    Mutation::Put(k, _) | Mutation::Delete(k) => k,
                };
                let hash = hash_key(&*root.hasher, key);
                let slot = hash as usize & next.mask;
                let branch = Arc::make_mut(&mut root.branches[slot >> next.shift]);
                let page_slot = &mut branch[slot & ((1 << next.shift) - 1)];
                if page_slot.is_none() && matches!(op, Mutation::Delete(_)) {
                    continue;
                }
                let page = Arc::make_mut(page_slot.get_or_insert_with(|| Arc::new(Vec::new())));
                let found = page.iter().position(|e| e.hash == hash && &e.key == key);
                match op {
                    Mutation::Put(k, v) => {
                        if let Some(i) = found {
                            page[i].value = v.clone();
                        } else {
                            page.push(Entry {
                                hash,
                                key: k.clone(),
                                value: v.clone(),
                            });
                            root.len += 1;
                        }
                    }
                    Mutation::Delete(_) => {
                        if let Some(i) = found {
                            page.swap_remove(i);
                            root.len -= 1;
                        }
                    }
                }
                if page.is_empty() {
                    *page_slot = None;
                }
            }
        }
        next.cursor.revision = revision;
        let cursor = next.cursor;
        let batch = Arc::new(Batch { cursor, operations });
        // Reserve before publication so allocator failures cannot leave a visible revision
        // without its corresponding journal record. Rust allocation failure normally aborts.
        self.history.reserve(1);
        self.retained_operations += batch.operations.len();
        self.history.push_back(batch);
        self.current = next;
        while self.history.len() > self.config.history_batches
            || self.retained_operations > self.config.history_operations
        {
            self.retained_operations -= self
                .history
                .pop_front()
                .expect("nonempty over budget")
                .operations
                .len();
        }
        Ok(cursor)
    }
}

#[cfg(test)]
mod internal_tests {
    use super::*;
    #[test]
    fn revision_exhaustion_does_not_change_state() {
        let mut s = Store::<u32, u32>::default();
        s.current.cursor.revision = u64::MAX;
        assert_eq!(s.commit([Mutation::Put(1, 2)]), Err(RevisionExhausted));
        assert!(s.snapshot().is_empty());
        assert_eq!(s.retained_history(), (0, 0));
    }
    #[test]
    fn future_cursor_rejected() {
        let s = Store::<u32, u32>::default();
        let c = Cursor {
            revision: 1,
            ..s.snapshot().cursor()
        };
        assert!(matches!(s.changes_since(c), Err(FeedError::FutureCursor)));
    }
}
