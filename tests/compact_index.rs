#![cfg(feature = "experimental")]
use rill_state::{
    experimental::{CompactIndexMap, HasherPolicy, OwnedHasher, SharedHasher},
    Mutation,
};
use std::{
    collections::BTreeMap,
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

#[derive(Clone, Default)]
struct Identity;
#[derive(Default)]
struct IdentityHash(u64);
impl Hasher for IdentityHash {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(31).wrapping_add(b as u64);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
impl BuildHasher for Identity {
    type Hasher = IdentityHash;
    fn build_hasher(&self) -> IdentityHash {
        IdentityHash::default()
    }
}

fn check<const L: usize, HP: HasherPolicy<Identity>, const B: bool, const A: bool>() {
    let mut map = CompactIndexMap::<u64, u64, Identity, L, HP, B, A>::default();
    let mut truth = BTreeMap::new();
    // Distinct full hashes sharing all 32 cached bits. Probe chains wrap from
    // the final slot, and zero/MAX fingerprints retain distinct complete keys.
    for i in 0..300 {
        let k = (i << 32) | u32::MAX as u64;
        map.insert(k, i);
        truth.insert(k, i);
    }
    map.insert(0, 700);
    truth.insert(0, 700);
    map.insert(u64::MAX, 701);
    truth.insert(u64::MAX, 701);
    let old = map.clone();
    let old_truth = truth.clone();
    for i in (0..300).step_by(3) {
        let k = (i << 32) | u32::MAX as u64;
        assert_eq!(map.remove(&k), truth.remove(&k));
    }
    for (&k, &v) in &truth {
        assert_eq!(map.get(&k), Some(&v));
    }
    assert_eq!(
        map.iter()
            .map(|(k, v)| (*k, *v))
            .collect::<BTreeMap<_, _>>(),
        truth
    );
    let mut random = 194411u64;
    for step in 0..2000 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let k = random % 1024;
        if random & 7 < 3 {
            assert_eq!(map.remove(&k), truth.remove(&k));
        } else {
            assert_eq!(map.insert(k, step), truth.insert(k, step));
        }
        assert_eq!(map.len(), truth.len());
        assert_eq!(map.get(&k), truth.get(&k));
    }
    map.apply(&[
        Mutation::Put(12, 1),
        Mutation::Delete(12),
        Mutation::Delete(12),
        Mutation::Put(12, 2),
        Mutation::Put(12, 3),
    ]);
    truth.insert(12, 3);
    assert_eq!(map.iter().len(), truth.len());
    assert_eq!(
        map.iter()
            .map(|(k, v)| (*k, *v))
            .collect::<BTreeMap<_, _>>(),
        truth
    );
    for (&k, &v) in &truth {
        assert_eq!(map.get(&k), Some(&v));
    }
    assert_eq!(
        old.iter()
            .map(|(k, v)| (*k, *v))
            .collect::<BTreeMap<_, _>>(),
        old_truth
    );
    for (&k, &v) in &old_truth {
        assert_eq!(old.get(&k), Some(&v));
    }
}
#[test]
fn all_load_and_builder_variants_preserve_collisions_growth_and_old_views() {
    check::<25, SharedHasher, false, false>();
    check::<25, SharedHasher, false, true>();
    check::<25, SharedHasher, true, false>();
    check::<25, SharedHasher, true, true>();
    check::<50, SharedHasher, false, false>();
    check::<50, SharedHasher, false, true>();
    check::<50, SharedHasher, true, false>();
    check::<50, SharedHasher, true, true>();
    check::<70, SharedHasher, false, false>();
    check::<70, SharedHasher, false, true>();
    check::<70, SharedHasher, true, false>();
    check::<70, SharedHasher, true, true>();
    check::<85, SharedHasher, false, false>();
    check::<85, SharedHasher, false, true>();
    check::<85, SharedHasher, true, false>();
    check::<85, SharedHasher, true, true>();
    check::<25, OwnedHasher, false, false>();
    check::<25, OwnedHasher, false, true>();
    check::<25, OwnedHasher, true, false>();
    check::<25, OwnedHasher, true, true>();
    check::<50, OwnedHasher, false, false>();
    check::<50, OwnedHasher, false, true>();
    check::<50, OwnedHasher, true, false>();
    check::<50, OwnedHasher, true, true>();
    check::<70, OwnedHasher, false, false>();
    check::<70, OwnedHasher, false, true>();
    check::<70, OwnedHasher, true, false>();
    check::<70, OwnedHasher, true, true>();
    check::<85, OwnedHasher, false, false>();
    check::<85, OwnedHasher, false, true>();
    check::<85, OwnedHasher, true, false>();
    check::<85, OwnedHasher, true, true>();
}
#[test]
fn every_swap_removal_victim_repairs_wraparound_indices() {
    for cached in [0, 15, u32::MAX as u64] {
        for victim in 0..128u64 {
            let mut map = CompactIndexMap::<u64, u64, Identity>::default();
            for i in 0..128 {
                map.insert((i << 32) | cached, i);
            }
            assert_eq!(map.remove(&((victim << 32) | cached)), Some(victim));
            assert_eq!(map.len(), 127);
            for i in 0..128 {
                assert_eq!(
                    map.get(&((i << 32) | cached)),
                    if i == victim { None } else { Some(&i) }
                );
            }
        }
    }
}
#[derive(Clone, Debug)]
struct Canonical {
    id: u64,
    metadata: u64,
}
impl PartialEq for Canonical {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Canonical {}
impl Hash for Canonical {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h)
    }
}
#[test]
fn borrowed_strings_canonical_keys_and_reinserted_metadata() {
    let mut strings = CompactIndexMap::<String, u64>::default();
    strings.insert("hello".into(), 7);
    assert_eq!(strings.get("hello"), Some(&7));
    assert_eq!(strings.remove("hello"), Some(7));
    assert!(strings.is_empty());
    let mut map = CompactIndexMap::<Canonical, u64, Identity>::default();
    map.insert(
        Canonical {
            id: 1,
            metadata: 10,
        },
        1,
    );
    map.insert(
        Canonical {
            id: 1,
            metadata: 20,
        },
        2,
    );
    map.apply(&[Mutation::Put(
        Canonical {
            id: 1,
            metadata: 30,
        },
        3,
    )]);
    assert_eq!(map.iter().next().unwrap().0.metadata, 10);
    map.remove(&Canonical {
        id: 1,
        metadata: 40,
    });
    map.insert(
        Canonical {
            id: 1,
            metadata: 50,
        },
        4,
    );
    assert_eq!(map.iter().next().unwrap().0.metadata, 50);
}
#[derive(Debug)]
struct PanicValue {
    id: u64,
    fail: Arc<AtomicBool>,
}
impl Clone for PanicValue {
    fn clone(&self) -> Self {
        assert!(
            !self.fail.load(Ordering::Relaxed),
            "injected value clone panic"
        );
        Self {
            id: self.id,
            fail: self.fail.clone(),
        }
    }
}
#[test]
fn failed_table_detachment_preserves_all_current_and_retained_bindings() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut map = CompactIndexMap::<u64, PanicValue, Identity>::default();
    for i in 0..100 {
        map.insert(
            i,
            PanicValue {
                id: i,
                fail: fail.clone(),
            },
        );
    }
    let old = map.clone();
    fail.store(true, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.insert(
            200,
            PanicValue {
                id: 200,
                fail: Arc::new(AtomicBool::new(false)),
            },
        );
    }))
    .is_err());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.remove(&0))).is_err());
    assert_eq!(map.len(), 100);
    assert_eq!(map.iter().len(), 100);
    assert_eq!(old.len(), 100);
    for i in 0..100 {
        assert_eq!(map.get(&i).unwrap().id, i);
        assert_eq!(old.get(&i).unwrap().id, i);
    }
    fail.store(false, Ordering::Relaxed);
    map.remove(&0);
    assert_eq!(old.get(&0).unwrap().id, 0);
}
#[derive(Clone)]
struct PanicHash {
    fail: Arc<AtomicU64>,
}
struct FailingHasher {
    fail: Arc<AtomicU64>,
    value: u64,
}
impl Hasher for FailingHasher {
    fn write(&mut self, _: &[u8]) {
        panic!("u64-only test hasher");
    }
    fn write_u64(&mut self, v: u64) {
        assert_ne!(self.fail.load(Ordering::Relaxed), v, "injected hash panic");
        self.value = v;
    }
    fn finish(&self) -> u64 {
        self.value
    }
}
impl BuildHasher for PanicHash {
    type Hasher = FailingHasher;
    fn build_hasher(&self) -> FailingHasher {
        FailingHasher {
            fail: self.fail.clone(),
            value: 0,
        }
    }
}
#[test]
fn moved_row_hash_panic_occurs_before_index_or_rows_change() {
    let fail = Arc::new(AtomicU64::new(u64::MAX));
    let mut map =
        CompactIndexMap::<u64, u64, PanicHash>::with_hasher(PanicHash { fail: fail.clone() });
    for i in 0..100 {
        map.insert(i, i);
    }
    fail.store(99, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.remove(&0))).is_err());
    fail.store(u64::MAX, Ordering::Relaxed);
    assert_eq!(map.len(), 100);
    for i in 0..100 {
        assert_eq!(map.get(&i), Some(&i));
    }
}
#[derive(Clone)]
struct DropKey {
    id: u64,
    fail: Arc<AtomicBool>,
}
impl PartialEq for DropKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for DropKey {}
impl Hash for DropKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h)
    }
}
impl Drop for DropKey {
    fn drop(&mut self) {
        assert!(
            !self.fail.swap(false, Ordering::Relaxed),
            "injected key drop panic"
        );
    }
}
#[test]
fn removed_key_destructor_panic_leaves_dense_and_probe_indices_consistent() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut map = CompactIndexMap::<DropKey, u64, Identity>::default();
    for i in 0..100 {
        map.insert(
            DropKey {
                id: i,
                fail: if i == 0 {
                    fail.clone()
                } else {
                    Arc::new(AtomicBool::new(false))
                },
            },
            i,
        );
    }
    fail.store(true, Ordering::Relaxed);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.remove(&DropKey {
            id: 0,
            fail: Arc::new(AtomicBool::new(false))
        })))
        .is_err()
    );
    assert_eq!(map.len(), 99);
    assert_eq!(map.iter().len(), 99);
    for i in 1..100 {
        assert_eq!(
            map.get(&DropKey {
                id: i,
                fail: Arc::new(AtomicBool::new(false))
            }),
            Some(&i)
        );
    }
}
#[test]
fn final_payload_owners_are_reclaimed_and_invalid_loads_rejected() {
    let payload = Arc::new(vec![7u8; 256]);
    let weak = Arc::downgrade(&payload);
    let mut map = CompactIndexMap::<u64, Arc<Vec<u8>>>::default();
    map.insert(1, payload);
    let old = map.clone();
    let removed = map.remove(&1);
    drop(map);
    drop(removed);
    assert!(weak.upgrade().is_some());
    drop(old);
    assert!(weak.upgrade().is_none());
    assert!(std::panic::catch_unwind(CompactIndexMap::<u64, u64, Identity, 0>::default).is_err());
    assert!(std::panic::catch_unwind(CompactIndexMap::<u64, u64, Identity, 100>::default).is_err());
}

struct CountedBuilder(Arc<AtomicU64>);
struct CountedHasher {
    calls: Arc<AtomicU64>,
    value: u64,
}
impl BuildHasher for CountedBuilder {
    type Hasher = CountedHasher;
    fn build_hasher(&self) -> CountedHasher {
        CountedHasher {
            calls: self.0.clone(),
            value: 0,
        }
    }
}
impl Hasher for CountedHasher {
    fn write(&mut self, _: &[u8]) {
        panic!("u64-only test hasher");
    }
    fn write_u64(&mut self, v: u64) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.value = v;
    }
    fn finish(&self) -> u64 {
        self.value
    }
}
#[test]
fn growth_uses_cached_words_without_rehashing_and_shared_builder_need_not_clone() {
    let count = Arc::new(AtomicU64::new(0));
    let mut map =
        CompactIndexMap::<u64, u64, CountedBuilder>::with_hasher(CountedBuilder(count.clone()));
    for i in 0..4000 {
        map.insert(i, i);
    }
    assert_eq!(count.load(Ordering::Relaxed), 4000);
    let old = map.clone();
    assert_eq!(count.load(Ordering::Relaxed), 4000);
    map.apply(&[Mutation::Put(2000, 9000)]);
    assert_eq!(count.load(Ordering::Relaxed), 4001);
    assert_eq!(old.get(&2000), Some(&2000));
    assert_eq!(map.get(&2000), Some(&9000));
}

#[derive(Clone)]
struct DropValue {
    id: u64,
    fail: Arc<AtomicBool>,
}
impl Drop for DropValue {
    fn drop(&mut self) {
        assert!(
            !self.fail.swap(false, Ordering::Relaxed),
            "injected value drop panic"
        );
    }
}
#[test]
fn replacement_value_destructor_panic_keeps_the_installed_binding_valid() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut map = CompactIndexMap::<u64, DropValue, Identity>::default();
    map.insert(
        0,
        DropValue {
            id: 0,
            fail: fail.clone(),
        },
    );
    map.insert(
        1,
        DropValue {
            id: 1,
            fail: Arc::new(AtomicBool::new(false)),
        },
    );
    fail.store(true, Ordering::Relaxed);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.apply(&[
            Mutation::Put(
                0,
                DropValue {
                    id: 2,
                    fail: Arc::new(AtomicBool::new(false))
                }
            )
        ])))
        .is_err()
    );
    assert_eq!(map.len(), 2);
    assert_eq!(map.iter().len(), 2);
    assert_eq!(map.get(&0).unwrap().id, 2);
    assert_eq!(map.get(&1).unwrap().id, 1);
}

fn batch_drop<const B: bool, const A: bool>() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut map = CompactIndexMap::<u64, DropValue, Identity, 70, SharedHasher, B, A>::default();
    for i in 0..100 {
        map.insert(
            (i << 32) | 15,
            DropValue {
                id: i,
                fail: if i == 0 {
                    fail.clone()
                } else {
                    Arc::new(AtomicBool::new(false))
                },
            },
        );
    }
    fail.store(true, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || map.apply(&[Mutation::Delete(15)])
    ))
    .is_err());
    assert_eq!(map.len(), 99);
    assert_eq!(map.iter().len(), 99);
    assert!(map.get(&15).is_none());
    for i in 1..100 {
        assert_eq!(map.get(&((i << 32) | 15)).unwrap().id, i);
    }
}
#[test]
fn both_batch_policies_repair_indices_before_removed_value_destructors_run() {
    batch_drop::<false, false>();
    batch_drop::<false, true>();
    batch_drop::<true, false>();
    batch_drop::<true, true>();
}

#[test]
fn widening_hash_failure_preserves_both_representations_and_all_rows() {
    const BEFORE: u64 = 65536 * 70 / 100;
    let fail = Arc::new(AtomicU64::new(u64::MAX));
    let mut map =
        CompactIndexMap::<u64, u64, PanicHash>::with_hasher(PanicHash { fail: fail.clone() });
    for i in 0..BEFORE {
        map.insert(i, i);
    }
    assert_eq!(map.index_word_bytes(), 4);
    assert_eq!(map.index_capacity(), 65536);
    let old = map.clone();
    fail.store(12, Ordering::Relaxed);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.insert(100000, 100000)))
            .is_err()
    );
    fail.store(u64::MAX, Ordering::Relaxed);
    assert_eq!(map.len(), BEFORE as usize);
    assert_eq!(map.index_word_bytes(), 4);
    assert_eq!(map.index_capacity(), 65536);
    assert!(map.get(&100000).is_none());
    for i in 0..BEFORE {
        assert_eq!(map.get(&i), Some(&i));
        assert_eq!(old.get(&i), Some(&i));
    }
    assert_eq!(map.insert(100000, 100000), None);
    assert_eq!(map.index_word_bytes(), 8);
    assert_eq!(map.index_capacity(), 131072);
    assert_eq!(old.index_word_bytes(), 4);
    assert_eq!(old.len(), BEFORE as usize);
    // Exercise a subsequent cached-hash wide growth, far beyond 16-bit rows.
    for i in BEFORE..100000 {
        map.insert(i, i);
    }
    assert_eq!(map.len(), 100001);
    assert_eq!(map.index_capacity(), 262144);
    for i in 0..=100000 {
        assert_eq!(map.get(&i), Some(&i));
    }
    assert_eq!(map.remove(&0), Some(0));
    assert_eq!(map.remove(&100000), Some(100000));
    for i in 1..100000 {
        assert_eq!(map.get(&i), Some(&i));
    }
    for i in 0..BEFORE {
        assert_eq!(old.get(&i), Some(&i));
    }
}
#[test]
fn widening_reconstructs_missing_hash_bits_exactly_once() {
    const BEFORE: u64 = 65536 * 70 / 100;
    let calls = Arc::new(AtomicU64::new(0));
    let mut map =
        CompactIndexMap::<u64, u64, CountedBuilder>::with_hasher(CountedBuilder(calls.clone()));
    for i in 0..BEFORE {
        map.insert(i, i);
    }
    assert_eq!(calls.load(Ordering::Relaxed), BEFORE);
    map.insert(100000, 100000);
    assert_eq!(calls.load(Ordering::Relaxed), BEFORE * 2 + 1);
    assert_eq!(map.index_word_bytes(), 8);
    map.insert(100001, 100001);
    assert_eq!(calls.load(Ordering::Relaxed), BEFORE * 2 + 2);
}
