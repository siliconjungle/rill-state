use rill_state::{
    Config, FeedError,
    Mutation::{self, *},
    Store,
};
use std::collections::BTreeMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

#[derive(Default)]
struct Collisions;
impl Hasher for Collisions {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {}
}

fn rng(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}
fn model(m: &mut BTreeMap<u64, u64>, ops: &[Mutation<u64, u64>]) {
    for op in ops {
        match op {
            Put(k, v) => {
                m.insert(*k, *v);
            }
            Delete(k) => {
                m.remove(k);
            }
        }
    }
}

#[test]
fn random_replay_old_snapshots_and_layouts() {
    // Every power-of-two branch grouping across eight page counts. Independent BTreeMap
    // oracle, complete key-value comparison, exact mutation-log replay and pinned old views.
    for bits in 0..8 {
        for branch_bits in 0..=bits {
            let cfg = Config {
                partitions: 1 << bits,
                branch_size: 1 << branch_bits,
                history_batches: 8,
                history_operations: 128,
            };
            let mut s = Store::new(cfg).unwrap();
            let mut expected = BTreeMap::new();
            let mut seed = 564341u64;
            let mut old = Vec::new();
            let mut cursor = s.snapshot().cursor();
            let mut replica = BTreeMap::new();
            for revision in 1..=300 {
                let ops: Vec<_> = (0..rng(&mut seed) % 23)
                    .map(|_| {
                        let key = rng(&mut seed) % 150;
                        if rng(&mut seed) % 3 == 0 {
                            Delete(key)
                        } else {
                            Put(key, rng(&mut seed))
                        }
                    })
                    .collect();
                model(&mut expected, &ops);
                let c = s.commit(ops.clone()).unwrap();
                assert_eq!(c.revision(), revision);
                let batches: Vec<_> = s.changes_since(cursor).unwrap().collect();
                assert_eq!(batches.len(), 1);
                assert_eq!(batches[0].operations(), ops);
                for b in batches {
                    model(&mut replica, b.operations());
                }
                assert_eq!(replica, expected);
                cursor = c;
                let snap = s.snapshot();
                assert_eq!(snap.len(), expected.len());
                assert_eq!(
                    snap.iter()
                        .map(|(k, v)| (*k, *v))
                        .collect::<BTreeMap<_, _>>(),
                    expected
                );
                for k in 0..170 {
                    assert_eq!(snap.get(&k), expected.get(&k));
                }
                if revision % 39 == 0 {
                    old.push((snap, expected.clone()));
                }
                for (view, truth) in &old {
                    assert_eq!(
                        view.iter()
                            .map(|(k, v)| (*k, *v))
                            .collect::<BTreeMap<_, _>>(),
                        *truth
                    );
                }
                assert!(s.retained_history().0 <= 8 && s.retained_history().1 <= 128);
            }
        }
    }
}

#[test]
fn colliding_non_numeric_keys_and_non_copy_values() {
    let mut s = Store::with_hasher(
        Config::default(),
        BuildHasherDefault::<Collisions>::default(),
    )
    .unwrap();
    s.commit((0..100).map(|k| Put(format!("key {k}"), vec![k; 8])))
        .unwrap();
    let old = s.snapshot();
    s.commit((0..100).map(|k| {
        if k % 2 == 0 {
            Delete(format!("key {k}"))
        } else {
            Put(format!("key {k}"), vec![0])
        }
    }))
    .unwrap();
    for k in 0..100 {
        assert_eq!(old.get(&format!("key {k}")), Some(&vec![k; 8]));
        assert_eq!(
            s.snapshot().get(&format!("key {k}")).cloned(),
            if k % 2 == 0 { None } else { Some(vec![0]) }
        );
    }
    assert_eq!(s.snapshot().len(), 50);
}

#[test]
fn gaps_empty_batches_duplicate_keys_and_foreign_cursors() {
    let mut s = Store::new(Config {
        history_batches: 2,
        history_operations: 3,
        ..Config::default()
    })
    .unwrap();
    let zero = s.snapshot().cursor();
    let one = s.commit([Put(1, 2), Delete(1), Put(1, 3)]).unwrap();
    let pinned_batch = s.changes_since(zero).unwrap().next().unwrap().clone();
    assert_eq!(s.snapshot().get(&1), Some(&3));
    s.commit([]).unwrap();
    s.commit([Delete(0)]).unwrap();
    assert!(matches!(s.changes_since(zero), Err(FeedError::Gap { oldest, .. }) if oldest == one));
    assert_eq!(s.changes_since(one).unwrap().len(), 2);
    let last = s.commit(std::iter::repeat_n(Put(9, 0), 4)).unwrap();
    assert_eq!(s.retained_history(), (0, 0));
    assert_eq!(s.changes_since(last).unwrap().len(), 0);
    assert!(matches!(s.changes_since(one), Err(FeedError::Gap { .. })));
    assert_eq!(pinned_batch.operations().len(), 3);
    let other = Store::<i32, i32>::default();
    assert!(matches!(
        s.changes_since(other.snapshot().cursor()),
        Err(FeedError::ForeignCursor)
    ));
}

#[test]
fn zero_history_is_explicit_and_snapshot_outlives_store() {
    let snapshot = {
        let mut s = Store::new(Config {
            history_batches: 0,
            history_operations: 0,
            ..Config::default()
        })
        .unwrap();
        let before = s.snapshot().cursor();
        s.commit([Put("abc".to_owned(), Arc::new(vec![42]))])
            .unwrap();
        assert!(matches!(
            s.changes_since(before),
            Err(FeedError::Gap { .. })
        ));
        s.snapshot()
    };
    assert_eq!(snapshot.get(&"abc".to_owned()).unwrap().as_slice(), &[42]);
}

#[test]
fn invalid_layouts_fail_before_allocation() {
    for (partitions, branch_size) in [(0, 1), (1, 0), (3, 1), (4, 8), (1 << 21, 64), (8, 3)] {
        assert!(Store::<u32, u32>::new(Config {
            partitions,
            branch_size,
            ..Config::default()
        })
        .is_err());
    }
}

#[test]
fn panicking_input_does_not_publish_partial_batch() {
    let mut s = Store::default();
    s.commit([Put(1, 2)]).unwrap();
    let before = s.snapshot();
    let iter = (0..3).map(|i| {
        assert_ne!(i, 2, "intentional iterator panic");
        Put(i, 55)
    });
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.commit(iter))).is_err());
    assert_eq!(s.snapshot().cursor(), before.cursor());
    assert_eq!(s.snapshot().get(&1), Some(&2));
}

#[test]
fn twenty_thousand_pinned_readers_share_a_revision() {
    let mut s = Store::default();
    s.commit((0..20_000).map(|k| Put(k, k))).unwrap();
    let readers: Vec<_> = (0..20_000).map(|_| s.snapshot()).collect();
    std::thread::scope(|scope| {
        for slice in readers.chunks(2_500) {
            scope.spawn(move || {
                for (i, view) in slice.iter().enumerate() {
                    assert_eq!(view.get(&i), Some(&i));
                    assert_eq!(view.len(), 20_000);
                }
            });
        }
        for revision in 1..20 {
            s.commit((0..20_000).map(|k| Put(k, k + revision))).unwrap();
        }
    });
    assert_eq!(s.snapshot().get(&0), Some(&19));
    assert_eq!(readers[0].get(&0), Some(&0));
}

#[test]
fn panicking_value_clone_leaves_state_and_feed_unchanged() {
    #[derive(Debug)]
    struct Value(u64);
    impl Clone for Value {
        fn clone(&self) -> Self {
            assert_ne!(self.0, 99, "intentional clone panic");
            Self(self.0)
        }
    }
    let mut s = Store::default();
    s.commit([Put(0, Value(0))]).unwrap();
    let before = s.snapshot();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || s.commit([Put(0, Value(1)), Put(1, Value(99))])
    ))
    .is_err());
    assert_eq!(s.snapshot().cursor(), before.cursor());
    assert_eq!(s.snapshot().get(&0).unwrap().0, 0);
    assert!(s.snapshot().get(&1).is_none());
    assert_eq!(s.changes_since(before.cursor()).unwrap().len(), 0);
}

#[test]
fn evicted_values_reclaimed_when_last_snapshot_is_released() {
    let payload = Arc::new(vec![13u8; 4096]);
    let weak = Arc::downgrade(&payload);
    let mut s = Store::new(Config {
        history_batches: 0,
        history_operations: 0,
        ..Config::default()
    })
    .unwrap();
    s.commit([Put("blob", payload)]).unwrap();
    let old = s.snapshot();
    s.commit([Delete("blob")]).unwrap();
    assert!(weak.upgrade().is_some());
    drop(old);
    assert!(weak.upgrade().is_none());
}

#[test]
fn complete_revisions_can_be_published_to_concurrent_readers() {
    let mut s = Store::default();
    s.commit((0..128).map(|k| Put(k, 0))).unwrap();
    let shared = Arc::new(std::sync::RwLock::new(s.snapshot()));
    let barrier = Arc::new(std::sync::Barrier::new(5));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let shared = shared.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                let mut last = 0;
                for _ in 0..2000 {
                    let view = shared.read().unwrap().clone();
                    let value = view.cursor().revision() - 1;
                    assert!(value >= last);
                    last = value;
                    for k in 0..128 {
                        assert_eq!(view.get(&k), Some(&value));
                    }
                }
            });
        }
        barrier.wait();
        for revision in 1..128 {
            s.commit((0..128).map(|k| Put(k, revision))).unwrap();
            *shared.write().unwrap() = s.snapshot();
        }
    });
    assert_eq!(shared.read().unwrap().get(&0), Some(&127));
}
