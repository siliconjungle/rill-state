#![cfg(feature = "experimental")]
use rill_state::experimental::{ProbeMap, SplitMap};
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
