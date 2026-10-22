use group_byte_index_proof::GroupByteMap;
use std::{
    borrow::Borrow,
    collections::BTreeMap,
    hash::{BuildHasherDefault, Hash, Hasher},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

#[derive(Clone, Debug)]
struct Key {
    id: u64,
    metadata: u64,
}
impl PartialEq for Key {
    fn eq(&self, rhs: &Self) -> bool {
        self.id == rhs.id
    }
}
impl Eq for Key {}
impl Hash for Key {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h)
    }
}
impl Borrow<u64> for Key {
    fn borrow(&self) -> &u64 {
        &self.id
    }
}
#[derive(Default)]
struct Collision;
impl Hasher for Collision {
    fn finish(&self) -> u64 {
        u64::MAX
    }
    fn write(&mut self, _: &[u8]) {}
}
#[derive(Default)]
struct Identity(u64);
impl Hasher for Identity {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(byte));
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}
#[test]
fn adjacent_fingerprint_false_candidates_require_complete_key_equality() {
    let mut map = GroupByteMap::<u64, u64, BuildHasherDefault<Identity>>::default();
    map.insert(0, 17);
    map.insert(1 << 57, 19);
    // Both stored rows can be returned by a word mask searching for tag zero.
    // Neither equals this absent key with the same tag and group home.
    assert_eq!(map.get(&2), None);
    assert_eq!(map.remove(&2), None);
    assert_eq!(map.get(&0), Some(&17));
    assert_eq!(map.get(&(1 << 57)), Some(&19));
    for tag in 2..128 {
        map.insert(tag << 57, tag);
    }
    for tag in 2..128 {
        assert_eq!(map.get(&(tag << 57)), Some(&tag));
    }
    for tag in (0..128).step_by(2) {
        map.remove(&(tag << 57));
    }
    for tag in (3..128).step_by(2) {
        assert_eq!(map.get(&(tag << 57)), Some(&tag));
    }
    assert_eq!(map.get(&2), None);
}
fn check<const L: usize, const G: usize>() {
    let mut map = GroupByteMap::<Key, Vec<u8>, BuildHasherDefault<Collision>, L, G>::default();
    let mut truth = BTreeMap::<u64, (u64, Vec<u8>)>::new();
    type Model = BTreeMap<u64, (u64, Vec<u8>)>;
    type Map<const L: usize, const G: usize> =
        GroupByteMap<Key, Vec<u8>, BuildHasherDefault<Collision>, L, G>;
    let mut old: Vec<(Map<L, G>, Model)> = Vec::new();
    let mut seed = 0x51a7e_u64;
    for step in 0..3000u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let id = seed % 128;
        if step % 4 == 0 {
            assert_eq!(map.remove(&id), truth.remove(&id).map(|(_, v)| v));
            assert!(map.remove(&id).is_none());
        } else {
            let key = Key { id, metadata: step };
            let value = vec![(step % 256) as u8; (step % 257) as usize];
            let before = truth.get(&id).map(|(_, v)| v.clone());
            match truth.get_mut(&id) {
                Some((_, v)) => *v = value.clone(),
                None => {
                    truth.insert(id, (step, value.clone()));
                }
            }
            assert_eq!(map.insert(key, value), before);
        }
        if step % 53 == 0 {
            assert_eq!(map.len(), truth.len());
            let mut found = BTreeMap::new();
            for (k, v) in map.iter() {
                assert!(found.insert(k.id, (k.metadata, v.clone())).is_none());
            }
            assert_eq!(found, truth);
            for id in 0..128 {
                assert_eq!(map.get(&id), truth.get(&id).map(|(_, v)| v));
            }
            for (snapshot, expected) in &old {
                let observed: BTreeMap<_, _> = snapshot
                    .iter()
                    .map(|(k, v): (&Key, &Vec<u8>)| (k.id, (k.metadata, v.clone())))
                    .collect();
                assert_eq!(&observed, expected);
            }
            if old.len() < 5 {
                old.push((map.clone(), truth.clone()));
            }
        }
    }
    for id in 0..128 {
        map.remove(&id);
    }
    assert!(map.is_empty());
    for id in 0..128 {
        assert!(map.get(&id).is_none());
        map.insert(Key { id, metadata: 9000 }, vec![17; 257]);
    }
    assert_eq!(map.len(), 128);
    for id in 0..128 {
        assert_eq!(map.get(&id), Some(&vec![17; 257]));
    }
}
#[test]
fn all_registered_policies_match_full_collision_model_and_retained_canonical_keys() {
    macro_rules! groups {
        ($l:literal) => {
            check::<$l, 8>();
            check::<$l, 16>();
            check::<$l, 32>();
        };
    }
    groups!(25);
    groups!(50);
    groups!(70);
    groups!(85);
}
#[test]
fn borrowed_strings_and_absent_deletes() {
    let mut m = GroupByteMap::<String, String>::default();
    m.insert("key".into(), "old".into());
    let old = m.clone();
    m.insert("key".into(), "new".into());
    assert_eq!(old.get("key").map(String::as_str), Some("old"));
    assert_eq!(m.remove("key").as_deref(), Some("new"));
    assert!(m.remove("key").is_none());
}

struct Tracked {
    id: u64,
    payload: Vec<u8>,
    live: Arc<AtomicUsize>,
    fail: Arc<AtomicBool>,
}
impl Tracked {
    fn new(id: u64, live: &Arc<AtomicUsize>, fail: &Arc<AtomicBool>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self {
            id,
            payload: vec![id as u8; 257],
            live: live.clone(),
            fail: fail.clone(),
        }
    }
}
impl Clone for Tracked {
    fn clone(&self) -> Self {
        assert!(
            !(self.id == 3 && self.fail.load(Ordering::SeqCst)),
            "injected value clone"
        );
        let payload = self.payload.clone();
        self.live.fetch_add(1, Ordering::SeqCst);
        Self {
            id: self.id,
            payload,
            live: self.live.clone(),
            fail: self.fail.clone(),
        }
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}
#[test]
fn failed_growth_clone_preserves_both_roots_and_reclaims_partial_values() {
    let live = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    {
        let mut m = GroupByteMap::<u64, Tracked>::default();
        for id in 0..11 {
            m.insert(id, Tracked::new(id, &live, &fail));
        }
        assert_eq!(m.capacity(), 16);
        let old = m.clone();
        fail.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| {
            m.insert(99, Tracked::new(99, &live, &fail));
        }))
        .is_err());
        fail.store(false, Ordering::SeqCst);
        assert_eq!(live.load(Ordering::SeqCst), 11);
        assert_eq!(m.len(), 11);
        assert_eq!(m.capacity(), 16);
        for id in 0..11 {
            assert_eq!(m.get(&id).unwrap().payload, vec![id as u8; 257]);
            assert_eq!(old.get(&id).unwrap().id, id);
        }
        m.insert(99, Tracked::new(99, &live, &fail));
        assert_eq!(m.capacity(), 32);
        assert_eq!(old.capacity(), 16);
        assert!(old.get(&99).is_none());
        drop(m);
        assert_eq!(live.load(Ordering::SeqCst), 11);
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[derive(Clone)]
struct HashKey {
    id: u64,
    fail: Arc<AtomicBool>,
}
impl PartialEq for HashKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for HashKey {}
impl Hash for HashKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        assert!(
            !(self.id == 3 && self.fail.load(Ordering::SeqCst)),
            "injected reconstruction hash"
        );
        self.id.hash(h);
    }
}
impl Borrow<u64> for HashKey {
    fn borrow(&self) -> &u64 {
        &self.id
    }
}
#[test]
fn failed_reconstruction_hash_preserves_roots_and_retry_grows() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut m = GroupByteMap::<HashKey, u64>::default();
    for id in 0..11 {
        m.insert(
            HashKey {
                id,
                fail: fail.clone(),
            },
            id * 17,
        );
    }
    let old = m.clone();
    fail.store(true, Ordering::SeqCst);
    assert!(catch_unwind(AssertUnwindSafe(|| m.insert(
        HashKey {
            id: 99,
            fail: fail.clone()
        },
        99
    )))
    .is_err());
    fail.store(false, Ordering::SeqCst);
    assert_eq!(m.capacity(), 16);
    assert_eq!(m.len(), 11);
    for id in 0..11 {
        assert_eq!(m.get(&id), Some(&(id * 17)));
        assert_eq!(old.get(&id), Some(&(id * 17)));
    }
    m.insert(
        HashKey {
            id: 99,
            fail: fail.clone(),
        },
        99,
    );
    assert_eq!(m.capacity(), 32);
    assert_eq!(old.capacity(), 16);
    assert!(old.get(&99).is_none());
}
fn threaded<const L: usize, const G: usize>() {
    let mut map =
        GroupByteMap::<u64, [u64; 4], std::collections::hash_map::RandomState, L, G>::default();
    for id in 0..20_000u64 {
        map.insert(id, [id, 0, !id, id * 17]);
    }
    let old = map.clone();
    let barrier = std::sync::Barrier::new(5);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let old = &old;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..4 {
                    for id in 0..20_000u64 {
                        assert_eq!(old.get(&id), Some(&[id, 0, !id, id * 17]));
                    }
                }
            });
        }
        barrier.wait();
        for id in 0..20_000u64 {
            map.insert(id, [id, 1, !id, id * 17]);
        }
        for id in (0..20_000u64).step_by(3) {
            map.remove(&id);
        }
    });
    for id in 0..20_000u64 {
        let expected = [id, 1, !id, id * 17];
        assert_eq!(old.get(&id), Some(&[id, 0, !id, id * 17]));
        assert_eq!(
            map.get(&id),
            if id % 3 == 0 { None } else { Some(&expected) }
        );
    }
}

#[test]
fn all_registered_policies_keep_twenty_thousand_records_on_four_actual_reader_threads() {
    macro_rules! groups {
        ($l:literal) => {
            threaded::<$l, 8>();
            threaded::<$l, 16>();
            threaded::<$l, 32>();
        };
    }
    groups!(25);
    groups!(50);
    groups!(70);
    groups!(85);
}
#[test]
fn tombstone_pressure_rebuilds_at_same_capacity_and_preserves_old_root() {
    let mut map = GroupByteMap::<u64, u64>::default();
    for id in 0..44 {
        map.insert(id, id * 17);
    }
    assert_eq!(map.capacity(), 64);
    for id in 0..40 {
        map.remove(&id);
    }
    assert_eq!(map.deleted_slots(), 40);
    let old = map.clone();
    map.insert(1000, 17);
    assert_eq!(map.capacity(), 64);
    assert_eq!(map.deleted_slots(), 0);
    assert_eq!(old.deleted_slots(), 40);
    assert!(old.get(&1000).is_none());
    for id in 40..44 {
        assert_eq!(map.get(&id), Some(&(id * 17)));
        assert_eq!(old.get(&id), Some(&(id * 17)));
    }
}
#[derive(Debug)]
struct PanicKey {
    id: u64,
    fail_clone: Arc<AtomicBool>,
    fail_drop: Arc<AtomicBool>,
}
impl Clone for PanicKey {
    fn clone(&self) -> Self {
        assert!(
            !(self.id == 3 && self.fail_clone.load(Ordering::SeqCst)),
            "injected key clone"
        );
        Self {
            id: self.id,
            fail_clone: self.fail_clone.clone(),
            fail_drop: self.fail_drop.clone(),
        }
    }
}
impl Drop for PanicKey {
    fn drop(&mut self) {
        assert!(
            !(self.id == 3 && self.fail_drop.swap(false, Ordering::SeqCst)),
            "injected key drop"
        );
    }
}
impl PartialEq for PanicKey {
    fn eq(&self, rhs: &Self) -> bool {
        self.id == rhs.id
    }
}
impl Eq for PanicKey {}
impl Hash for PanicKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h)
    }
}
impl Borrow<u64> for PanicKey {
    fn borrow(&self) -> &u64 {
        &self.id
    }
}
#[test]
fn key_clone_failure_reclaims_partial_cow_and_remove_drop_failure_leaves_usable_map() {
    let live = Arc::new(AtomicUsize::new(0));
    let no_value_failure = Arc::new(AtomicBool::new(false));
    let fail_clone = Arc::new(AtomicBool::new(false));
    let fail_drop = Arc::new(AtomicBool::new(false));
    let key = |id| PanicKey {
        id,
        fail_clone: fail_clone.clone(),
        fail_drop: fail_drop.clone(),
    };
    {
        let mut map = GroupByteMap::<PanicKey, Tracked>::default();
        for id in 0..11 {
            map.insert(key(id), Tracked::new(id, &live, &no_value_failure));
        }
        let old = map.clone();
        fail_clone.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(
            || map.insert(key(99), Tracked::new(99, &live, &no_value_failure))
        ))
        .is_err());
        fail_clone.store(false, Ordering::SeqCst);
        assert_eq!(live.load(Ordering::SeqCst), 11);
        for id in 0..11 {
            assert_eq!(map.get(&id).unwrap().id, id);
            assert_eq!(old.get(&id).unwrap().id, id);
        }
        fail_drop.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| map.remove(&3u64))).is_err());
        assert_eq!(map.len(), 10);
        assert!(map.get(&3u64).is_none());
        assert_eq!(old.get(&3u64).unwrap().id, 3);
        assert_eq!(map.iter().count(), 10);
        map.insert(key(99), Tracked::new(99, &live, &no_value_failure));
        assert_eq!(map.len(), 11);
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
