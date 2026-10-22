#![cfg(feature = "experimental")]
use rill_state::{
    experimental::{ShardedInlineMap, SharedHasher},
    Mutation,
};
use std::{
    hash::{BuildHasherDefault, Hasher},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Default)]
struct Collision;
impl Hasher for Collision {
    fn finish(&self) -> u64 {
        u64::MAX
    }
    fn write(&mut self, _: &[u8]) {}
}
struct Value {
    n: u64,
    fail: Arc<AtomicBool>,
}
impl Clone for Value {
    fn clone(&self) -> Self {
        assert!(
            !(self.n == 2 && self.fail.load(Ordering::Relaxed)),
            "injected value-clone failure"
        );
        Self {
            n: self.n,
            fail: self.fail.clone(),
        }
    }
}
type Map<const G: bool> =
    ShardedInlineMap<u64, Value, BuildHasherDefault<Collision>, 50, 16, SharedHasher, G>;

#[test]
fn sharded_panic_grouped_clone_keeps_counts_and_map_usable() {
    let fail = Arc::new(AtomicBool::new(true));
    let mut map = Map::<true>::default();
    let old = map.clone();
    let ops = [
        Mutation::Put(
            1,
            Value {
                n: 1,
                fail: fail.clone(),
            },
        ),
        Mutation::Put(
            2,
            Value {
                n: 2,
                fail: fail.clone(),
            },
        ),
    ];
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.apply(&ops)));
    assert!(outcome.is_err());
    assert_eq!(map.iter().count(), 1);
    assert_eq!(map.len(), 1);
    assert!(old.is_empty());
    assert_eq!(map.get(&1).unwrap().n, 1);
    assert!(map.get(&2).is_none());
    fail.store(false, Ordering::Relaxed);
    map.insert(3, Value { n: 3, fail });
    assert_eq!(map.len(), 2);
    assert_eq!(map.iter().count(), 2);
}

#[test]
fn sharded_panic_clone_during_shared_growth_preserves_all_existing_keys() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut map = Map::<false>::default();
    for n in 0..8 {
        map.insert(
            n,
            Value {
                n,
                fail: fail.clone(),
            },
        );
    }
    let old = map.clone();
    fail.store(true, Ordering::Relaxed);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.insert(
            8,
            Value {
                n: 8,
                fail: fail.clone(),
            },
        )
    }));
    assert!(outcome.is_err());
    assert_eq!(map.len(), 8);
    assert_eq!(map.iter().count(), 8);
    for n in 0..8 {
        assert_eq!(map.get(&n).unwrap().n, n);
        assert_eq!(old.get(&n).unwrap().n, n);
    }
    fail.store(false, Ordering::Relaxed);
    map.insert(8, Value { n: 8, fail });
    assert_eq!(map.len(), 9);
    assert_eq!(old.len(), 8);
}

#[test]
fn sharded_panic_drop_during_batch_delete_keeps_counts_and_old_view() {
    #[derive(Clone)]
    struct DropValue(Arc<AtomicBool>);
    impl Drop for DropValue {
        fn drop(&mut self) {
            assert!(
                !self.0.swap(false, Ordering::Relaxed),
                "injected value-drop failure"
            );
        }
    }
    fn check<const G: bool>() {
        let fail = Arc::new(AtomicBool::new(false));
        let mut map = ShardedInlineMap::<
            u64,
            DropValue,
            BuildHasherDefault<Collision>,
            50,
            16,
            SharedHasher,
            G,
        >::default();
        map.insert(1, DropValue(fail.clone()));
        let old = map.clone();
        fail.store(true, Ordering::Relaxed);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            map.apply(&[Mutation::Delete(1)])
        }));
        assert!(outcome.is_err());
        assert!(map.is_empty());
        assert_eq!(map.iter().count(), 0);
        assert_eq!(old.len(), 1);
        assert!(old.get(&1).is_some());
    }
    check::<false>();
    check::<true>();
}
