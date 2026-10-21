//! Generic key/payload-shape checks and development measurements. This complements,
//! rather than replaces, the registered inline-u64 comparison and final holdout gate.
use rill_state::{
    experimental::{
        ArrayRecordMap, CopyKeys, HasherPolicy, InlineSliceMap, KeyPolicy, OwnedHasher, ProbeMap,
        RecordMap, SharedHasher, SharedKeys, SharedMap, SplitMap,
    },
    Config, Cursor, Mutation, Snapshot, Store,
};
use std::{
    collections::{BTreeMap, VecDeque},
    hash::{BuildHasher, Hash},
    hint::black_box,
    sync::Arc,
    time::Instant,
};
trait Key: Clone + Ord + Hash + Eq + std::fmt::Debug + Send + Sync {
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
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct VariableKey(String);
impl Key for VariableKey {
    fn make(id: u64) -> Self {
        const SUFFIX: [usize; 16] = [
            0, 0, 0, 8, 8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 512, 1024,
        ];
        let mut key = format!("k/{id:016x}/");
        key.extend(std::iter::repeat_n(
            (b'a' + (id % 26) as u8) as char,
            SUFFIX[id as usize % 16],
        ));
        Self(key)
    }
}
trait Value: Clone + PartialEq + Eq + std::fmt::Debug + Send + Sync {
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
fn mixed_bytes(id: u64, epoch: u64) -> Vec<u8> {
    let len = match id.wrapping_add(epoch) % 32 {
        0..=7 => 0,
        8..=11 => 8,
        12..=15 => 32,
        16..=23 => 128,
        24..=27 => 512,
        28..=30 => 2048,
        _ => 4096,
    };
    let mut data = vec![id.wrapping_add(epoch) as u8; len];
    if len >= 8 {
        data[..8].copy_from_slice(&(id ^ epoch).to_le_bytes());
    }
    if len >= 16 {
        data[len - 8..].copy_from_slice(&epoch.to_le_bytes());
    }
    data
}
fn touch_mixed(data: &[u8]) -> u64 {
    if data.is_empty() {
        0
    } else {
        touch_bytes(data) ^ (data.len() as u64)
    }
}
#[derive(Clone, Eq, PartialEq, Debug)]
struct MixedOwned(Vec<u8>);
impl Value for MixedOwned {
    fn make(id: u64, epoch: u64) -> Self {
        Self(mixed_bytes(id, epoch))
    }
    fn touch(&self) -> u64 {
        touch_mixed(&self.0)
    }
}
#[derive(Clone, Eq, PartialEq, Debug)]
struct MixedShared(Arc<[u8]>);
impl Value for MixedShared {
    fn make(id: u64, epoch: u64) -> Self {
        Self(mixed_bytes(id, epoch).into())
    }
    fn touch(&self) -> u64 {
        touch_mixed(&self.0)
    }
}
trait Map<K: Key, V: Value>: Clone + Send + Sync {
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
            fn batch(&mut self, ops: &[Mutation<K, V>]) {
                for op in ops {
                    match op {
                        Mutation::Put(k, v) => {
                            if let Some(prior) = self.get_mut(k) {
                                *prior = v.clone();
                            } else {
                                self.insert(k.clone(), v.clone());
                            }
                        }
                        Mutation::Delete(k) => {
                            self.remove(k);
                        }
                    }
                }
            }
        }
    };
}
map!(hashbrown::HashMap<K,V>,hashbrown::HashMap::new());
map!(hashbrown::HashMap<K,V,std::collections::hash_map::RandomState>,hashbrown::HashMap::with_hasher(Default::default()));
map!(imbl::HashMap<K,V>,imbl::HashMap::new());
type ImblFast<K, V> =
    imbl::GenericHashMap<K, V, hashbrown::DefaultHashBuilder, imbl::shared_ptr::DefaultSharedPtr>;
map!(ImblFast<K,V>,ImblFast::with_hasher(Default::default()));
map!(im::HashMap<K,V>,im::HashMap::new());
map!(im::HashMap<K,V,hashbrown::DefaultHashBuilder>,im::HashMap::with_hasher(hashbrown::DefaultHashBuilder::default()));
impl<K: Key, V: Value, H: BuildHasher + Clone + Default + Send + Sync> Map<K, V>
    for rpds::HashTrieMapSync<K, V, H>
{
    fn new() -> Self {
        Self::new_with_hasher_and_ptr_kind(H::default())
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.get(k)
    }
    fn put(&mut self, k: K, v: V) {
        self.insert_mut(k, v);
    }
    fn del(&mut self, k: &K) {
        self.remove_mut(k);
    }
    fn len(&self) -> usize {
        self.size()
    }
    fn visit(&self, mut f: impl FnMut(&K, &V)) {
        for (k, v) in self.iter() {
            f(k, v);
        }
    }
    fn batch(&mut self, ops: &[Mutation<K, V>]) {
        for op in ops {
            match op {
                Mutation::Put(k, v) => {
                    if let Some(prior) = self.get_mut(k) {
                        *prior = v.clone();
                    } else {
                        self.insert_mut(k.clone(), v.clone());
                    }
                }
                Mutation::Delete(k) => {
                    self.remove_mut(k);
                }
            }
        }
    }
}

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
    fn batch(&mut self, ops: &[Mutation<K, V>]) {
        for op in ops {
            match op {
                Mutation::Put(k, v) => {
                    if let Some(prior) = self.get_mut_cow(k) {
                        *prior = v.clone();
                    } else {
                        self.insert_cow(k.clone(), v.clone());
                    }
                }
                Mutation::Delete(k) => {
                    self.remove_cow(k);
                }
            }
        }
    }
}
impl<K: Key, V: Value, const L: usize, const P: usize> Map<K, V>
    for SharedMap<K, V, hashbrown::DefaultHashBuilder, L, P>
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
impl<K: Key, V: Value, const L: usize, const P: usize> Map<K, V>
    for RecordMap<K, V, hashbrown::DefaultHashBuilder, L, P>
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
impl<
        K: Key,
        V: Value,
        const L: usize,
        HP: HasherPolicy<hashbrown::DefaultHashBuilder>,
        const SPAN: bool,
    > Map<K, V> for InlineSliceMap<K, V, hashbrown::DefaultHashBuilder, L, HP, SPAN>
where
    HP::Stored: Send + Sync,
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
impl<
        K: Key,
        V: Value,
        const L: usize,
        const P: usize,
        KP: KeyPolicy<K>,
        HP: HasherPolicy<hashbrown::DefaultHashBuilder>,
    > Map<K, V> for ArrayRecordMap<K, V, hashbrown::DefaultHashBuilder, L, P, KP, HP>
where
    KP::Stored: Send + Sync,
    HP::Stored: Send + Sync,
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
trait Read<K: Key, V: Value>: Clone + Send + Sync {
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
impl<K: Key, V: Value, S: BuildHasher + Send + Sync> Read<K, V> for Snapshot<K, V, S> {
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
impl<K: Key, V: Value, S: BuildHasher + Send + Sync> Engine<K, V> for Native<K, V, S> {
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
    thread_check: bool,
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
        thread_check: a.iter().any(|x| x == "--thread-check"),
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
    if o.thread_check {
        let mut expected = BTreeMap::new();
        apply(&mut expected, &initial);
        let mut final_expected = expected.clone();
        let barrier = std::sync::Barrier::new(5);
        std::thread::scope(|scope| {
            for worker in 0..4 {
                let old = pinned.clone();
                let expected = &expected;
                let reads = &reads;
                let barrier = &barrier;
                scope.spawn(move || {
                    let revision = old.revision();
                    barrier.wait();
                    for _ in 0..o.rounds {
                        for k in reads.iter().skip(worker).step_by(4) {
                            assert_eq!(old.get(k), expected.get(k));
                        }
                        assert_eq!(old.revision(), revision);
                    }
                });
            }
            barrier.wait();
            for ops in traces {
                apply(&mut final_expected, &ops);
                engine.publish(ops);
            }
        });
        assert_eq!(engine.view().contents(), final_expected);
        println!("{{\"engine\":\"{}\",\"shape\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"verified\":true,\"threads\":4,\"thread_checked\":true}}",o.engine,o.shape,o.case,o.seed,o.records,o.readers,o.rounds);
        return;
    }
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
        "inline-shared-span-25" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,25,SharedHasher,true>)
        }
        "inline-shared-span-50" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,50,SharedHasher,true>)
        }
        "inline-shared-span-70" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,70,SharedHasher,true>)
        }
        "inline-shared-span-85" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,85,SharedHasher,true>)
        }
        "inline-shared-index-25" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,25,SharedHasher,false>)
        }
        "inline-shared-index-50" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,50,SharedHasher,false>)
        }
        "inline-shared-index-70" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,70,SharedHasher,false>)
        }
        "inline-shared-index-85" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,85,SharedHasher,false>)
        }
        "inline-owned-span-25" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,25,OwnedHasher,true>)
        }
        "inline-owned-span-50" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,50,OwnedHasher,true>)
        }
        "inline-owned-span-70" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,70,OwnedHasher,true>)
        }
        "inline-owned-span-85" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,85,OwnedHasher,true>)
        }
        "inline-owned-index-25" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,25,OwnedHasher,false>)
        }
        "inline-owned-index-50" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,50,OwnedHasher,false>)
        }
        "inline-owned-index-70" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,70,OwnedHasher,false>)
        }
        "inline-owned-index-85" => {
            map!(InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,85,OwnedHasher,false>)
        }
        "hashbrown" => map!(hashbrown::HashMap<K,V>),
        "hashbrown-sip" => map!(hashbrown::HashMap<K,V,std::collections::hash_map::RandomState>),
        "imbl-hash" => map!(imbl::HashMap<K,V>),
        "imbl-fast" => map!(ImblFast<K,V>),
        "im-hash" => map!(im::HashMap<K,V>),
        "im-fast" => map!(im::HashMap<K,V,hashbrown::DefaultHashBuilder>),
        "rpds-hash" => map!(rpds::HashTrieMapSync<K,V>),
        "rpds-fast" => map!(rpds::HashTrieMapSync<K,V,hashbrown::DefaultHashBuilder>),
        "imhm" => map!(imhm::Map<K,V>),
        "array-copy16" => map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,16,CopyKeys>),
        "array-copy64" => map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,64,CopyKeys>),
        "array-copy256" => map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,256,CopyKeys>),
        "array-copy1024" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,1024,CopyKeys>)
        }
        "array-shared16" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,16,SharedKeys>)
        }
        "array-shared64" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,64,SharedKeys>)
        }
        "array-shared256" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,256,SharedKeys>)
        }
        "array-shared1024" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,70,1024,SharedKeys>)
        }
        "array-copy-low16" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,16,CopyKeys>)
        }
        "array-copy-low256" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,256,CopyKeys>)
        }
        "array-shared-low16" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,16,SharedKeys>)
        }
        "array-shared-low256" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,256,SharedKeys>)
        }
        "record-lowflat" => map!(RecordMap<K,V,hashbrown::DefaultHashBuilder,50,0>),
        "probe-lowflat" => map!(ProbeMap<K,V,hashbrown::DefaultHashBuilder,25,0>),
        "array-owned-copy-low16" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,16,CopyKeys,OwnedHasher>)
        }
        "array-owned-copy-low256" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,256,CopyKeys,OwnedHasher>)
        }
        "array-owned-shared-low16" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,16,SharedKeys,OwnedHasher>)
        }
        "array-owned-shared-low256" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,50,256,SharedKeys,OwnedHasher>)
        }
        "array-owned-copy-sparse64" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,64,CopyKeys,OwnedHasher>)
        }
        "array-owned-copy-sparse1024" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,1024,CopyKeys,OwnedHasher>)
        }
        "array-owned-shared-sparse64" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,64,SharedKeys,OwnedHasher>)
        }
        "array-owned-shared-sparse1024" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,1024,SharedKeys,OwnedHasher>)
        }
        "array-copy-sparse64" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,64,CopyKeys>)
        }
        "array-copy-sparse1024" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,1024,CopyKeys>)
        }
        "array-shared-sparse64" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,64,SharedKeys>)
        }
        "array-shared-sparse1024" => {
            map!(ArrayRecordMap<K,V,hashbrown::DefaultHashBuilder,25,1024,SharedKeys>)
        }
        "record-flat" => map!(RecordMap<K,V,hashbrown::DefaultHashBuilder,70,0>),
        "record-paged16" => map!(RecordMap<K,V,hashbrown::DefaultHashBuilder,70,16>),
        "record-paged64" => map!(RecordMap<K,V,hashbrown::DefaultHashBuilder,70,64>),
        "record-paged256" => map!(RecordMap<K,V,hashbrown::DefaultHashBuilder,70,256>),
        "shared-flat" => map!(SharedMap<K,V,hashbrown::DefaultHashBuilder,70,0>),
        "shared-paged16" => map!(SharedMap<K,V,hashbrown::DefaultHashBuilder,70,16>),
        "shared-paged64" => map!(SharedMap<K,V,hashbrown::DefaultHashBuilder,70,64>),
        "shared-paged256" => map!(SharedMap<K,V,hashbrown::DefaultHashBuilder,70,256>),
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
        "mixed-owned" => dispatch::<VariableKey, MixedOwned>(&o),
        "mixed-shared" => dispatch::<VariableKey, MixedShared>(&o),
        "u64-inline" => dispatch::<u64, [u64; 4]>(&o),
        "string-inline" => dispatch::<String, [u64; 4]>(&o),
        "u64-owned256" => dispatch::<u64, Vec<u8>>(&o),
        "string-owned256" => dispatch::<String, Vec<u8>>(&o),
        "u64-shared256" => dispatch::<u64, Arc<[u8]>>(&o),
        "string-shared256" => dispatch::<String, Arc<[u8]>>(&o),
        _ => panic!("unknown shape"),
    }
}
