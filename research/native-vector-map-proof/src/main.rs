#![forbid(unsafe_code)]
use std::{
    fmt::Debug,
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
const N: usize = 20_000;
#[derive(Debug)]
struct Stats {
    live: AtomicUsize,
    clones: AtomicUsize,
    fail_id: AtomicUsize,
}
impl Stats {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            live: AtomicUsize::new(0),
            clones: AtomicUsize::new(0),
            fail_id: AtomicUsize::new(usize::MAX),
        })
    }
}
#[derive(Debug)]
struct Value {
    id: usize,
    epoch: u8,
    payload: Vec<u8>,
    stats: Arc<Stats>,
}
impl Value {
    fn new(id: usize, epoch: u8, stats: &Arc<Stats>) -> Self {
        stats.live.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            epoch,
            payload: vec![epoch; 128],
            stats: stats.clone(),
        }
    }
}
impl Clone for Value {
    fn clone(&self) -> Self {
        assert_ne!(
            self.stats.fail_id.load(Ordering::Relaxed),
            self.id,
            "injected value clone failure"
        );
        self.stats.clones.fetch_add(1, Ordering::Relaxed);
        let payload = self.payload.clone();
        self.stats.live.fetch_add(1, Ordering::Relaxed);
        Self {
            id: self.id,
            epoch: self.epoch,
            payload,
            stats: self.stats.clone(),
        }
    }
}
impl Drop for Value {
    fn drop(&mut self) {
        self.stats.live.fetch_sub(1, Ordering::Relaxed);
    }
}
trait Vector: Clone + Send + Sync {
    fn push(&mut self, value: Value);
    fn pop(&mut self) -> Option<Value>;
    fn get(&self, index: usize) -> Option<&Value>;
    fn get_mut(&mut self, index: usize) -> Option<&mut Value>;
    fn len(&self) -> usize;
}
macro_rules! vector {
    ($ty:ty) => {
        impl Vector for $ty {
            fn push(&mut self, value: Value) {
                self.push(value)
            }
            fn pop(&mut self) -> Option<Value> {
                self.pop()
            }
            fn get(&self, index: usize) -> Option<&Value> {
                self.get(index)
            }
            fn get_mut(&mut self, index: usize) -> Option<&mut Value> {
                self.get_mut(index)
            }
            fn len(&self) -> usize {
                self.len()
            }
        }
    };
}
vector!(pvec::PVec<Value>);
vector!(pvec::core::RrbVec<Value>);
vector!(pvec::core::RbVec<Value>);
impl Vector for Vec<Value> {
    fn push(&mut self, v: Value) {
        Vec::push(self, v)
    }
    fn pop(&mut self) -> Option<Value> {
        Vec::pop(self)
    }
    fn get(&self, i: usize) -> Option<&Value> {
        self.as_slice().get(i)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut Value> {
        self.as_mut_slice().get_mut(i)
    }
    fn len(&self) -> usize {
        Vec::len(self)
    }
}
fn check<V: Vector>(v: &V, epochs: &[u8]) {
    assert_eq!(v.len(), epochs.len());
    for (id, &epoch) in epochs.iter().enumerate() {
        let value = v.get(id).expect("missing vector row");
        assert_eq!((value.id, value.epoch), (id, epoch));
        assert_eq!(value.payload.len(), 128);
        assert!(value.payload.iter().all(|&b| b == epoch));
    }
    assert!(v.get(epochs.len()).is_none());
}
fn vector_proof<V: Vector>(name: &str, mut current: V) {
    let stats = Stats::new();
    for id in 0..N {
        current.push(Value::new(id, 0, &stats));
    }
    let initial = current.clone();
    let mut epochs = vec![0; N];
    let mut old = Vec::new();
    for epoch in 1..=16u8 {
        let mut next = current.clone();
        for i in 0..200 {
            let id = ((epoch as usize - 1) * 200 + i) % N;
            *next.get_mut(id).unwrap() = Value::new(id, epoch, &stats);
            epochs[id] = epoch;
        }
        current = next;
        check(&current, &epochs);
        old.push((current.clone(), epochs.clone()));
    }
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let history = &old;
            scope.spawn(move || {
                for (view, model) in history {
                    check(view, model);
                }
            });
        }
    });
    check(&initial, &vec![0; N]);
    stats.fail_id.store(17, Ordering::Relaxed);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut next = current.clone();
        let _ = next.get_mut(17).unwrap();
    }));
    assert!(failed.is_err(), "failed clone was not exercised");
    stats.fail_id.store(usize::MAX, Ordering::Relaxed);
    check(&current, &epochs);
    for (view, model) in &old {
        check(view, model);
    }
    // The primitives expose pop/get_mut, sufficient for a staged swap-removal
    // adapter. This is an API proof, not yet a complete key/index/journal engine.
    let branch = if cfg!(feature = "small-branch") {
        4
    } else {
        32
    };
    let tail = if N % branch == 0 { branch } else { N % branch };
    stats.fail_id.store(N - tail - 1, Ordering::Relaxed);
    let pop_failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut next = current.clone();
        for _ in 0..=tail {
            let _ = next.pop();
        }
    }));
    assert!(
        pop_failed.is_err(),
        "shared-tail clone failure was not exercised"
    );
    stats.fail_id.store(usize::MAX, Ordering::Relaxed);
    check(&current, &epochs);
    for (view, model) in &old {
        check(view, model);
    }
    let mut swapped = current.clone();
    let last = swapped.pop().unwrap();
    let removed = std::mem::replace(swapped.get_mut(7).unwrap(), last);
    assert_eq!(removed.id, 7);
    drop(removed);
    assert_eq!(swapped.len(), N - 1);
    assert_eq!(swapped.get(7).unwrap().id, N - 1);
    for i in 0..N - 1 {
        let id = if i == 7 { N - 1 } else { i };
        let value = swapped.get(i).unwrap();
        assert_eq!((value.id, value.epoch), (id, epochs[id]));
        assert!(value.payload.iter().all(|&b| b == epochs[id]));
    }
    assert!(swapped.get(N - 1).is_none());

    check(&current, &epochs);
    drop(swapped);
    drop(old);
    drop(initial);
    drop(current);
    assert_eq!(stats.live.load(Ordering::Relaxed), 0);
    println!("{{\"kind\":\"native-vector-api-proof\",\"engine\":\"{name}\",\"records\":{N},\"retained_views\":16,\"reader_threads\":4,\"clone_panic_exercised\":true,\"pop_clone_panic_exercised\":true,\"after_all_drop\":0,\"value_clones\":{},\"small_branch\":{},\"verified\":true}}",stats.clones.load(Ordering::Relaxed),cfg!(feature="small-branch"));
}
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
        self.id.hash(h)
    }
}
impl std::borrow::Borrow<u64> for Key {
    fn borrow(&self) -> &u64 {
        &self.id
    }
}
fn map_proof<S: BuildHasher + Clone + Send + Sync>(name: &str, hasher: S, swap: bool) {
    let stats = Stats::new();
    let mut map = ordermap::OrderMap::with_hasher(hasher.clone());
    for id in 0..N {
        map.insert(
            Key {
                id: id as u64,
                metadata: 0,
            },
            Value::new(id, 0, &stats),
        );
    }
    let old = map.clone();
    for id in 0..200usize {
        *map.get_mut(&(id as u64)).unwrap() = Value::new(id, 1, &stats);
    }
    let removed = if swap {
        map.swap_remove(&0u64)
    } else {
        map.remove(&0u64)
    };
    assert_eq!(removed.unwrap().id, 0);
    map.insert(Key { id: 0, metadata: 2 }, Value::new(0, 2, &stats));
    assert_eq!(map.get_key_value(&0u64).unwrap().0.metadata, 2);
    map.insert(Key { id: 0, metadata: 9 }, Value::new(0, 3, &stats));
    assert_eq!(map.get_key_value(&0u64).unwrap().0.metadata, 2);
    assert_eq!(map.len(), N);
    assert_eq!(old.len(), N);
    for id in 0..N {
        let expected = if id == 0 {
            3
        } else if id < 200 {
            1
        } else {
            0
        };
        let value = map.get(&(id as u64)).unwrap();
        assert_eq!((value.id, value.epoch), (id, expected));
        assert!(value.payload.iter().all(|&b| b == expected));
    }
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let view = &old;
            scope.spawn(move || {
                for id in 0..N {
                    let (key, value) = view.get_key_value(&(id as u64)).unwrap();
                    assert_eq!(key.metadata, 0);
                    assert_eq!((value.id, value.epoch), (id, 0));
                    assert!(value.payload.iter().all(|&b| b == 0));
                }
            });
        }
    });
    stats.fail_id.store(17, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| map.clone())).is_err());
    stats.fail_id.store(usize::MAX, Ordering::Relaxed);
    assert_eq!(map.len(), N);
    for id in 0..N {
        assert_eq!(old.get(&(id as u64)).unwrap().epoch, 0);

        let value = map.get(&(id as u64)).unwrap();
        let expected = if id == 0 {
            3
        } else if id < 200 {
            1
        } else {
            0
        };
        assert_eq!((value.id, value.epoch), (id, expected));
        assert!(value.payload.iter().all(|&b| b == expected));
    }
    let mut order = ordermap::OrderMap::with_hasher(hasher);
    for id in 0..8 {
        order.insert(id, id);
    }
    if swap {
        order.swap_remove(&1);
    } else {
        order.remove(&1);
    }
    let actual = order.keys().copied().collect::<Vec<_>>();
    assert_eq!(
        actual,
        if swap {
            vec![0, 7, 2, 3, 4, 5, 6]
        } else {
            vec![0, 2, 3, 4, 5, 6, 7]
        }
    );
    drop(old);
    drop(map);
    assert_eq!(stats.live.load(Ordering::Relaxed), 0);
    println!("{{\"kind\":\"native-map-api-proof\",\"engine\":\"{name}\",\"records\":{N},\"reader_threads\":4,\"clone_panic_exercised\":true,\"after_all_drop\":0,\"verified\":true}}");
}
fn main() {
    vector_proof("std-vec", Vec::new());
    vector_proof("pvec-flat", pvec::PVec::new());
    vector_proof("pvec-tree", pvec::PVec::new_with_tree());
    vector_proof("pvec-rrb", pvec::core::RrbVec::new());
    vector_proof("pvec-rb", pvec::core::RbVec::new());
    for swap in [false, true] {
        map_proof(
            if swap {
                "ordermap-swap"
            } else {
                "ordermap-shift"
            },
            std::collections::hash_map::RandomState::new(),
            swap,
        );
        map_proof(
            if swap {
                "ordermap-swap-fast"
            } else {
                "ordermap-shift-fast"
            },
            hashbrown::DefaultHashBuilder::default(),
            swap,
        );
    }
}
