#![cfg(feature = "experimental")]
use rill_state::{
    experimental::{InlineSliceMap, ProbeMap, SplitMap},
    Mutation,
};
use std::{
    borrow::Borrow,
    hash::{BuildHasherDefault, Hash, Hasher},
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
            "injected clone failure"
        );
        Self {
            n: self.n,
            fail: self.fail.clone(),
        }
    }
}
trait Subject: Clone {
    fn put(&mut self, k: u64, v: Value);
    fn read(&self, k: u64) -> Option<u64>;
    fn len(&self) -> usize;
    fn count(&self) -> usize;
}
macro_rules! subject {
    ($ty:ty) => {
        impl Subject for $ty {
            fn put(&mut self, k: u64, v: Value) {
                self.insert(k, v);
            }
            fn read(&self, k: u64) -> Option<u64> {
                self.get(&k).map(|v| v.n)
            }
            fn len(&self) -> usize {
                self.len()
            }
            fn count(&self) -> usize {
                self.iter().count()
            }
        }
    };
}
impl<const P: usize> Subject for ProbeMap<u64, Value, BuildHasherDefault<Collision>, 50, P> {
    fn put(&mut self, k: u64, v: Value) {
        self.insert(k, v);
    }
    fn read(&self, k: u64) -> Option<u64> {
        self.get(&k).map(|v| v.n)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn count(&self) -> usize {
        self.iter().count()
    }
}
subject!(InlineSliceMap<u64,Value,BuildHasherDefault<Collision>,50>);
fn growth(mut map: impl Subject) {
    let fail = Arc::new(AtomicBool::new(false));
    for n in 0..64 {
        map.put(
            n,
            Value {
                n,
                fail: fail.clone(),
            },
        );
    }
    let old = map.clone();
    fail.store(true, Ordering::Relaxed);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.put(
            64,
            Value {
                n: 64,
                fail: fail.clone(),
            },
        )
    }));
    assert!(result.is_err());
    assert_eq!(map.len(), 64);
    assert_eq!(map.count(), 64);
    for n in 0..64 {
        assert_eq!(map.read(n), Some(n));
        assert_eq!(old.read(n), Some(n));
    }
    fail.store(false, Ordering::Relaxed);
    map.put(64, Value { n: 64, fail });
    assert_eq!(map.len(), 65);
    assert_eq!(old.len(), 64);
}
#[test]
fn inline_growth_clone_panic_preserves_current_and_retained_bindings() {
    growth(InlineSliceMap::<
        u64,
        Value,
        BuildHasherDefault<Collision>,
        50,
    >::default());
}
#[test]
fn probing_growth_clone_panic_preserves_current_and_retained_bindings() {
    growth(ProbeMap::<u64, Value, BuildHasherDefault<Collision>, 50, 0>::default());
    growth(ProbeMap::<u64, Value, BuildHasherDefault<Collision>, 50, 16>::default());
    growth(ProbeMap::<u64, Value, BuildHasherDefault<Collision>, 50, 64>::default());
}
#[test]
fn paged_delete_clone_panic_preserves_probe_reachability() {
    use std::sync::atomic::AtomicU64;
    struct PagedValue {
        n: u64,
        fail_at: Arc<AtomicU64>,
    }
    impl Clone for PagedValue {
        fn clone(&self) -> Self {
            assert_ne!(
                self.n,
                self.fail_at.load(Ordering::Relaxed),
                "injected paged clone failure"
            );
            Self {
                n: self.n,
                fail_at: self.fail_at.clone(),
            }
        }
    }
    let fail_at = Arc::new(AtomicU64::new(u64::MAX));
    let mut base = ProbeMap::<u64, PagedValue, BuildHasherDefault<Collision>, 70, 16>::default();
    for n in 0..32 {
        base.insert(
            n,
            PagedValue {
                n,
                fail_at: fail_at.clone(),
            },
        );
    }
    let mut panics = 0;
    // Cover every victim/failure pair, independently of growth or iterator order.
    for victim in 0..32 {
        for failure in 0..32 {
            let mut map = base.clone();
            fail_at.store(failure, Ordering::Relaxed);
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.remove(&victim)));
            fail_at.store(u64::MAX, Ordering::Relaxed);
            match result {
                Err(_) => panics += 1,
                Ok(value) => assert_eq!(value.unwrap().n, victim),
            }
            assert_eq!(map.len(), map.iter().count());
            assert_eq!(map.len(), 31 + usize::from(map.get(&victim).is_some()));
            for n in 0..32 {
                if n != victim {
                    assert_eq!(map.get(&n).unwrap().n, n);
                }
                assert_eq!(base.get(&n).unwrap().n, n);
            }
        }
    }
    assert!(panics > 0, "the injected clone failures must be exercised");
}
#[test]
fn flat_batch_delete_drop_panic_keeps_counts_and_probe_chains() {
    #[derive(Clone)]
    struct DropValue(Arc<AtomicBool>);
    impl Drop for DropValue {
        fn drop(&mut self) {
            assert!(
                !self.0.swap(false, Ordering::Relaxed),
                "injected drop failure"
            );
        }
    }
    macro_rules! check {
        ($ty:ty) => {
            let fail = Arc::new(AtomicBool::new(false));
            let mut map = <$ty>::default();
            map.insert(1, DropValue(fail.clone()));
            map.insert(2, DropValue(fail.clone()));
            let old = map.clone();
            fail.store(true, Ordering::Relaxed);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                map.apply(&[Mutation::Delete(1)])
            }));
            assert!(result.is_err());
            assert_eq!(map.len(), 1);
            assert_eq!(map.iter().count(), 1);
            assert!(map.get(&2).is_some());
            assert_eq!(old.len(), 2);
        };
    }
    check!(ProbeMap<u64,DropValue,BuildHasherDefault<Collision>,50,0>);
    check!(InlineSliceMap<u64,DropValue,BuildHasherDefault<Collision>,50>);
}
#[test]
fn split_delete_key_drop_panic_keeps_counts_and_probe_chains() {
    #[derive(Clone)]
    struct Key {
        n: u64,
        fail: Arc<AtomicBool>,
    }
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.n == other.n
        }
    }
    impl Eq for Key {}
    impl Hash for Key {
        fn hash<H: Hasher>(&self, h: &mut H) {
            self.n.hash(h);
        }
    }
    impl Borrow<u64> for Key {
        fn borrow(&self) -> &u64 {
            &self.n
        }
    }
    impl Drop for Key {
        fn drop(&mut self) {
            if self.n == 1 {
                assert!(
                    !self.fail.swap(false, Ordering::Relaxed),
                    "injected key-drop failure"
                );
            }
        }
    }
    let fail = Arc::new(AtomicBool::new(false));
    let mut map = SplitMap::<Key, u64, BuildHasherDefault<Collision>, 50>::default();
    for n in 1..=3 {
        map.insert(
            Key {
                n,
                fail: fail.clone(),
            },
            n,
        );
    }
    let old = map.clone();
    fail.store(true, Ordering::Relaxed);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.remove(&1)));
    assert!(result.is_err());
    assert_eq!(map.len(), 2);
    assert_eq!(map.iter().count(), 2);
    assert_eq!(map.get(&2), Some(&2));
    assert_eq!(map.get(&3), Some(&3));
    assert_eq!(old.get(&1), Some(&1));
}
