//! Generic key/payload-shape checks and development measurements. This complements,
//! rather than replaces, the registered inline-u64 comparison and final holdout gate.
use rill_state::{
    experimental::{ProbeMap, SplitMap},
    Config, Cursor, Mutation, Snapshot, Store,
};
use std::{
    collections::{BTreeMap, VecDeque},
    hash::{BuildHasher, Hash},
    hint::black_box,
    sync::Arc,
    time::Instant,
};
trait Key: Clone + Ord + Hash + Eq + std::fmt::Debug {
    fn make(n: u64) -> Self;
}
impl Key for u64 {
    fn make(n: u64) -> Self {
        n.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
    }
}
impl Key for String {
    fn make(n: u64) -> Self {
        format!("tenant/{:016x}/resource/{n:016x}", <u64 as Key>::make(n))
    }
}
trait Value: Clone + PartialEq + Eq + std::fmt::Debug {
    fn make(id: u64, epoch: u64) -> Self;
    fn touch(&self) -> u64;
}
impl Value for [u64; 4] {
    fn make(id: u64, epoch: u64) -> Self {
        [id, epoch, id ^ epoch, id.wrapping_add(epoch)]
    }
    fn touch(&self) -> u64 {
        self[0] ^ self[1] ^ self[2] ^ self[3]
    }
}
fn bytes(id: u64, epoch: u64) -> Vec<u8> {
    let mut b = vec![(epoch % 256) as u8; 256];
    b[..8].copy_from_slice(&id.to_le_bytes());
    b[248..].copy_from_slice(&epoch.to_le_bytes());
    b
}
fn touch_bytes(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
        ^ u64::from_le_bytes(b[b.len() - 8..].try_into().unwrap())
}
impl Value for Vec<u8> {
    fn make(id: u64, epoch: u64) -> Self {
        bytes(id, epoch)
    }
    fn touch(&self) -> u64 {
        touch_bytes(self)
    }
}
impl Value for Arc<[u8]> {
    fn make(id: u64, epoch: u64) -> Self {
        bytes(id, epoch).into()
    }
    fn touch(&self) -> u64 {
        touch_bytes(self)
    }
}
trait Map<K: Key, V: Value>: Clone {
    fn new() -> Self;
    fn get(&self, k: &K) -> Option<&V>;
    fn put(&mut self, k: K, v: V);
    fn del(&mut self, k: &K);
    fn len(&self) -> usize;
    fn visit(&self, f: impl FnMut(&K, &V));
    fn batch(&mut self, ops: &[Mutation<K, V>]) {
        for op in ops {
            match op {
                Mutation::Put(k, v) => self.put(k.clone(), v.clone()),
                Mutation::Delete(k) => self.del(k),
            }
        }
    }
}
macro_rules! map {
    ($ty:ty,$new:expr) => {
        impl<K: Key, V: Value> Map<K, V> for $ty {
            fn new() -> Self {
                $new
            }
            fn get(&self, k: &K) -> Option<&V> {
                self.get(k)
            }
            fn put(&mut self, k: K, v: V) {
                self.insert(k, v);
            }
            fn del(&mut self, k: &K) {
                self.remove(k);
            }
            fn len(&self) -> usize {
                self.len()
            }
            fn visit(&self, mut f: impl FnMut(&K, &V)) {
                for (k, v) in self.iter() {
                    f(k, v);
                }
            }
        }
    };
}
map!(hashbrown::HashMap<K,V>,hashbrown::HashMap::new());
map!(hashbrown::HashMap<K,V,std::collections::hash_map::RandomState>,hashbrown::HashMap::with_hasher(Default::default()));
map!(imbl::HashMap<K,V>,imbl::HashMap::new());
impl<K: Key, V: Value> Map<K, V> for imhm::Map<K, V> {
    fn new() -> Self {
        Self::new()
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.get(k)
    }
    fn put(&mut self, k: K, v: V) {
        self.insert_cow(k, v);
    }
    fn del(&mut self, k: &K) {
        self.remove_cow(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(&K, &V)) {
        for (k, v) in self.iter() {
            f(k, v);
        }
    }
}
impl<K: Key, V: Value, const L: usize, const P: usize> Map<K, V>
    for ProbeMap<K, V, hashbrown::DefaultHashBuilder, L, P>
{
    fn new() -> Self {
        Self::default()
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.get(k)
    }
    fn put(&mut self, k: K, v: V) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &K) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(&K, &V)) {
        for (k, v) in self.iter() {
            f(k, v);
        }
    }
    fn batch(&mut self, ops: &[Mutation<K, V>]) {
        self.apply(ops);
    }
}
impl<K: Key, V: Value, const L: usize> Map<K, V>
    for SplitMap<K, V, hashbrown::DefaultHashBuilder, L>
{
    fn new() -> Self {
        Self::default()
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.get(k)
    }
    fn put(&mut self, k: K, v: V) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &K) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(&K, &V)) {
        for (k, v) in self.iter() {
            f(k, v);
        }
    }
}
trait Read<K: Key, V: Value>: Clone {
    fn revision(&self) -> u64;
    fn get(&self, k: &K) -> Option<&V>;
    fn len(&self) -> usize;
    fn visit(&self, f: impl FnMut(&K, &V));
    fn contents(&self) -> BTreeMap<K, V> {
        let mut m = BTreeMap::new();
        self.visit(|k, v| {
            assert!(m.insert(k.clone(), v.clone()).is_none());
        });
        m
    }
    fn checksum(&self) -> u64 {
        let mut sum = 0u64;
        self.visit(|_, v| sum = sum.wrapping_add(v.touch()));
        sum
    }
}
impl<K: Key, V: Value, S: BuildHasher> Read<K, V> for Snapshot<K, V, S> {
    fn revision(&self) -> u64 {
        self.cursor().revision()
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.get(k)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(&K, &V)) {
        for (k, v) in self.iter() {
            f(k, v);
        }
    }
}
struct View<M>(Arc<M>, u64);
impl<M> Clone for View<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1)
    }
}
impl<K: Key, V: Value, M: Map<K, V>> Read<K, V> for View<M> {
    fn revision(&self) -> u64 {
        self.1
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.0.get(k)
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn visit(&self, f: impl FnMut(&K, &V)) {
        self.0.visit(f)
    }
}
trait Engine<K: Key, V: Value> {
    type View: Read<K, V>;
    fn publish(&mut self, ops: Vec<Mutation<K, V>>);
    fn view(&self) -> Self::View;
    fn revision(&self) -> u64;
    fn feed(&self, after: u64) -> Vec<Vec<Mutation<K, V>>>;
}
struct Compared<K: Key, V: Value, M: Map<K, V>> {
    view: View<M>,
    history: VecDeque<(u64, Arc<[Mutation<K, V>]>)>,
    operations: usize,
}
impl<K: Key, V: Value, M: Map<K, V>> Compared<K, V, M> {
    fn new() -> Self {
        Self {
            view: View(Arc::new(M::new()), 0),
            history: VecDeque::new(),
            operations: 0,
        }
    }
}
impl<K: Key, V: Value, M: Map<K, V>> Engine<K, V> for Compared<K, V, M> {
    type View = View<M>;
    fn publish(&mut self, ops: Vec<Mutation<K, V>>) {
        let ops: Arc<[Mutation<K, V>]> = ops.into();
        if !ops.is_empty() {
            let mut map = (*self.view.0).clone();
            map.batch(&ops);
            self.view.0 = Arc::new(map);
        }
        self.view.1 += 1;
        self.operations += ops.len();
        self.history.push_back((self.view.1, ops));
        while self.history.len() > 64 || self.operations > 1_000_000 {
            self.operations -= self.history.pop_front().unwrap().1.len();
        }
    }
    fn view(&self) -> Self::View {
        self.view.clone()
    }
    fn revision(&self) -> u64 {
        self.view.1
    }
    fn feed(&self, after: u64) -> Vec<Vec<Mutation<K, V>>> {
        let oldest = self.history.front().map_or(self.view.1, |(r, _)| r - 1);
        assert!(after >= oldest && after <= self.view.1);
        self.history
            .iter()
            .skip((after - oldest) as usize)
            .map(|(_, ops)| ops.to_vec())
            .collect()
    }
}
struct Native<K: Key, V: Value, S: BuildHasher> {
    store: Store<K, V, S>,
    cursors: Vec<Cursor>,
}
impl<K: Key, V: Value, S: BuildHasher + Default> Native<K, V, S> {
    fn new() -> Self {
        let store = Store::with_hasher(Config::default(), S::default()).unwrap();
        let zero = store.cursor();
        Self {
            store,
            cursors: vec![zero],
        }
    }
}
impl<K: Key, V: Value, S: BuildHasher> Engine<K, V> for Native<K, V, S> {
    type View = Snapshot<K, V, S>;
    fn publish(&mut self, ops: Vec<Mutation<K, V>>) {
        self.cursors.push(self.store.commit(ops).unwrap());
    }
    fn view(&self) -> Self::View {
        self.store.snapshot()
    }
    fn revision(&self) -> u64 {
        self.store.cursor().revision()
    }
    fn feed(&self, after: u64) -> Vec<Vec<Mutation<K, V>>> {
        self.store
            .changes_since(self.cursors[after as usize])
            .unwrap()
            .map(|b| b.operations().to_vec())
            .collect()
    }
}
struct Options {
    engine: String,
    shape: String,
    case: String,
    seed: u64,
    records: usize,
    readers: usize,
    rounds: usize,
    verify: bool,
}
fn options() -> Options {
    let a: Vec<_> = std::env::args().collect();
    let get = |key: &str, default: &str| {
        a.windows(2)
            .find(|p| p[0] == key)
            .map(|p| p[1].clone())
            .unwrap_or_else(|| default.into())
    };
    Options {
        engine: get("--engine", "hashbrown"),
        shape: get("--shape", "string-owned256"),
        case: get("--case", "sparse"),
        seed: get("--seed", "4815162342").parse().unwrap(),
        records: get("--records", "20000").parse().unwrap(),
        readers: get("--readers", "20000").parse().unwrap(),
        rounds: get("--rounds", "16").parse().unwrap(),
        verify: a.iter().any(|x| x == "--verify"),
    }
}
fn rng(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}
fn apply<K: Key, V: Value>(m: &mut BTreeMap<K, V>, ops: &[Mutation<K, V>]) {
    for op in ops {
        match op {
            Mutation::Put(k, v) => {
                m.insert(k.clone(), v.clone());
            }
            Mutation::Delete(k) => {
                m.remove(k);
            }
        }
    }
}
fn run<K: Key, V: Value, E: Engine<K, V>>(mut engine: E, o: &Options) {
    assert!(o.records > 0 && o.rounds > 0);
    let initial: Vec<_> = (0..o.records as u64)
        .map(|i| Mutation::Put(K::make(i), V::make(i, 0)))
        .collect();
    engine.publish(initial.clone());
    for r in 0..8 {
        engine.publish(
            (0..o.records.min(128) as u64)
                .map(|i| Mutation::Put(K::make(i), V::make(i, r + 1)))
                .collect(),
        );
        let v = engine.view();
        for i in 0..o.records.min(128) as u64 {
            black_box(v.get(&K::make(i)));
        }
    }
    engine.publish(initial.clone());
    let pinned = engine.view();
    let mut seed = o.seed;
    let count = match o.case.as_str() {
        "read" | "miss100" | "scan" => 0,
        "single" => 1,
        "sparse" | "churn" | "slow" => o.records / 100,
        "burst" => o.records / 2,
        _ => panic!("unknown case"),
    };
    let traces: Vec<Vec<_>> = (0..o.rounds)
        .map(|r| {
            (0..count)
                .map(|i| {
                    let id = rng(&mut seed) % o.records as u64;
                    if o.case == "churn" && i % 2 == 0 {
                        Mutation::Delete(K::make(id))
                    } else {
                        Mutation::Put(K::make(id), V::make(id, r as u64 + 1))
                    }
                })
                .collect()
        })
        .collect();
    let reads: Vec<_> = (0..o.readers)
        .map(|_| {
            K::make(
                rng(&mut seed) % o.records as u64
                    + if o.case == "miss100" {
                        o.records as u64
                    } else {
                        0
                    },
            )
        })
        .collect();
    if o.verify {
        let mut truth = BTreeMap::new();
        apply(&mut truth, &initial);
        let mut old = vec![(pinned.clone(), truth.clone(), pinned.revision())];
        for ops in traces {
            let after = engine.revision();
            apply(&mut truth, &ops);
            engine.publish(ops.clone());
            assert_eq!(engine.feed(after), vec![ops]);
            let view = engine.view();
            assert_eq!(view.revision(), after + 1);
            assert_eq!(view.len(), truth.len());
            assert_eq!(view.contents(), truth);
            for k in &reads {
                assert_eq!(view.get(k), truth.get(k));
            }
            for (v, m, r) in &old {
                assert_eq!(v.revision(), *r);
                assert_eq!(v.contents(), *m);
            }
            if old.len() < 5 {
                old.push((view, truth.clone(), after + 1));
            }
        }
        println!("{{\"engine\":\"{}\",\"shape\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"verified\":true}}",o.engine,o.shape,o.case,o.seed,o.records,o.readers,o.rounds);
        return;
    }
    let mut checksum = 0u64;
    let mut held = Vec::new();
    let mut publish_ns = 0u128;
    let mut read_ns = 0u128;
    let wall = Instant::now();
    for ops in traces {
        let start = Instant::now();
        engine.publish(ops);
        publish_ns += start.elapsed().as_nanos();
        let start = Instant::now();
        let view = engine.view();
        if o.case == "scan" {
            checksum = checksum.wrapping_add(black_box(view.checksum()));
        } else {
            for k in &reads {
                checksum = checksum
                    .wrapping_add(black_box(view.get(black_box(k))).map_or(0, Value::touch));
            }
        }
        if o.case == "slow" {
            held.push(view);
            if held.len() > 16 {
                held.remove(0);
            }
        }
        read_ns += start.elapsed().as_nanos();
    }
    black_box(pinned);
    black_box(held);
    let total = wall.elapsed().as_nanos();
    let view = engine.view();
    println!("{{\"engine\":\"{}\",\"shape\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"total_ns\":{},\"publish_ns\":{},\"read_ns\":{},\"read_checksum\":{},\"checksum\":{},\"final_len\":{}}}",o.engine,o.shape,o.case,o.seed,o.records,o.readers,o.rounds,total,publish_ns,read_ns,checksum,view.checksum(),view.len());
}
fn dispatch<K: Key, V: Value>(o: &Options) {
    macro_rules! map {
        ($m:ty) => {
            run::<K, V, _>(Compared::<K, V, $m>::new(), o)
        };
    }
    match o.engine.as_str() {
        "hashbrown" => map!(hashbrown::HashMap<K,V>),
        "hashbrown-sip" => map!(hashbrown::HashMap<K,V,std::collections::hash_map::RandomState>),
        "imbl-hash" => map!(imbl::HashMap<K,V>),
        "imhm" => map!(imhm::Map<K,V>),
        "probe-flat" => map!(ProbeMap<K,V,hashbrown::DefaultHashBuilder,70,0>),
        "probe-paged16" => map!(ProbeMap<K,V,hashbrown::DefaultHashBuilder,70,16>),
        "probe-paged64" => map!(ProbeMap<K,V,hashbrown::DefaultHashBuilder,70,64>),
        "split" => map!(SplitMap<K,V,hashbrown::DefaultHashBuilder,70>),
        "rill" => run::<K, V, _>(
            Native::<K, V, std::collections::hash_map::RandomState>::new(),
            o,
        ),
        "rillfast" => run::<K, V, _>(Native::<K, V, hashbrown::DefaultHashBuilder>::new(), o),
        _ => panic!("unknown engine"),
    }
}
fn main() {
    let o = options();
    match o.shape.as_str() {
        "u64-inline" => dispatch::<u64, [u64; 4]>(&o),
        "string-inline" => dispatch::<String, [u64; 4]>(&o),
        "u64-owned256" => dispatch::<u64, Vec<u8>>(&o),
        "string-owned256" => dispatch::<String, Vec<u8>>(&o),
        "u64-shared256" => dispatch::<u64, Arc<[u8]>>(&o),
        "string-shared256" => dispatch::<String, Arc<[u8]>>(&o),
        _ => panic!("unknown shape"),
    }
}
