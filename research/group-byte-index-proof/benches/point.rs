//! Already-held raw getters; not snapshot engine or network performance.
use group_byte_index_proof::GroupByteMap;
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::{BuildHasher, Hash},
    hint::black_box,
    time::Instant,
};
type Value = [u64; 4];
trait Key: Clone + Hash + Eq + Debug {
    fn make(id: u64) -> Self;
}
impl Key for u64 {
    fn make(id: u64) -> Self {
        id.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
    }
}
impl Key for String {
    fn make(id: u64) -> Self {
        format!("key-{:016x}-{:016x}-{:016x}", id, id.rotate_left(13), !id)
    }
}
trait Map<K> {
    fn put(&mut self, k: K, v: Value);
    fn get(&self, k: &K) -> Option<&Value>;
}
impl<K: Clone + Hash + Eq, S: BuildHasher, const L: usize, const G: usize> Map<K>
    for GroupByteMap<K, Value, S, L, G>
{
    fn put(&mut self, k: K, v: Value) {
        self.insert(k, v);
    }
    #[inline]
    fn get(&self, k: &K) -> Option<&Value> {
        self.get(k)
    }
}
macro_rules! map {
    ($ty:ty) => {
        impl<K: Hash + Eq, S: BuildHasher> Map<K> for $ty {
            fn put(&mut self, k: K, v: Value) {
                self.insert(k, v);
            }
            #[inline]
            fn get(&self, k: &K) -> Option<&Value> {
                self.get(k)
            }
        }
    };
}
map!(HashMap<K,Value,S>);
map!(hashbrown::HashMap<K,Value,S>);
fn value(id: u64) -> Value {
    [id, !id, id.rotate_left(13), id.wrapping_mul(317)]
}
fn digest(v: &Value) -> u64 {
    v[0].rotate_left(7) ^ v[1].wrapping_mul(19) ^ v[2].rotate_left(23) ^ v[3]
}
fn rng(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}
struct Options {
    engine: String,
    shape: String,
    case: String,
    seed: u64,
    records: usize,
    readers: usize,
    rounds: usize,
}
#[inline]
fn reads<K, M: Map<K>>(map: &M, keys: &[K]) -> u64 {
    let mut sum = 0u64;
    for key in keys {
        if let Some(v) = black_box(map.get(black_box(key))) {
            sum = sum.wrapping_add(digest(v));
        }
    }
    black_box(sum)
}
fn run<K: Key, M: Map<K>>(mut map: M, o: &Options) {
    for id in 0..o.records as u64 {
        map.put(K::make(id), value(id));
    }
    // Complete initial values and guaranteed-absent keys, outside timing.
    for id in 0..o.records as u64 {
        assert_eq!(map.get(&K::make(id)), Some(&value(id)));
        assert!(map.get(&K::make(id + o.records as u64)).is_none());
    }
    let percent = match o.case.as_str() {
        "read" => 0,
        "miss50" => 50,
        "miss100" => 100,
        _ => panic!("unknown case"),
    };
    let mut seed = o.seed;
    let mut expected = 0u64;
    let keys: Vec<_> = (0..o.readers)
        .map(|i| {
            let id = rng(&mut seed) % o.records as u64;
            let missing = i % 100 < percent;
            if !missing {
                expected = expected.wrapping_add(digest(&value(id)));
            }
            K::make(id + if missing { o.records as u64 } else { 0 })
        })
        .collect();
    for _ in 0..4 {
        assert_eq!(reads(&map, &keys), expected);
    }
    let mut sum = 0u64;
    let start = Instant::now();
    for _ in 0..o.rounds {
        sum = sum.wrapping_add(reads(black_box(&map), &keys));
    }
    let total_ns = start.elapsed().as_nanos();
    assert_eq!(sum, expected.wrapping_mul(o.rounds as u64));
    println!("{{\"kind\":\"raw-held-map-getter\",\"engine\":\"{}\",\"shape\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"total_ns\":{},\"checksum\":{},\"verified\":true}}",o.engine,o.shape,o.case,o.seed,o.records,o.readers,o.rounds,total_ns,sum);
}
fn grouped<K: Key, S: BuildHasher + Default>(o: &Options, policy: &str) {
    let (load, group) = policy.split_once('-').unwrap();
    let load: usize = load.parse().unwrap();
    let group: usize = group.parse().unwrap();
    macro_rules! call {
        ($l:literal,$g:literal) => {
            run::<K, _>(GroupByteMap::<K, Value, S, $l, $g>::default(), o)
        };
    }
    match (load, group) {
        (25, 8) => call!(25, 8),
        (25, 16) => call!(25, 16),
        (25, 32) => call!(25, 32),
        (50, 8) => call!(50, 8),
        (50, 16) => call!(50, 16),
        (50, 32) => call!(50, 32),
        (70, 8) => call!(70, 8),
        (70, 16) => call!(70, 16),
        (70, 32) => call!(70, 32),
        (85, 8) => call!(85, 8),
        (85, 16) => call!(85, 16),
        (85, 32) => call!(85, 32),
        _ => panic!("unknown policy"),
    }
}
fn dispatch<K: Key>(o: &Options) {
    type Sip = std::collections::hash_map::RandomState;
    type Fast = hashbrown::DefaultHashBuilder;
    match o.engine.as_str() {
        "std-hash" => run::<K, _>(HashMap::<K, Value, Sip>::default(), o),
        "std-fast" => run::<K, _>(HashMap::<K, Value, Fast>::default(), o),
        "hashbrown-sip" => run::<K, _>(hashbrown::HashMap::<K, Value, Sip>::default(), o),
        "hashbrown" => run::<K, _>(hashbrown::HashMap::<K, Value, Fast>::default(), o),
        e if e.starts_with("group-sip-") => {
            grouped::<K, Sip>(o, e.strip_prefix("group-sip-").unwrap())
        }
        e if e.starts_with("group-fast-") => {
            grouped::<K, Fast>(o, e.strip_prefix("group-fast-").unwrap())
        }
        _ => panic!("unknown engine"),
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let get = |name: &str, default: &str| {
        args.windows(2)
            .find(|p| p[0] == name)
            .map(|p| p[1].clone())
            .unwrap_or_else(|| default.into())
    };
    let o = Options {
        engine: get("--engine", "group-fast-70-8"),
        shape: get("--shape", "u64-inline"),
        case: get("--case", "read"),
        seed: get("--seed", "4815162342").parse().unwrap(),
        records: get("--records", "20000").parse().unwrap(),
        readers: get("--readers", "20000").parse().unwrap(),
        rounds: get("--rounds", "64").parse().unwrap(),
    };
    assert!(o.records > 0 && o.readers > 0 && o.rounds > 0);
    match o.shape.as_str() {
        "u64-inline" => dispatch::<u64>(&o),
        "string-inline" => dispatch::<String>(&o),
        _ => panic!("unknown shape"),
    }
}
