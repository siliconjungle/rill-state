#![cfg(feature = "experimental")]
use rill_state::experimental::{
    ArrayRecordMap, CopyKeys, HasherPolicy, InlineSliceMap, KeyPolicy, OwnedHasher, ProbeMap,
    RecordMap, SharedHasher, SharedKeys, SharedMap, SplitMap,
};
use rill_state::Mutation;
use std::{
    collections::BTreeMap,
    hash::{BuildHasherDefault, Hasher},
    sync::Arc,
};

#[derive(Default, Clone)]
struct Wrap;
impl Hasher for Wrap {
    fn finish(&self) -> u64 {
        u64::MAX
    }
    fn write(&mut self, _: &[u8]) {}
}
#[derive(Default, Clone)]
struct Zero;
impl Hasher for Zero {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {}
}
trait Subject: Clone {
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>>;
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>>;
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>>;
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>>;
    fn len(&self) -> usize;
    fn batch(&mut self, ops: Vec<Mutation<String, Arc<Vec<u8>>>>) {
        for op in ops {
            match op {
                Mutation::Put(k, v) => {
                    self.put(k, v);
                }
                Mutation::Delete(k) => {
                    self.del(&k);
                }
            }
        }
    }
}
impl<S: std::hash::BuildHasher, const L: usize, const P: usize> Subject
    for ProbeMap<String, Arc<Vec<u8>>, S, L, P>
{
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        self.insert(k, v)
    }
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>> {
        self.remove(k)
    }
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>> {
        self.get(k)
    }
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn batch(&mut self, ops: Vec<Mutation<String, Arc<Vec<u8>>>>) {
        self.apply(&ops);
    }
}
impl<S: std::hash::BuildHasher, const L: usize> Subject for SplitMap<String, Arc<Vec<u8>>, S, L> {
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        self.insert(k, v)
    }
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>> {
        self.remove(k)
    }
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>> {
        self.get(k)
    }
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    fn len(&self) -> usize {
        self.len()
    }
}
impl<S: std::hash::BuildHasher, const L: usize, const P: usize> Subject
    for SharedMap<String, Arc<Vec<u8>>, S, L, P>
{
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        self.insert(k, v).map(|v| v.as_ref().clone())
    }
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>> {
        self.remove(k).map(|v| v.as_ref().clone())
    }
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>> {
        self.get(k)
    }
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn batch(&mut self, ops: Vec<Mutation<String, Arc<Vec<u8>>>>) {
        self.apply(&ops);
    }
}
impl<S: std::hash::BuildHasher, const L: usize, const P: usize> Subject
    for RecordMap<String, Arc<Vec<u8>>, S, L, P>
{
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        self.insert(k, v).map(|r| r.value().clone())
    }
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>> {
        self.remove(k).map(|r| r.value().clone())
    }
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>> {
        self.get(k)
    }
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn batch(&mut self, ops: Vec<Mutation<String, Arc<Vec<u8>>>>) {
        self.apply(&ops);
    }
}
#[test]
fn packed_records_preserve_borrowed_reads_collisions_and_retained_views() {
    macro_rules! pages {
        ($l:literal) => {
            oracle(RecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 0>::default());
            oracle(RecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 16>::default());
            oracle(RecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 64>::default());
            oracle(RecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 256>::default());
        };
    }
    pages!(50);
    pages!(70);
    pages!(85);
    oracle(RecordMap::<_, _, BuildHasherDefault<Zero>, 70, 16>::default());
}
#[test]
fn packed_records_keep_canonical_key_metadata_on_equal_key_replacement() {
    #[derive(Clone)]
    struct Key(u64, &'static str);
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Eq for Key {}
    impl std::hash::Hash for Key {
        fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
            self.0.hash(h)
        }
    }
    let mut map = RecordMap::<Key, u64>::default();
    map.insert(Key(1, "canonical"), 10);
    let old = map.clone();
    map.insert(Key(1, "replacement"), 20);
    assert_eq!(map.iter().next().unwrap().0 .1, "canonical");
    map.apply(&[Mutation::Put(Key(1, "batch replacement"), 30)]);
    let (k, v) = map.iter().next().unwrap();
    assert_eq!(k.1, "canonical");
    assert_eq!(*v, 30);
    let (k, v) = old.iter().next().unwrap();
    assert_eq!(k.1, "canonical");
    assert_eq!(*v, 10);
}
#[test]
fn packed_records_release_after_last_snapshot_and_removed_handle() {
    let value = Arc::new(vec![7u8; 4096]);
    let weak = Arc::downgrade(&value);
    let mut map = RecordMap::<String, _>::default();
    map.insert("key".into(), value);
    let view = map.clone();
    let removed = map.remove("key").unwrap();
    assert_eq!(removed.key(), "key");
    assert_eq!(removed.value().len(), 4096);
    drop(view);
    assert!(weak.upgrade().is_some());
    drop(removed);
    assert!(weak.upgrade().is_none());
    // Snapshot cloning itself imposes no Clone bound on the underlying K/V.
    struct NonClone;
    let mut map = RecordMap::<u64, NonClone>::default();
    map.insert(1, NonClone);
    let old = map.clone();
    map.remove(&1);
    assert!(old.get(&1).is_some());
}
#[test]
fn shared_records_survive_collision_growth_deletion_and_old_readers() {
    macro_rules! pages {
        ($l:literal) => {
            oracle(SharedMap::<_, _, BuildHasherDefault<Wrap>, $l, 0>::default());
            oracle(SharedMap::<_, _, BuildHasherDefault<Wrap>, $l, 16>::default());
            oracle(SharedMap::<_, _, BuildHasherDefault<Wrap>, $l, 64>::default());
            oracle(SharedMap::<_, _, BuildHasherDefault<Wrap>, $l, 256>::default());
        };
    }
    pages!(50);
    pages!(70);
    pages!(85);
    oracle(SharedMap::<_, _, BuildHasherDefault<Zero>, 70, 16>::default());
}
#[test]
fn shared_records_clone_only_changed_values_and_release_final_owners() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Eq, PartialEq, Hash)]
    struct CountedKey(String);
    // Keys deliberately do not implement Clone: ordinary inserts, reads, removes
    // and snapshot cloning must work without copying any underlying key or value.
    struct CountedValue(Arc<AtomicUsize>);
    impl Drop for CountedValue {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut map = SharedMap::<CountedKey, CountedValue>::default();
    map.insert(CountedKey("a".into()), CountedValue(drops.clone()));
    let old = map.clone();
    map.insert(CountedKey("a".into()), CountedValue(drops.clone()));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(old);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(map);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}
impl<
        S: std::hash::BuildHasher,
        const L: usize,
        const P: usize,
        KP: KeyPolicy<String>,
        HP: HasherPolicy<S>,
    > Subject for ArrayRecordMap<String, Arc<Vec<u8>>, S, L, P, KP, HP>
{
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        self.insert(k, v).map(|r| r.value().clone())
    }
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>> {
        self.remove(k).map(|r| r.value().clone())
    }
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>> {
        self.get(k)
    }
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn batch(&mut self, ops: Vec<Mutation<String, Arc<Vec<u8>>>>) {
        self.apply(&ops);
    }
}
#[test]
fn owned_hashers_preserve_cow_snapshots_collisions_and_borrowed_keys() {
    oracle(ArrayRecordMap::<
        _,
        _,
        BuildHasherDefault<Wrap>,
        50,
        16,
        CopyKeys,
        OwnedHasher,
    >::default());
    oracle(ArrayRecordMap::<
        _,
        _,
        BuildHasherDefault<Wrap>,
        70,
        256,
        SharedKeys,
        OwnedHasher,
    >::default());
    oracle(ArrayRecordMap::<
        _,
        _,
        BuildHasherDefault<Zero>,
        85,
        64,
        CopyKeys,
        OwnedHasher,
    >::default());
    oracle(ArrayRecordMap::<
        _,
        _,
        BuildHasherDefault<Zero>,
        25,
        1024,
        SharedKeys,
        OwnedHasher,
    >::default());
}
#[test]
fn owned_and_shared_hashers_keep_the_same_seed_across_map_clones() {
    #[derive(Clone)]
    struct Seeded(u64);
    impl std::hash::BuildHasher for Seeded {
        type Hasher = std::collections::hash_map::DefaultHasher;
        fn build_hasher(&self) -> Self::Hasher {
            let mut h = Self::Hasher::new();
            h.write_u64(self.0);
            h
        }
    }
    oracle(
        ArrayRecordMap::<_, _, _, 70, 16, CopyKeys, OwnedHasher>::with_hasher(Seeded(0x452acfa7)),
    );
    oracle(
        ArrayRecordMap::<_, _, _, 50, 64, SharedKeys, OwnedHasher>::with_hasher(Seeded(0xcad65abe)),
    );
    oracle(
        ArrayRecordMap::<_, _, _, 70, 16, CopyKeys, SharedHasher>::with_hasher(Seeded(0x452acfa7)),
    );
}
#[test]
fn shared_hasher_snapshot_cloning_does_not_require_builder_clone() {
    struct NotClone;
    impl std::hash::BuildHasher for NotClone {
        type Hasher = std::collections::hash_map::DefaultHasher;
        fn build_hasher(&self) -> Self::Hasher {
            Self::Hasher::new()
        }
    }
    let mut map = ArrayRecordMap::<u64, u64, _, 70, 16, SharedKeys>::with_hasher(NotClone);
    map.insert(1, 2);
    let old = map.clone();
    map.insert(1, 3);
    assert_eq!(old.get(&1), Some(&2));
    assert_eq!(map.get(&1), Some(&3));
}
#[test]
fn array_pages_preserve_collisions_wraparound_batches_and_borrowed_reads() {
    macro_rules! pages {
        ($l:literal,$kp:ty) => {
            oracle(ArrayRecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 1, $kp>::default());
            oracle(ArrayRecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 16, $kp>::default());
            oracle(ArrayRecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 64, $kp>::default());
            oracle(ArrayRecordMap::<_, _, BuildHasherDefault<Wrap>, $l, 256, $kp>::default());
        };
    }
    pages!(25, CopyKeys);
    pages!(50, CopyKeys);
    pages!(70, CopyKeys);
    pages!(85, CopyKeys);
    pages!(25, SharedKeys);
    pages!(50, SharedKeys);
    pages!(70, SharedKeys);
    pages!(85, SharedKeys);
    oracle(ArrayRecordMap::<
        _,
        _,
        BuildHasherDefault<Zero>,
        70,
        16,
        CopyKeys,
    >::default());
    oracle(ArrayRecordMap::<
        _,
        _,
        BuildHasherDefault<Zero>,
        70,
        16,
        SharedKeys,
    >::default());
}
#[test]
fn large_array_pages_grow_without_changing_retained_records() {
    fn check<KP: KeyPolicy<String>>() {
        let mut map = ArrayRecordMap::<
            String,
            Arc<Vec<u8>>,
            std::collections::hash_map::RandomState,
            70,
            1024,
            KP,
        >::default();
        let mut model = BTreeMap::new();
        let mut old = Vec::new();
        for step in 0..7 {
            for id in step * 1000..(step + 1) * 1000 {
                let k = format!("large {id}");
                let v = Arc::new(vec![(id % 251) as u8; 17]);
                map.insert(k.clone(), v.clone());
                model.insert(k, v);
            }
            assert_eq!(map.contents(), model);
            old.push((map.clone(), model.clone()));
        }
        for id in (0..7000).step_by(3) {
            let k = format!("large {id}");
            map.remove(k.as_str());
            model.remove(&k);
        }
        assert_eq!(map.contents(), model);
        for (view, expected) in old {
            assert_eq!(view.contents(), expected);
        }
    }
    check::<CopyKeys>();
    check::<SharedKeys>();
}
#[test]
fn array_key_policies_preserve_identity_with_distinct_copy_costs() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Key {
        id: u64,
        label: &'static str,
        clones: Arc<AtomicUsize>,
    }
    impl Clone for Key {
        fn clone(&self) -> Self {
            self.clones.fetch_add(1, Ordering::Relaxed);
            Self {
                id: self.id,
                label: self.label,
                clones: self.clones.clone(),
            }
        }
    }
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
    impl Eq for Key {}
    impl std::hash::Hash for Key {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.id.hash(state);
        }
    }
    fn check<KP: KeyPolicy<Key>>(expected: usize) {
        let count = Arc::new(AtomicUsize::new(0));
        let mut map=ArrayRecordMap::<Key,u64,std::collections::hash_map::RandomState,70,16,KP>::default();
        map.insert(
            Key {
                id: 1,
                label: "canonical",
                clones: count.clone(),
            },
            0,
        );
        let old = map.clone();
        for epoch in 1..=100 {
            map.insert(
                Key {
                    id: 1,
                    label: "replacement",
                    clones: count.clone(),
                },
                epoch,
            );
        }
        map.apply(&[Mutation::Put(
            Key {
                id: 1,
                label: "batch",
                clones: count.clone(),
            },
            101,
        )]);
        let (k, v) = map.iter().next().unwrap();
        assert_eq!(k.label, "canonical");
        assert_eq!(*v, 101);
        let (k, v) = old.iter().next().unwrap();
        assert_eq!(k.label, "canonical");
        assert_eq!(*v, 0);
        map.remove(&Key {
            id: 1,
            label: "query",
            clones: count.clone(),
        });
        map.insert(
            Key {
                id: 1,
                label: "reinserted",
                clones: count.clone(),
            },
            102,
        );
        assert_eq!(map.iter().next().unwrap().0.label, "reinserted");
        assert_eq!(old.iter().next().unwrap().0.label, "canonical");
        assert_eq!(count.load(Ordering::Relaxed), expected);
    }
    check::<CopyKeys>(101);
    check::<SharedKeys>(0);
}
#[test]
fn array_records_release_after_final_snapshot_and_returned_handle() {
    fn check<KP: KeyPolicy<String>>() {
        let data = Arc::new(vec![3u8; 4096]);
        let weak = Arc::downgrade(&data);
        let mut map = ArrayRecordMap::<
            String,
            _,
            std::collections::hash_map::RandomState,
            70,
            16,
            KP,
        >::default();
        map.insert("key".into(), data);
        let old = map.clone();
        let removed = map.remove("key").unwrap();
        drop(old);
        assert!(weak.upgrade().is_some());
        drop(removed);
        assert!(weak.upgrade().is_none());
    }
    check::<CopyKeys>();
    check::<SharedKeys>();
}
#[test]
fn array_page_parameters_are_checked_before_allocation() {
    assert!(std::panic::catch_unwind(
        ArrayRecordMap::<u64, u64, std::collections::hash_map::RandomState, 70, 0>::default
    )
    .is_err());
    assert!(std::panic::catch_unwind(
        ArrayRecordMap::<u64, u64, std::collections::hash_map::RandomState, 70, 3>::default
    )
    .is_err());
    assert!(std::panic::catch_unwind(
        ArrayRecordMap::<u64, u64, std::collections::hash_map::RandomState, 70, 8192>::default
    )
    .is_err());
    assert!(std::panic::catch_unwind(
        ArrayRecordMap::<u64, u64, std::collections::hash_map::RandomState, 91, 16>::default
    )
    .is_err());
}
impl<S: std::hash::BuildHasher, const L: usize, HP: HasherPolicy<S>, const SPAN: bool> Subject
    for InlineSliceMap<String, Arc<Vec<u8>>, S, L, HP, SPAN>
{
    fn put(&mut self, k: String, v: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        self.insert(k, v)
    }
    fn del(&mut self, k: &str) -> Option<Arc<Vec<u8>>> {
        self.remove(k)
    }
    fn read(&self, k: &str) -> Option<&Arc<Vec<u8>>> {
        self.get(k)
    }
    fn contents(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn batch(&mut self, ops: Vec<Mutation<String, Arc<Vec<u8>>>>) {
        self.apply(&ops);
    }
}

#[test]
fn inline_slices_preserve_collision_growth_deletion_and_borrowed_keys() {
    macro_rules! load {
        ($l:literal) => {
            oracle(InlineSliceMap::<
                _,
                _,
                BuildHasherDefault<Wrap>,
                $l,
                SharedHasher,
                true,
            >::default());
            oracle(InlineSliceMap::<
                _,
                _,
                BuildHasherDefault<Wrap>,
                $l,
                SharedHasher,
                false,
            >::default());
            oracle(InlineSliceMap::<
                _,
                _,
                BuildHasherDefault<Wrap>,
                $l,
                OwnedHasher,
                true,
            >::default());
            oracle(InlineSliceMap::<
                _,
                _,
                BuildHasherDefault<Wrap>,
                $l,
                OwnedHasher,
                false,
            >::default());
        };
    }
    load!(25);
    load!(50);
    load!(70);
    load!(85);
    oracle(InlineSliceMap::<
        _,
        _,
        BuildHasherDefault<Zero>,
        70,
        SharedHasher,
    >::default());
    oracle(InlineSliceMap::<
        _,
        _,
        BuildHasherDefault<Zero>,
        70,
        OwnedHasher,
        false,
    >::default());
    oracle(InlineSliceMap::<String, Arc<Vec<u8>>>::default());
}

#[test]
fn inline_hash_normalization_never_confuses_zero_and_max_keys() {
    #[derive(Clone, Default)]
    struct Identity(u64);
    impl Hasher for Identity {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0 = u64::from_ne_bytes(bytes.try_into().unwrap());
        }
        fn write_u64(&mut self, n: u64) {
            self.0 = n;
        }
    }
    fn check<HP: HasherPolicy<BuildHasherDefault<Identity>>, const SPAN: bool>() {
        let mut map =
            InlineSliceMap::<u64, u64, BuildHasherDefault<Identity>, 70, HP, SPAN>::default();
        for key in [0, u64::MAX, 15, 31, 47, 63, 79] {
            map.insert(key, key ^ 111);
        }
        let old = map.clone();
        for key in [0, u64::MAX, 31, 63] {
            assert_eq!(map.remove(&key), Some(key ^ 111));
            assert_eq!(map.get(&key), None);
            for retained in [15, 47, 79] {
                assert_eq!(map.get(&retained), Some(&(retained ^ 111)));
            }
            map.insert(key, key ^ 222);
        }
        for key in [0, u64::MAX, 15, 31, 47, 63, 79] {
            assert_eq!(old.get(&key), Some(&(key ^ 111)));
        }
        assert_eq!(map.len(), 7);
    }
    check::<SharedHasher, true>();
    check::<SharedHasher, false>();
    check::<OwnedHasher, true>();
    check::<OwnedHasher, false>();
}

#[test]
fn inline_canonical_keys_survive_replacement_then_change_after_reinsertion() {
    #[derive(Clone)]
    struct Key(u64, &'static str);
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Eq for Key {}
    impl std::hash::Hash for Key {
        fn hash<H: Hasher>(&self, h: &mut H) {
            self.0.hash(h);
        }
    }
    let mut map = InlineSliceMap::<Key, u64>::default();
    map.insert(Key(1, "canonical"), 10);
    let old = map.clone();
    map.insert(Key(1, "replacement"), 20);
    map.apply(&[Mutation::Put(Key(1, "batch replacement"), 30)]);
    assert_eq!(map.iter().next().unwrap().0 .1, "canonical");
    assert_eq!(map.remove(&Key(1, "query")), Some(30));
    map.insert(Key(1, "fresh"), 40);
    assert_eq!(map.iter().next().unwrap().0 .1, "fresh");
    assert_eq!(old.iter().next().unwrap().0 .1, "canonical");
    assert_eq!(old.iter().next().unwrap().1, &10);
}

#[test]
fn inline_growth_and_removal_release_only_unretained_payloads() {
    let payload = Arc::new(vec![7u8; 4096]);
    let weak = Arc::downgrade(&payload);
    let mut map = InlineSliceMap::<String, Arc<Vec<u8>>>::default();
    map.insert("pinned".into(), payload);
    let old = map.clone();
    let operations: Vec<_> = (0..7000)
        .map(|i| Mutation::Put(format!("k/{i}"), Arc::new(vec![i as u8; 17])))
        .collect();
    map.apply(&operations);
    assert_eq!(map.len(), 7001);
    assert_eq!(old.len(), 1);
    let removed = map.remove("pinned").unwrap();
    drop(old);
    assert!(weak.upgrade().is_some());
    drop(removed);
    assert!(weak.upgrade().is_none());
    for i in 0..7000 {
        assert_eq!(
            map.get(format!("k/{i}").as_str()).unwrap().as_slice(),
            &[i as u8; 17]
        );
    }
}

#[test]
fn inline_growth_of_a_shared_full_table_preserves_both_complete_views() {
    let mut map =
        InlineSliceMap::<String, Vec<u8>, std::collections::hash_map::RandomState, 50>::default();
    for i in 0..8 {
        map.insert(format!("k/{i}"), vec![i as u8; 257]);
    }
    let old = map.clone();
    map.insert("k/8".into(), vec![8; 257]);
    assert_eq!(map.len(), 9);
    assert_eq!(old.len(), 8);
    for i in 0..8 {
        assert_eq!(
            old.get(format!("k/{i}").as_str()),
            Some(&vec![i as u8; 257])
        );
        assert_eq!(
            map.get(format!("k/{i}").as_str()),
            Some(&vec![i as u8; 257])
        );
    }
    assert!(old.get("k/8").is_none());
    assert_eq!(map.get("k/8"), Some(&vec![8; 257]));
}

#[test]
fn inline_failed_detachment_keeps_the_published_view() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Value {
        data: u64,
        fail: Arc<AtomicBool>,
    }
    impl Clone for Value {
        fn clone(&self) -> Self {
            assert!(!self.fail.load(Ordering::Relaxed), "injected clone panic");
            Self {
                data: self.data,
                fail: self.fail.clone(),
            }
        }
    }
    let fail = Arc::new(AtomicBool::new(false));
    let mut published = InlineSliceMap::<u64, Value>::default();
    published.insert(
        1,
        Value {
            data: 17,
            fail: fail.clone(),
        },
    );
    let mut staged = published.clone();
    fail.store(true, Ordering::Relaxed);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        staged.insert(
            2,
            Value {
                data: 19,
                fail: fail.clone(),
            },
        );
    }));
    assert!(outcome.is_err());
    assert_eq!(published.len(), 1);
    assert_eq!(published.get(&1).unwrap().data, 17);
    assert!(published.get(&2).is_none());
    assert_eq!(staged.len(), 1);
    assert_eq!(staged.get(&1).unwrap().data, 17);
}

#[test]
fn inline_shared_builder_supports_a_nonclone_hasher() {
    struct NotClone;
    impl std::hash::BuildHasher for NotClone {
        type Hasher = Wrap;
        fn build_hasher(&self) -> Wrap {
            Wrap
        }
    }
    let mut map = InlineSliceMap::<u64, u64, _, 50>::with_hasher(NotClone);
    map.insert(1, 10);
    let old = map.clone();
    map.insert(1, 20);
    assert_eq!(old.get(&1), Some(&10));
    assert_eq!(map.get(&1), Some(&20));
    assert!(std::panic::catch_unwind(
        InlineSliceMap::<u64, u64, std::collections::hash_map::RandomState, 19>::default
    )
    .is_err());
    assert!(std::panic::catch_unwind(
        InlineSliceMap::<u64, u64, std::collections::hash_map::RandomState, 91>::default
    )
    .is_err());
}

fn oracle<M: Subject>(mut m: M) {
    let mut truth = BTreeMap::new();
    let mut seed = 0xcafec001u64;
    let mut pinned = Vec::new();
    for i in 0..4000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let k = format!("key / {} / unicode λ", seed % 257);
        if i % 7 == 0 {
            let mut ops = Vec::new();
            for j in 0..10 {
                let key = format!("key / {} / unicode λ", (seed.wrapping_add(j / 2)) % 257);
                if j % 3 == 0 {
                    truth.remove(&key);
                    ops.push(Mutation::Delete(key));
                } else {
                    let value = Arc::new(vec![(i % 256) as u8; j as usize + 1]);
                    truth.insert(key.clone(), value.clone());
                    ops.push(Mutation::Put(key, value));
                }
            }
            m.batch(ops);
        }
        match seed % 5 {
            0 | 1 => assert_eq!(m.del(&k), truth.remove(&k)),
            2 | 3 => {
                let v = Arc::new(vec![(i % 256) as u8; (i % 64) + 1]);
                assert_eq!(m.put(k.clone(), v.clone()), truth.insert(k.clone(), v));
            }
            _ => assert_eq!(m.read(&k), truth.get(&k)),
        }
        assert_eq!(m.len(), truth.len());
        if i % 73 == 0 {
            assert_eq!(m.contents(), truth);
            for (old, expected) in &pinned {
                assert_eq!(Subject::contents(old), *expected);
            }
        }
        if i % 1000 == 0 {
            pinned.push((m.clone(), truth.clone()));
        }
    }
    assert_eq!(m.contents(), truth);
    for k in truth.keys() {
        assert_eq!(m.del(k), truth.get(k).cloned());
    }
    assert_eq!(m.len(), 0);
    assert!(m.contents().is_empty());
    for (old, expected) in pinned {
        assert_eq!(old.contents(), expected);
    }
}
#[test]
fn generic_strings_heap_values_and_borrowed_keys() {
    oracle(ProbeMap::<
        _,
        _,
        std::collections::hash_map::RandomState,
        70,
        0,
    >::default());
    oracle(ProbeMap::<
        _,
        _,
        std::collections::hash_map::RandomState,
        70,
        64,
    >::default());
    oracle(SplitMap::<_, _, std::collections::hash_map::RandomState, 70>::default());
}
#[test]
fn collisions_wraparound_deletion_growth_and_retained_views() {
    macro_rules! cases {
        ($l:literal) => {
            oracle(ProbeMap::<_, _, BuildHasherDefault<Wrap>, $l, 0>::default());
            oracle(ProbeMap::<_, _, BuildHasherDefault<Wrap>, $l, 16>::default());
            oracle(ProbeMap::<_, _, BuildHasherDefault<Wrap>, $l, 64>::default());
            oracle(ProbeMap::<_, _, BuildHasherDefault<Wrap>, $l, 256>::default());
            oracle(SplitMap::<_, _, BuildHasherDefault<Wrap>, $l>::default());
        };
    }
    cases!(20);
    cases!(50);
    cases!(70);
    cases!(90);
    oracle(SplitMap::<_, _, BuildHasherDefault<Zero>, 85>::default());
}
#[test]
fn retained_pages_release_values_after_the_last_owner() {
    let v = Arc::new(vec![0u8; 4096]);
    let weak = Arc::downgrade(&v);
    let mut m = ProbeMap::<String, _, std::collections::hash_map::RandomState, 70, 16>::default();
    m.insert("first".into(), v);
    let old = m.clone();
    m.remove("first");
    assert!(weak.upgrade().is_some());
    drop(old);
    assert!(weak.upgrade().is_none());
}
