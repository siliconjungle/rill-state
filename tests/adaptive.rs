#![cfg(feature = "experimental")]
use rill_state::{
    experimental::{AdaptiveMap, FlatMode, InlineFlat, ReuseSharedFlat, SharedFlat},
    Mutation,
};
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
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Key {}
impl Hash for Key {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h);
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
type Builder = BuildHasherDefault<Collision>;
type Model = BTreeMap<u64, (u64, Vec<u8>)>;
fn contents<
    const T: usize,
    const D: usize,
    const B: bool,
    const P: usize,
    F: FlatMode<Key, Vec<u8>, Builder>,
    const R: bool,
>(
    map: &AdaptiveMap<Key, Vec<u8>, Builder, T, D, B, P, F, R>,
) -> Model {
    let mut out = Model::new();
    map.visit(|k, v| {
        assert!(out.insert(k.id, (k.metadata, v.clone())).is_none());
        assert!(std::ptr::eq(v, map.get(&k.id).unwrap()));
    });
    assert_eq!(out.len(), map.len());
    out
}
fn policy<
    const T: usize,
    const D: usize,
    const B: bool,
    const P: usize,
    F: FlatMode<Key, Vec<u8>, Builder>,
    const R: bool,
>() {
    type Map<const T: usize, const D: usize, const B: bool, const P: usize, F, const R: bool> =
        AdaptiveMap<Key, Vec<u8>, Builder, T, D, B, P, F, R>;
    let mut map = Map::<T, D, B, P, F, R>::default();
    let initial: Vec<_> = (0..128u64)
        .map(|id| {
            Mutation::Put(
                Key {
                    id,
                    metadata: id + 1000,
                },
                vec![id as u8; 257],
            )
        })
        .collect();
    let mut model: Model = (0..128u64)
        .map(|id| (id, (id + 1000, vec![id as u8; 257])))
        .collect();
    map.apply(&initial);
    assert!(!map.is_paged());
    let old = map.clone();
    for step in 1..=3 {
        map.apply(&[Mutation::Put(
            Key {
                id: 0,
                metadata: 9999,
            },
            vec![step; 257],
        )]);
        model.get_mut(&0).unwrap().1 = vec![step; 257];
        assert_eq!(map.is_paged(), step as usize >= D);
    }
    assert_eq!(contents(&map), model);
    assert_eq!(contents(&old)[&0], (1000, vec![0; 257]));
    let initial_model = (0..128u64)
        .map(|id| (id, (id + 1000, vec![id as u8; 257])))
        .collect();
    let mut snapshots = vec![(old, initial_model)];
    let mut seed = 0x51a7e_u64;
    for step in 0..1000u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let id = seed % 192;
        let ops = if step % 179 == 0 {
            (0..192)
                .map(|id| {
                    let data = vec![step as u8; (step % 257) as usize];
                    model
                        .entry(id)
                        .and_modify(|(_, v)| *v = data.clone())
                        .or_insert((7000 + step + id, data.clone()));
                    Mutation::Put(
                        Key {
                            id,
                            metadata: 7000 + step + id,
                        },
                        data,
                    )
                })
                .collect()
        } else if step % 4 == 0 {
            model.remove(&id);
            vec![
                Mutation::Delete(Key { id, metadata: 0 }),
                Mutation::Delete(Key { id, metadata: 1 }),
            ]
        } else {
            let data = vec![(step % 256) as u8; (step % 257) as usize];
            model
                .entry(id)
                .and_modify(|(_, v)| *v = data.clone())
                .or_insert((step, data.clone()));
            vec![Mutation::Put(Key { id, metadata: step }, data)]
        };
        map.apply(&ops);
        if step % 67 == 0 {
            assert_eq!(contents(&map), model);
            for id in 0..192 {
                assert_eq!(map.get(&id), model.get(&id).map(|(_, v)| v));
            }
            for (view, expected) in &snapshots {
                assert_eq!(&contents(view), expected);
            }
            if snapshots.len() < 5 {
                snapshots.push((map.clone(), model.clone()));
            }
        }
    }
    assert_eq!(contents(&map), model);
}
#[test]
fn all_two_hundred_forty_policies_match_collisions_canonical_keys_and_retained_values() {
    macro_rules! mode {
        ($t:literal,$d:literal,$b:literal,$p:literal) => {
            policy::<$t, $d, $b, $p, InlineFlat, false>();
            policy::<$t, $d, $b, $p, InlineFlat, true>();
            policy::<$t, $d, $b, $p, SharedFlat<0>, false>();
            policy::<$t, $d, $b, $p, SharedFlat<0>, true>();
            policy::<$t, $d, $b, $p, SharedFlat<2>, false>();
            policy::<$t, $d, $b, $p, SharedFlat<2>, true>();
            policy::<$t, $d, $b, $p, ReuseSharedFlat<0>, false>();
            policy::<$t, $d, $b, $p, ReuseSharedFlat<0>, true>();
            policy::<$t, $d, $b, $p, ReuseSharedFlat<2>, false>();
            policy::<$t, $d, $b, $p, ReuseSharedFlat<2>, true>();
        };
    }
    macro_rules! pages {
        ($t:literal,$d:literal,$b:literal) => {
            mode!($t, $d, $b, 16);
            mode!($t, $d, $b, 64);
        };
    }
    macro_rules! gate {
        ($t:literal,$d:literal) => {
            pages!($t, $d, false);
            pages!($t, $d, true);
        };
    }
    macro_rules! delay {
        ($t:literal) => {
            gate!($t, 1);
            gate!($t, 3);
        };
    }
    delay!(1);
    delay!(10);
    delay!(100);
}
#[test]
fn submitted_batch_density_type_gate_and_empty_batches_are_case_blind() {
    type Map = AdaptiveMap<u64, u64, Builder, 10, 3, false, 16>;
    let mut map = Map::default();
    map.apply(&(0..100).map(|id| Mutation::Put(id, id)).collect::<Vec<_>>());
    map.apply(&[Mutation::Put(0, 1)]);
    map.apply(&[]);
    assert_eq!(map.qualifying_batches(), 1);
    map.apply(&(0..11).map(|id| Mutation::Put(id, id)).collect::<Vec<_>>());
    assert_eq!(map.qualifying_batches(), 0);
    for _ in 0..2 {
        map.apply(&[Mutation::Put(0, 2)]);
        assert!(!map.is_paged());
    }
    let old = map.clone();
    map.apply(&[Mutation::Put(0, 3)]);
    assert!(map.is_paged());
    assert!(!old.is_paged());
    let mut gated = AdaptiveMap::<u64, u64, Builder, 100, 1, true, 16>::default();
    gated.apply(&[Mutation::Put(0, 0)]);
    for i in 1..10 {
        gated.apply(&[Mutation::Put(0, i)]);
    }
    assert!(!gated.is_paged());
    let mut borrowed =
        AdaptiveMap::<String, Vec<u8>, Builder, 100, 1, true, 64, SharedFlat<2>>::default();
    borrowed.apply(
        &(0..128)
            .map(|id| {
                Mutation::Put(
                    if id == 0 {
                        "key".into()
                    } else {
                        format!("key-{id}")
                    },
                    vec![1; 257],
                )
            })
            .collect::<Vec<_>>(),
    );
    let old = borrowed.clone();
    borrowed.apply(&[Mutation::Put("key".into(), vec![2; 257])]);
    assert!(borrowed.is_paged());
    assert_eq!(borrowed.get("key"), Some(&vec![2; 257]));
    assert_eq!(old.get("key"), Some(&vec![1; 257]));
}

#[test]
fn large_batches_reset_reversible_storage_but_keep_oneway_control_paged() {
    let mut reset =
        AdaptiveMap::<Key, Vec<u8>, Builder, 100, 1, false, 16, SharedFlat<2>, true>::default();
    let mut oneway =
        AdaptiveMap::<Key, Vec<u8>, Builder, 100, 1, false, 16, SharedFlat<2>, false>::default();
    let initial: Vec<_> = (0..128)
        .map(|id| {
            Mutation::Put(
                Key {
                    id,
                    metadata: id + 1000,
                },
                vec![id as u8; 257],
            )
        })
        .collect();
    reset.apply(&initial);
    oneway.apply(&initial);
    let small = [Mutation::Put(
        Key {
            id: 17,
            metadata: 9999,
        },
        vec![99; 257],
    )];
    reset.apply(&small);
    oneway.apply(&small);
    assert!(reset.is_paged() && oneway.is_paged());
    let old = reset.clone();
    reset.visit(|_, _| {});
    reset.visit(|_, _| {});
    assert_eq!(reset.cached_records(), 128);
    let large: Vec<_> = (0..64)
        .map(|id| Mutation::Put(Key { id, metadata: 9999 }, vec![88; 257]))
        .collect();
    reset.apply(&large);
    oneway.apply(&large);
    assert!(!reset.is_paged());
    assert!(oneway.is_paged());
    assert_eq!(reset.cached_records(), 0);
    assert_eq!(contents(&reset), contents(&oneway));
    assert_eq!(reset.get(&17), Some(&vec![88; 257]));
    assert_eq!(old.get(&17), Some(&vec![99; 257]));
    reset.visit(|k, v| {
        assert_eq!(k.metadata, k.id + 1000);
        assert!(std::ptr::eq(v, reset.get(&k.id).unwrap()));
    });
}

fn delta_and_reverse_failures<F: FlatMode<u64, Value, Builder>>() {
    let live = Arc::new(AtomicUsize::new(0));
    let clones = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    {
        let mut current = AdaptiveMap::<u64, Value, Builder, 100, 1, false, 16, F>::default();
        let initial: Vec<_> = (0..128)
            .map(|id| Mutation::Put(id, Value::new(id + 1000, &live, &clones, &fail)))
            .collect();
        current.apply(&initial);
        drop(initial);
        let old = current.clone();
        fail.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut staged = current.clone();
            staged.apply(&[
                Mutation::Put(0, Value::new(999, &live, &clones, &fail)),
                Mutation::Put(1, Value::new(17, &live, &clones, &fail)),
            ]);
            current = staged;
        }))
        .is_err());
        assert!(!current.is_paged());
        assert_eq!(live.load(Ordering::SeqCst), 128);
        for id in 0..128 {
            assert_eq!(current.get(&id).unwrap().id, id + 1000);
        }
        fail.store(false, Ordering::SeqCst);
        let mut staged = current.clone();
        staged.apply(&[
            Mutation::Put(0, Value::new(999, &live, &clones, &fail)),
            Mutation::Put(1, Value::new(17, &live, &clones, &fail)),
        ]);
        current = staged;
        assert!(current.is_paged());
        let paged = current.clone();
        let large: Vec<_> = (0..64)
            .map(|id| Mutation::Put(id, Value::new(id + 2000, &live, &clones, &fail)))
            .collect();
        fail.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut staged = current.clone();
            staged.apply(&large);
            current = staged;
        }))
        .is_err());
        assert!(current.is_paged());
        assert_eq!(current.get(&0).unwrap().id, 999);
        assert_eq!(current.get(&1).unwrap().id, 17);
        fail.store(false, Ordering::SeqCst);
        let mut staged = current.clone();
        staged.apply(&large);
        current = staged;
        drop(large);
        assert!(!current.is_paged());
        assert_eq!(current.get(&0).unwrap().id, 2000);
        assert_eq!(paged.get(&0).unwrap().id, 999);
        assert_eq!(old.get(&0).unwrap().id, 1000);
        for _ in 0..3 {
            current.visit(|k, v| assert!(std::ptr::eq(v, current.get(k).unwrap())));
        }
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[test]
fn partial_delta_and_reverse_conversion_failures_preserve_all_published_states() {
    delta_and_reverse_failures::<InlineFlat>();
    delta_and_reverse_failures::<SharedFlat<0>>();
    delta_and_reverse_failures::<SharedFlat<2>>();
}

struct Value {
    id: u64,
    bytes: Vec<u8>,
    live: Arc<AtomicUsize>,
    clones: Arc<AtomicUsize>,
    fail: Arc<AtomicBool>,
}
impl Value {
    fn new(
        id: u64,
        live: &Arc<AtomicUsize>,
        clones: &Arc<AtomicUsize>,
        fail: &Arc<AtomicBool>,
    ) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self {
            id,
            bytes: vec![id as u8; 257],
            live: live.clone(),
            clones: clones.clone(),
            fail: fail.clone(),
        }
    }
}
impl Clone for Value {
    fn clone(&self) -> Self {
        assert!(
            !(self.id == 17 && self.fail.load(Ordering::SeqCst)),
            "injected value clone"
        );
        let bytes = self.bytes.clone();
        self.live.fetch_add(1, Ordering::SeqCst);
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            id: self.id,
            bytes,
            live: self.live.clone(),
            clones: self.clones.clone(),
            fail: self.fail.clone(),
        }
    }
}
impl Drop for Value {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}
fn caches<F: FlatMode<u64, Value, Builder>>() {
    let live = Arc::new(AtomicUsize::new(0));
    let clones = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    {
        let mut map = AdaptiveMap::<u64, Value, Builder, 100, 1, false, 16, F>::default();
        let initial: Vec<_> = (0..128)
            .map(|id| Mutation::Put(id, Value::new(id, &live, &clones, &fail)))
            .collect();
        map.apply(&initial);
        drop(initial);
        let flat = map.clone();
        let before = clones.load(Ordering::SeqCst);
        fail.store(true, Ordering::SeqCst);
        for _ in 0..4 {
            map.visit(|k, v| {
                assert_eq!(v.id, *k);
                assert!(std::ptr::eq(v, map.get(k).unwrap()));
            });
        }
        assert_eq!(clones.load(Ordering::SeqCst), before);
        assert_eq!(
            map.cached_records(),
            if F::SCAN_AFTER == 0 { 0 } else { 128 }
        );
        fail.store(false, Ordering::SeqCst);
        map.apply(&[Mutation::Put(0, Value::new(999, &live, &clones, &fail))]);
        assert!(map.is_paged());
        assert_eq!(map.cached_records(), 0);
        let before = clones.load(Ordering::SeqCst);
        fail.store(true, Ordering::SeqCst);
        for _ in 0..4 {
            map.visit(|k, v| {
                assert_eq!(v.id, if *k == 0 { 999 } else { *k });
                assert!(std::ptr::eq(v, map.get(k).unwrap()));
            });
        }
        assert_eq!(clones.load(Ordering::SeqCst), before);
        let old = map.clone();
        fail.store(false, Ordering::SeqCst);
        map.apply(&[Mutation::Put(0, Value::new(888, &live, &clones, &fail))]);
        assert_eq!(map.cached_records(), 0);
        assert_eq!(old.get(&0).unwrap().id, 999);
        assert_eq!(flat.get(&0).unwrap().id, 0);
        map.visit(|_, v| assert_eq!(v.bytes, vec![v.id as u8; 257]));
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[test]
fn flat_and_paged_caches_preserve_value_identity_without_cloning_payloads() {
    caches::<InlineFlat>();
    caches::<SharedFlat<0>>();
    caches::<SharedFlat<2>>();
    caches::<ReuseSharedFlat<0>>();
    caches::<ReuseSharedFlat<2>>();
}

fn transfers<F: FlatMode<u64, Value, Builder>>() {
    let live = Arc::new(AtomicUsize::new(0));
    let clones = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    {
        let mut map = AdaptiveMap::<u64, Value, Builder, 100, 1, false, 16, F>::default();
        map.apply(
            &(0..128)
                .map(|id| Mutation::Put(id, Value::new(id, &live, &clones, &fail)))
                .collect::<Vec<_>>(),
        );
        let initial = map.clone();
        fail.store(true, Ordering::SeqCst); // Unchanged payload 17 must never be cloned.
        let before = clones.load(Ordering::SeqCst);
        map.apply(&[Mutation::Put(0, Value::new(999, &live, &clones, &fail))]);
        assert!(map.is_paged());
        assert_eq!(clones.load(Ordering::SeqCst) - before, 1);
        assert!(std::ptr::eq(
            initial.get(&17).unwrap(),
            map.get(&17).unwrap()
        ));
        map.visit(|_, _| {});
        map.visit(|_, _| {});
        let paged = map.clone();
        let large: Vec<_> = (64..128)
            .map(|id| Mutation::Put(id, Value::new(id + 1000, &live, &clones, &fail)))
            .collect();
        let before = clones.load(Ordering::SeqCst);
        map.apply(&large);
        assert!(!map.is_paged());
        assert_eq!(clones.load(Ordering::SeqCst) - before, 64);
        assert!(std::ptr::eq(
            initial.get(&17).unwrap(),
            map.get(&17).unwrap()
        ));
        assert_eq!(map.cached_records(), 0);
        map.visit(|_, _| {});
        map.visit(|_, _| {});
        for id in 0..128 {
            assert_eq!(initial.get(&id).unwrap().id, id);
            assert_eq!(paged.get(&id).unwrap().id, if id == 0 { 999 } else { id });
            assert_eq!(
                map.get(&id).unwrap().id,
                if id == 0 {
                    999
                } else if id >= 64 {
                    id + 1000
                } else {
                    id
                }
            );
        }
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[test]
fn shared_transitions_clone_only_delta_payloads_preserve_instances_and_reclaim_every_owner() {
    transfers::<ReuseSharedFlat<0>>();
    transfers::<ReuseSharedFlat<2>>();
}

fn transfer_delta_failure<F: FlatMode<u64, Value, Builder>>() {
    let live = Arc::new(AtomicUsize::new(0));
    let clones = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    {
        let mut current = AdaptiveMap::<u64, Value, Builder, 100, 1, false, 16, F>::default();
        current.apply(
            &(0..128)
                .map(|id| Mutation::Put(id, Value::new(id, &live, &clones, &fail)))
                .collect::<Vec<_>>(),
        );
        let old = current.clone();
        fail.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut staged = current.clone();
            staged.apply(&[
                Mutation::Put(0, Value::new(999, &live, &clones, &fail)),
                Mutation::Put(1, Value::new(17, &live, &clones, &fail)),
            ]);
            current = staged;
        }))
        .is_err());
        assert!(!current.is_paged());
        assert_eq!(live.load(Ordering::SeqCst), 128);
        for id in 0..128 {
            assert_eq!(current.get(&id).unwrap().id, id);
            assert_eq!(old.get(&id).unwrap().id, id);
        }
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[test]
fn failed_delta_after_shared_transfer_never_installs_partial_publication() {
    transfer_delta_failure::<ReuseSharedFlat<0>>();
    transfer_delta_failure::<ReuseSharedFlat<2>>();
}
#[test]
fn failed_conversion_reclaims_partial_values_and_preserves_published_roots() {
    let live = Arc::new(AtomicUsize::new(0));
    let clones = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    {
        let mut current =
            AdaptiveMap::<u64, Value, Builder, 100, 1, false, 16, SharedFlat<2>>::default();
        let initial: Vec<_> = (0..128)
            .map(|id| Mutation::Put(id, Value::new(id, &live, &clones, &fail)))
            .collect();
        current.apply(&initial);
        drop(initial);
        let old = current.clone();
        fail.store(true, Ordering::SeqCst);
        let failure = catch_unwind(AssertUnwindSafe(|| {
            let mut staged = current.clone();
            staged.apply(&[Mutation::Put(0, Value::new(999, &live, &clones, &fail))]);
            current = staged;
        }));
        assert!(failure.is_err());
        fail.store(false, Ordering::SeqCst);
        assert!(!current.is_paged());
        assert_eq!(live.load(Ordering::SeqCst), 128);
        for id in 0..128 {
            assert_eq!(current.get(&id).unwrap().id, id);
            assert_eq!(old.get(&id).unwrap().bytes, vec![id as u8; 257]);
        }
        let mut staged = current.clone();
        staged.apply(&[Mutation::Put(0, Value::new(999, &live, &clones, &fail))]);
        current = staged;
        assert!(current.is_paged());
        assert!(!old.is_paged());
        assert_eq!(old.get(&0).unwrap().id, 0);
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
fn threaded<F: FlatMode<u64, Vec<u8>, std::collections::hash_map::RandomState> + Send + Sync>()
where
    F::Data: Send + Sync,
{
    let mut map = AdaptiveMap::<u64, Vec<u8>, _, 1, 1, false, 16, F>::default();
    map.apply(
        &(0..20_000)
            .map(|id| Mutation::Put(id, vec![id as u8; 257]))
            .collect::<Vec<_>>(),
    );
    let old = map.clone();
    let barrier = std::sync::Barrier::new(5);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let old = &old;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..3 {
                    old.visit(|k, v| {
                        assert_eq!(v, &vec![*k as u8; 257]);
                        assert!(std::ptr::eq(v, old.get(k).unwrap()));
                    });
                }
            });
        }
        barrier.wait();
        map.apply(&[Mutation::Put(17, vec![99; 257])]);
        assert!(map.is_paged());
    });
    assert_eq!(old.get(&17), Some(&vec![17; 257]));
    assert_eq!(map.get(&17), Some(&vec![99; 257]));
}
#[test]
fn four_actual_readers_keep_twenty_thousand_records_during_conversion() {
    threaded::<InlineFlat>();
    threaded::<SharedFlat<0>>();
    threaded::<SharedFlat<2>>();
    threaded::<ReuseSharedFlat<0>>();
    threaded::<ReuseSharedFlat<2>>();
}

#[derive(Default)]
struct KeyFaults {
    clone: AtomicBool,
    hash: AtomicBool,
    drop: AtomicBool,
}
struct FaultKey {
    id: u64,
    faults: Arc<KeyFaults>,
}
impl Clone for FaultKey {
    fn clone(&self) -> Self {
        assert!(
            !(self.id == 17 && self.faults.clone.load(Ordering::SeqCst)),
            "injected key clone"
        );
        Self {
            id: self.id,
            faults: self.faults.clone(),
        }
    }
}
impl Hash for FaultKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        assert!(
            !(self.id == 17 && self.faults.hash.load(Ordering::SeqCst)),
            "injected conversion hash"
        );
        self.id.hash(h);
    }
}
impl Drop for FaultKey {
    fn drop(&mut self) {
        assert!(
            !(self.id == 17 && self.faults.drop.swap(false, Ordering::SeqCst)),
            "injected canonical key drop"
        );
    }
}
impl PartialEq for FaultKey {
    fn eq(&self, rhs: &Self) -> bool {
        self.id == rhs.id
    }
}
impl Eq for FaultKey {}
impl Borrow<u64> for FaultKey {
    fn borrow(&self) -> &u64 {
        &self.id
    }
}
fn key_failures<F: FlatMode<FaultKey, Value, Builder>>() {
    let live = Arc::new(AtomicUsize::new(0));
    let clones = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicBool::new(false));
    let faults = Arc::new(KeyFaults::default());
    let key = |id| FaultKey {
        id,
        faults: faults.clone(),
    };
    {
        let mut current = AdaptiveMap::<FaultKey, Value, Builder, 100, 1, false, 16, F>::default();
        let initial: Vec<_> = (0..128)
            .map(|id| Mutation::Put(key(id), Value::new(id, &live, &clones, &fail)))
            .collect();
        current.apply(&initial);
        drop(initial);
        let old = current.clone();
        for flag in [&faults.clone, &faults.hash] {
            flag.store(true, Ordering::SeqCst);
            assert!(catch_unwind(AssertUnwindSafe(|| {
                let mut staged = current.clone();
                staged.apply(&[Mutation::Put(
                    key(0),
                    Value::new(999, &live, &clones, &fail),
                )]);
                current = staged;
            }))
            .is_err());
            flag.store(false, Ordering::SeqCst);
            assert!(!current.is_paged());
            assert_eq!(live.load(Ordering::SeqCst), 128);
            for id in 0..128 {
                assert_eq!(current.get(&id).unwrap().id, id);
                assert_eq!(old.get(&id).unwrap().bytes, vec![id as u8; 257]);
            }
        }
        // Read-only traversals neither clone nor hash keys, including cache fill.
        faults.clone.store(true, Ordering::SeqCst);
        faults.hash.store(true, Ordering::SeqCst);
        for _ in 0..3 {
            current.visit(|k, v| assert_eq!(k.id, v.id));
        }
        faults.clone.store(false, Ordering::SeqCst);
        faults.hash.store(false, Ordering::SeqCst);
        faults.drop.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut staged = current.clone();
            staged.apply(&[Mutation::Delete(key(17))]);
            current = staged;
        }))
        .is_err());
        assert!(!current.is_paged());
        assert_eq!(live.load(Ordering::SeqCst), 128);
        for id in 0..128 {
            assert_eq!(current.get(&id).unwrap().id, id);
        }
        let mut staged = current.clone();
        staged.apply(&[Mutation::Delete(key(17))]);
        current = staged;
        assert!(current.is_paged());
        assert_eq!(current.len(), 127);
        assert!(current.get(&17).is_none());
        assert_eq!(old.get(&17).unwrap().id, 17);
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[test]
fn key_clone_hash_and_drop_failures_preserve_published_roots_and_release_staging() {
    key_failures::<InlineFlat>();
    key_failures::<SharedFlat<0>>();
    key_failures::<SharedFlat<2>>();
}
#[derive(Default)]
struct FaultBuilder(Arc<AtomicBool>);
impl Clone for FaultBuilder {
    fn clone(&self) -> Self {
        assert!(!self.0.load(Ordering::SeqCst), "injected builder clone");
        Self(self.0.clone())
    }
}
impl std::hash::BuildHasher for FaultBuilder {
    type Hasher = Collision;
    fn build_hasher(&self) -> Self::Hasher {
        Collision
    }
}
fn builder_failure<const D: usize>() {
    let flag = Arc::new(AtomicBool::new(false));
    let mut map = AdaptiveMap::<u64, Vec<u8>, FaultBuilder, 100, D, false, 16>::with_hasher(
        FaultBuilder(flag.clone()),
    );
    map.apply(
        &(0..128)
            .map(|id| Mutation::Put(id, vec![id as u8; 257]))
            .collect::<Vec<_>>(),
    );
    let old = map.clone();
    flag.store(true, Ordering::SeqCst);
    let snapshot_while_clone_fails = map.clone();
    assert_eq!(snapshot_while_clone_fails.get(&0), Some(&vec![0; 257]));
    assert!(catch_unwind(AssertUnwindSafe(|| {
        let mut staged = map.clone();
        staged.apply(&[Mutation::Put(0, vec![99; 257])]);
        map = staged;
    }))
    .is_err());
    flag.store(false, Ordering::SeqCst);
    assert!(!map.is_paged());
    assert_eq!(map.qualifying_batches(), 0);
    for id in 0..128 {
        assert_eq!(map.get(&id), Some(&vec![id as u8; 257]));
        assert_eq!(old.get(&id), Some(&vec![id as u8; 257]));
    }
}
#[test]
fn builder_clone_failure_preserves_flat_cow_and_conversion_sources() {
    builder_failure::<1>();
    builder_failure::<3>();
}

fn reverse_key_failures<F: FlatMode<FaultKey, Vec<u8>, Builder>>() {
    let faults = Arc::new(KeyFaults::default());
    let key = |id| FaultKey {
        id,
        faults: faults.clone(),
    };
    let mut current = AdaptiveMap::<FaultKey, Vec<u8>, Builder, 100, 1, false, 16, F>::default();
    current.apply(
        &(0..128)
            .map(|id| Mutation::Put(key(id), vec![id as u8; 257]))
            .collect::<Vec<_>>(),
    );
    current.apply(&[Mutation::Put(key(0), vec![99; 257])]);
    assert!(current.is_paged());
    let old = current.clone();
    let large: Vec<_> = (0..64)
        .map(|id| Mutation::Put(key(id), vec![88; 257]))
        .collect();
    for flag in [&faults.clone, &faults.hash] {
        flag.store(true, Ordering::SeqCst);
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut staged = current.clone();
            staged.apply(&large);
            current = staged;
        }))
        .is_err());
        flag.store(false, Ordering::SeqCst);
        assert!(current.is_paged());
        for id in 0..128 {
            let expected = vec![if id == 0 { 99 } else { id as u8 }; 257];
            assert_eq!(current.get(&id), Some(&expected));
            assert_eq!(old.get(&id), Some(&expected));
        }
    }
    let mut staged = current.clone();
    staged.apply(&large);
    current = staged;
    assert!(!current.is_paged());
    assert!(old.is_paged());
    assert_eq!(current.get(&0), Some(&vec![88; 257]));
    assert_eq!(old.get(&0), Some(&vec![99; 257]));
}
#[test]
fn reverse_key_clone_and_hash_failures_leave_published_paged_roots_usable() {
    reverse_key_failures::<InlineFlat>();
    reverse_key_failures::<SharedFlat<0>>();
    reverse_key_failures::<SharedFlat<2>>();
    reverse_key_failures::<ReuseSharedFlat<0>>();
    reverse_key_failures::<ReuseSharedFlat<2>>();
}

fn transfer_hash_failure<F: FlatMode<FaultKey, Vec<u8>, Builder>>() {
    let faults = Arc::new(KeyFaults::default());
    let key = |id| FaultKey {
        id,
        faults: faults.clone(),
    };
    let mut current = AdaptiveMap::<FaultKey, Vec<u8>, Builder, 100, 1, false, 16, F>::default();
    current.apply(
        &(0..128)
            .map(|id| Mutation::Put(key(id), vec![id as u8; 257]))
            .collect::<Vec<_>>(),
    );
    let old = current.clone();
    faults.hash.store(true, Ordering::SeqCst);
    assert!(catch_unwind(AssertUnwindSafe(|| {
        let mut staged = current.clone();
        staged.apply(&[Mutation::Put(key(0), vec![99; 257])]);
        current = staged;
    }))
    .is_err());
    faults.hash.store(false, Ordering::SeqCst);
    assert!(!current.is_paged());
    assert_eq!(current.len(), 128);
    for id in 0..128 {
        assert_eq!(current.get(&id), Some(&vec![id as u8; 257]));
        assert_eq!(old.get(&id), Some(&vec![id as u8; 257]));
    }
    faults.clone.store(true, Ordering::SeqCst);
    let mut staged = current.clone();
    staged.apply(&[Mutation::Put(key(0), vec![99; 257])]);
    current = staged;
    assert!(current.is_paged());
    assert!(std::ptr::eq(
        current.get(&17).unwrap(),
        old.get(&17).unwrap()
    ));
    assert_eq!(old.get(&0), Some(&vec![0; 257]));
    assert_eq!(current.get(&0), Some(&vec![99; 257]));
    faults.clone.store(false, Ordering::SeqCst);
}
#[test]
fn failed_shared_rehash_preserves_roots_and_forward_transfer_avoids_unchanged_key_clones() {
    transfer_hash_failure::<ReuseSharedFlat<0>>();
    transfer_hash_failure::<ReuseSharedFlat<2>>();
}
