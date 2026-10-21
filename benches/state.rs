//! Shared-contract comparison driver. Every measured engine retains immutable snapshots
//! and records ordered batch operations. Concurrent maps require explicit copy adapters.
use rill_state::{Config, Cursor, Mutation, Snapshot, Store};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::BuildHasher;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

type Key = u64;
type Value = [u64; 4];
type Op = Mutation<Key, Value>;
fn rng(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}
fn key(n: u64) -> u64 {
    n.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
}
fn value(k: u64, round: u64) -> Value {
    [k, round, k ^ round, k.wrapping_add(round)]
}
fn digest(k: u64, v: Value) -> u64 {
    k.wrapping_add(v[0].rotate_left(7)) ^ v[1].wrapping_mul(317) ^ v[2].rotate_left(31) ^ v[3]
}

trait ReadView: Clone + Send + Sync {
    fn revision(&self) -> u64;
    fn get(&self, k: &Key) -> Option<Value>;
    fn len(&self) -> usize;
    fn checksum(&self) -> u64;
    fn contents(&self) -> BTreeMap<Key, Value>;
}
trait Engine {
    type View: ReadView;
    fn publish(&mut self, ops: Vec<Op>);
    fn snapshot(&self) -> Self::View;
    fn feed(&self, after: u64) -> Vec<Arc<[Op]>>;
    fn revision(&self) -> u64;
    fn feed_summary(&self, after: u64) -> (usize, usize);
}
impl<S: BuildHasher + Send + Sync> ReadView for Snapshot<Key, Value, S> {
    fn revision(&self) -> u64 {
        self.cursor().revision()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn checksum(&self) -> u64 {
        self.iter()
            .fold(0u64, |a, (k, v)| a.wrapping_add(digest(*k, *v)))
    }
    fn contents(&self) -> BTreeMap<Key, Value> {
        self.iter().map(|(k, v)| (*k, *v)).collect()
    }
}
struct Rill<S = std::collections::hash_map::RandomState> {
    store: Store<Key, Value, S>,
    cursors: Vec<Cursor>,
}
impl<S: BuildHasher + Send + Sync> Engine for Rill<S> {
    type View = Snapshot<Key, Value, S>;
    fn publish(&mut self, ops: Vec<Op>) {
        self.cursors.push(self.store.commit(ops).unwrap());
    }
    fn snapshot(&self) -> Self::View {
        self.store.snapshot()
    }
    fn feed(&self, after: u64) -> Vec<Arc<[Op]>> {
        // Materialize the same Arc<[Op]> form as other adapters. This is used only by
        // the oracle; timed feeds below compare exact shared-batch metadata separately.
        self.store
            .changes_since(self.cursors[after as usize])
            .unwrap()
            .map(|b| Arc::from(b.operations()))
            .collect()
    }
    fn revision(&self) -> u64 {
        self.store.cursor().revision()
    }
    fn feed_summary(&self, after: u64) -> (usize, usize) {
        let mut n = 0;
        let mut ops = 0;
        for b in self
            .store
            .changes_since(self.cursors[after as usize])
            .unwrap()
        {
            n += 1;
            ops += black_box(b.operations()).len();
        }
        (n, ops)
    }
}

trait Map: Clone + Send + Sync {
    fn empty() -> Self;
    fn get(&self, k: &Key) -> Option<Value>;
    fn put(&mut self, k: Key, v: Value);
    fn del(&mut self, k: &Key);
    fn len(&self) -> usize;
    fn visit(&self, f: impl FnMut(Key, Value));
    fn batch(&mut self, ops: &[Op]) {
        for op in ops {
            match op {
                Op::Put(k, v) => self.put(*k, *v),
                Op::Delete(k) => self.del(k),
            }
        }
    }
}
struct View<M>(Arc<M>, u64);
impl<M> Clone for View<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1)
    }
}
impl<M: Map> ReadView for View<M> {
    fn revision(&self) -> u64 {
        self.1
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.get(k)
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn checksum(&self) -> u64 {
        let mut sum = 0u64;
        self.0.visit(|k, v| sum = sum.wrapping_add(digest(k, v)));
        sum
    }
    fn contents(&self) -> BTreeMap<Key, Value> {
        let mut m = BTreeMap::new();
        self.0.visit(|k, v| {
            assert!(m.insert(k, v).is_none());
        });
        m
    }
}
struct Compared<M> {
    current: View<M>,
    history: VecDeque<(u64, Arc<[Op]>)>,
    revision: u64,
    count: usize,
}
impl<M: Map> Compared<M> {
    fn new() -> Self {
        Self {
            current: View(Arc::new(M::empty()), 0),
            history: VecDeque::new(),
            revision: 0,
            count: 0,
        }
    }
}
impl<M: Map> Engine for Compared<M> {
    type View = View<M>;
    fn publish(&mut self, ops: Vec<Op>) {
        let ops: Arc<[Op]> = ops.into();
        // Empty revisions change the cursor/journal, not the state. In particular,
        // a mutable HashMap baseline must not pay for an unnecessary full copy.
        if !ops.is_empty() {
            let mut map = (*self.current.0).clone();
            map.batch(&ops);
            self.current.0 = Arc::new(map);
        }
        self.revision += 1;
        self.current.1 = self.revision;
        self.count += ops.len();
        self.history.push_back((self.revision, ops));
        while self.history.len() > 64 || self.count > 1_000_000 {
            self.count -= self.history.pop_front().unwrap().1.len();
        }
    }
    fn snapshot(&self) -> Self::View {
        self.current.clone()
    }
    fn feed(&self, after: u64) -> Vec<Arc<[Op]>> {
        let oldest = self.history.front().map_or(self.revision, |(r, _)| r - 1);
        assert!(after >= oldest && after <= self.revision);
        self.history
            .iter()
            .skip((after - oldest) as usize)
            .map(|(_, ops)| ops.clone())
            .collect()
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn feed_summary(&self, after: u64) -> (usize, usize) {
        let oldest = self.history.front().map_or(self.revision, |(r, _)| r - 1);
        assert!(after >= oldest && after <= self.revision);
        let mut n = 0;
        let mut ops = 0;
        for (_, b) in self.history.iter().skip((after - oldest) as usize) {
            n += 1;
            ops += black_box(&**b).len();
        }
        (n, ops)
    }
}

macro_rules! map_impl {
    ($ty:ty,$new:expr) => {
        impl Map for $ty {
            fn empty() -> Self {
                $new
            }
            fn get(&self, k: &Key) -> Option<Value> {
                self.get(k).copied()
            }
            fn put(&mut self, k: Key, v: Value) {
                self.insert(k, v);
            }
            fn del(&mut self, k: &Key) {
                self.remove(k);
            }
            fn len(&self) -> usize {
                self.len()
            }
            fn visit(&self, mut f: impl FnMut(Key, Value)) {
                for (k, v) in self.iter() {
                    f(*k, *v);
                }
            }
        }
    };
}
map_impl!(HashMap<Key,Value>,HashMap::new());
map_impl!(BTreeMap<Key,Value>,BTreeMap::new());
map_impl!(hashbrown::HashMap<Key,Value>,hashbrown::HashMap::new());
map_impl!(hashbrown::HashMap<Key,Value,std::collections::hash_map::RandomState>,hashbrown::HashMap::with_hasher(Default::default()));
map_impl!(im::HashMap<Key,Value>,im::HashMap::new());
map_impl!(im::OrdMap<Key,Value>,im::OrdMap::new());
map_impl!(imbl::HashMap<Key,Value>,imbl::HashMap::new());
type ImblFast<K, V> =
    imbl::GenericHashMap<K, V, hashbrown::DefaultHashBuilder, imbl::shared_ptr::DefaultSharedPtr>;
map_impl!(ImblFast<Key,Value>,ImblFast::with_hasher(Default::default()));
map_impl!(im::HashMap<Key,Value,hashbrown::DefaultHashBuilder>,im::HashMap::with_hasher(hashbrown::DefaultHashBuilder::default()));

map_impl!(imbl::OrdMap<Key,Value>,imbl::OrdMap::new());
map_impl!(indexmap::IndexMap<Key,Value>,indexmap::IndexMap::new());
map_impl!(btree_slab::BTreeMap<Key,Value>,btree_slab::BTreeMap::new());

impl<const L: usize, const P: usize> Map
    for rill_state::experimental::SharedMap<Key, Value, hashbrown::DefaultHashBuilder, L, P>
{
    fn empty() -> Self {
        Self::default()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        self.apply(ops);
    }
}
impl<const L: usize, const P: usize> Map
    for rill_state::experimental::RecordMap<Key, Value, hashbrown::DefaultHashBuilder, L, P>
{
    fn empty() -> Self {
        Self::default()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        self.apply(ops);
    }
}
impl<const L: usize, const P: usize> Map
    for rill_state::experimental::ProbeMap<Key, Value, hashbrown::DefaultHashBuilder, L, P>
{
    fn empty() -> Self {
        Self::default()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        self.apply(ops);
    }
}
impl<const L: usize, const P: usize, KP: rill_state::experimental::KeyPolicy<Key>> Map
    for rill_state::experimental::ArrayRecordMap<
        Key,
        Value,
        hashbrown::DefaultHashBuilder,
        L,
        P,
        KP,
    >
where
    KP::Stored: Send + Sync,
{
    fn empty() -> Self {
        Self::default()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        self.apply(ops);
    }
}
impl<const L: usize> Map
    for rill_state::experimental::SplitMap<Key, Value, hashbrown::DefaultHashBuilder, L>
{
    fn empty() -> Self {
        Self::default()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
}
// Native two-copy engines need independently owned backends to keep arbitrary
// old revisions without blocking later publications. Capturing/rebuilding and
// native guard costs belong to this adapter, not native short-reader throughput.
struct FlashCopy {
    write: std::sync::Mutex<flashmap::WriteHandle<Key, Value>>,
    read: flashmap::ReadHandle<Key, Value>,
}
impl Clone for FlashCopy {
    fn clone(&self) -> Self {
        let mut next = Self::empty();
        {
            let mut view = next.write.get_mut().unwrap().guard();
            self.visit(|k, v| {
                view.insert(k, v);
            });
        }
        next
    }
}
impl Map for FlashCopy {
    fn empty() -> Self {
        let (write, read) = flashmap::new();
        Self {
            write: std::sync::Mutex::new(write),
            read,
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.read.guard().get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.write.get_mut().unwrap().guard().insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.write.get_mut().unwrap().guard().remove(*k);
    }
    fn len(&self) -> usize {
        self.read.guard().len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        let view = self.read.guard();
        for (k, v) in view.iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        let mut view = self.write.get_mut().unwrap().guard();
        for op in ops {
            match op {
                Op::Put(k, v) => {
                    view.insert(*k, *v);
                }
                Op::Delete(k) => {
                    view.remove(*k);
                }
            }
        }
    }
}
struct EvmapCopy {
    write: std::sync::Mutex<evmap::handles::WriteHandle<Key, Value>>,
    read: std::sync::Mutex<evmap::handles::ReadHandle<Key, Value>>,
}
impl Clone for EvmapCopy {
    fn clone(&self) -> Self {
        let mut next = Self::empty();
        let write = next.write.get_mut().unwrap();
        self.visit(|k, v| {
            write.update(k, v);
        });
        write.publish();
        next
    }
}
impl Map for EvmapCopy {
    fn empty() -> Self {
        let (mut write, read) = evmap::new();
        write.publish();
        Self {
            write: std::sync::Mutex::new(write),
            read: std::sync::Mutex::new(read),
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.read.lock().unwrap().get_one(k).map(|v| *v)
    }
    fn put(&mut self, k: Key, v: Value) {
        self.write.get_mut().unwrap().update(k, v).publish();
    }
    fn del(&mut self, k: &Key) {
        self.write.get_mut().unwrap().remove_entry(*k).publish();
    }
    fn len(&self) -> usize {
        self.read.lock().unwrap().len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        let read = self.read.lock().unwrap();
        let view = read.enter().unwrap();
        for (k, values) in view.iter() {
            let mut values = values.iter();
            let value = *values.next().unwrap();
            assert!(values.next().is_none());
            f(*k, value);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        let write = self.write.get_mut().unwrap();
        for op in ops {
            match op {
                Op::Put(k, v) => {
                    write.update(*k, *v);
                }
                Op::Delete(k) => {
                    write.remove_entry(*k);
                }
            }
        }
        write.publish();
    }
}
#[derive(Clone)]
struct LeftData(hashbrown::HashMap<Key, Value>);
impl left_right::Absorb<Op> for LeftData {
    fn absorb_first(&mut self, op: &mut Op, _: &Self) {
        match op {
            Op::Put(k, v) => {
                self.0.insert(*k, *v);
            }
            Op::Delete(k) => {
                self.0.remove(k);
            }
        }
    }
    fn sync_with(&mut self, other: &Self) {
        self.0 = other.0.clone();
    }
}
struct LeftCopy {
    write: std::sync::Mutex<left_right::WriteHandle<LeftData, Op>>,
    read: std::sync::Mutex<left_right::ReadHandle<LeftData>>,
}
impl Clone for LeftCopy {
    fn clone(&self) -> Self {
        let mut next = Self::empty();
        let write = next.write.get_mut().unwrap();
        self.visit(|k, v| {
            write.append(Op::Put(k, v));
        });
        write.publish();
        next
    }
}
impl Map for LeftCopy {
    fn empty() -> Self {
        // Cloning one explicitly initialized table gives both sides identical
        // hash-builder state, as required by left-right's operational replay.
        let (mut write, read) = left_right::new_from_empty(LeftData(hashbrown::HashMap::new()));
        write.publish();
        Self {
            write: std::sync::Mutex::new(write),
            read: std::sync::Mutex::new(read),
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.read.lock().unwrap().enter().unwrap().0.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.write
            .get_mut()
            .unwrap()
            .append(Op::Put(k, v))
            .publish();
    }
    fn del(&mut self, k: &Key) {
        self.write
            .get_mut()
            .unwrap()
            .append(Op::Delete(*k))
            .publish();
    }
    fn len(&self) -> usize {
        self.read.lock().unwrap().enter().unwrap().0.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        let read = self.read.lock().unwrap();
        let view = read.enter().unwrap();
        for (k, v) in view.0.iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        let write = self.write.get_mut().unwrap();
        for op in ops {
            write.append(op.clone());
        }
        write.publish();
    }
}

impl Map for scc::HashMap<Key, Value> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.read_sync(k, |_, v| *v)
    }
    fn put(&mut self, k: Key, v: Value) {
        self.entry_sync(k)
            .and_modify(|prior| *prior = v)
            .or_insert(v);
    }
    fn del(&mut self, k: &Key) {
        self.remove_sync(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        self.iter_sync(|k, v| {
            f(*k, *v);
            true
        });
    }
}

struct SccCapture(
    scc::HashMap<Key, Value>,
    std::collections::hash_map::RandomState,
);
impl Clone for SccCapture {
    fn clone(&self) -> Self {
        // Complete the source read before entering another SCC map. This avoids
        // nested map operations inside the source iterator's bucket read guard.
        let mut rows = Vec::with_capacity(self.0.len());
        self.0.iter_sync(|k, v| {
            rows.push((*k, *v));
            true
        });
        let copy = scc::HashMap::with_capacity_and_hasher(rows.len(), self.1.clone());
        for (k, v) in rows {
            copy.insert_sync(k, v).unwrap();
        }
        Self(copy, self.1.clone())
    }
}
impl Map for SccCapture {
    fn empty() -> Self {
        let hasher = std::collections::hash_map::RandomState::new();
        Self(
            scc::HashMap::with_capacity_and_hasher(0, hasher.clone()),
            hasher,
        )
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.read_sync(k, |_, v| *v)
    }
    fn put(&mut self, k: Key, v: Value) {
        drop(self.0.entry_sync(k).insert_entry(v));
    }
    fn del(&mut self, k: &Key) {
        self.0.remove_sync(k);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        self.0.iter_sync(|k, v| {
            f(*k, *v);
            true
        });
    }
}
impl Map for contrie::ConMap<Key, Value> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).map(|e| *e.value())
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.iter().count()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for e in self.iter() {
            f(*e.key(), *e.value());
        }
    }
}
// The library compares atomic values bytewise; every byte must be initialized.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct LeapValue {
    value: Value,
    kind: u64,
}
const _: () = assert!(std::mem::size_of::<LeapValue>() == 5 * std::mem::size_of::<u64>());
impl leapfrog::Value for LeapValue {
    fn is_null(&self) -> bool {
        self.kind == 0
    }
    fn is_redirect(&self) -> bool {
        self.kind == 1
    }
    fn null() -> Self {
        Self {
            value: [0; 4],
            kind: 0,
        }
    }
    fn redirect() -> Self {
        Self {
            value: [0; 4],
            kind: 1,
        }
    }
}
struct Leap(
    std::sync::Mutex<leapfrog::HashMap<Key, LeapValue, hashbrown::DefaultHashBuilder>>,
    usize,
);
impl Clone for Leap {
    fn clone(&self) -> Self {
        Self(
            std::sync::Mutex::new(self.0.lock().unwrap().clone()),
            self.1,
        )
    }
}
impl Map for Leap {
    fn empty() -> Self {
        Self(
            std::sync::Mutex::new(leapfrog::HashMap::with_capacity_and_hasher(
                16,
                Default::default(),
            )),
            0,
        )
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.lock().unwrap().get(k).map(|v| v.value)
    }
    fn put(&mut self, k: Key, v: Value) {
        let prior = self
            .0
            .get_mut()
            .unwrap()
            .insert(k, LeapValue { value: v, kind: 2 });
        if prior.is_none_or(|old| old.kind == 0) {
            self.1 += 1;
        }
    }
    fn del(&mut self, k: &Key) {
        if self
            .0
            .get_mut()
            .unwrap()
            .remove(k)
            .is_some_and(|v| v.kind == 2)
        {
            self.1 -= 1;
        }
    }
    fn len(&self) -> usize {
        self.1
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.0.lock().unwrap().iter() {
            f(*k, v.value);
        }
    }
}

struct LeapConcurrent(leapfrog::LeapMap<Key, LeapValue, hashbrown::DefaultHashBuilder>);
impl Clone for LeapConcurrent {
    fn clone(&self) -> Self {
        let copy =
            leapfrog::LeapMap::with_capacity_and_hasher(self.0.capacity(), Default::default());
        for mut entry in self.0.iter() {
            if let Some((k, v)) = entry.key_value() {
                copy.insert(k, v);
            }
        }
        Self(copy)
    }
}
impl Map for LeapConcurrent {
    fn empty() -> Self {
        Self(leapfrog::LeapMap::with_capacity_and_hasher(
            16,
            Default::default(),
        ))
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.get(k).and_then(|mut r| r.value()).map(|v| v.value)
    }
    fn put(&mut self, k: Key, v: Value) {
        self.0.insert(k, LeapValue { value: v, kind: 2 });
    }
    fn del(&mut self, k: &Key) {
        self.0.remove(k);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for mut entry in self.0.iter() {
            if let Some((k, v)) = entry.key_value() {
                f(k, v.value);
            }
        }
    }
}

impl Map for imhm::Map<Key, Value> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert_cow(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove_cow(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
}
impl Map for imhm::Map<Key, Value, std::collections::hash_map::RandomState> {
    fn empty() -> Self {
        Self::with_hasher(Default::default())
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert_cow(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove_cow(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
}
impl Map for papaya::HashMap<Key, Value> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.pin().get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.pin().insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.pin().remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.pin().iter() {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        let pin = self.pin();
        for op in ops {
            match op {
                Op::Put(k, v) => {
                    pin.insert(*k, *v);
                }
                Op::Delete(k) => {
                    pin.remove(k);
                }
            }
        }
    }
}
impl Map for dashmap::DashMap<Key, Value> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).map(|v| *v)
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for e in self.iter() {
            f(*e.key(), *e.value());
        }
    }
}
impl Map for flurry::HashMap<Key, Value> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k, &self.guard()).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert(k, v, &self.guard());
    }
    fn del(&mut self, k: &Key) {
        self.remove(k, &self.guard());
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter(&self.guard()) {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        let guard = self.guard();
        for op in ops {
            match op {
                Op::Put(k, v) => {
                    self.insert(*k, *v, &guard);
                }
                Op::Delete(k) => {
                    self.remove(k, &guard);
                }
            }
        }
    }
}

#[derive(Clone)]
struct ChunkBatch<const N: usize>(immutable_chunkmap::map::Map<Key, Value, N>);
impl<const N: usize> Map for ChunkBatch<N> {
    fn empty() -> Self {
        Self(immutable_chunkmap::map::Map::new())
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.0.insert_cow(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.0.remove_cow(k);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in &self.0 {
            f(*k, *v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        // Stable sort and last-write-wins coalescing are charged inside publication;
        // the original uncoalesced operations remain in the common change journal.
        let mut sorted: Vec<_> = ops
            .iter()
            .map(|op| match op {
                Op::Put(k, v) => (*k, Some(*v)),
                Op::Delete(k) => (*k, None),
            })
            .collect();
        sorted.sort_by_key(|(k, _)| *k);
        let mut unique: Vec<(Key, Option<Value>)> = Vec::with_capacity(sorted.len());
        for (k, v) in sorted {
            if let Some(last) = unique.last_mut().filter(|(prior, _)| *prior == k) {
                last.1 = v;
            } else {
                unique.push((k, v));
            }
        }
        self.0 = self.0.update_many(unique, |k, v, _| v.map(|v| (k, v)));
    }
}

#[derive(Clone)]
struct Vart(vart::art::Tree<vart::FixedSizeKey<8>, Value>);
impl Map for Vart {
    fn empty() -> Self {
        Self(vart::art::Tree::new())
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0
            .get(&vart::FixedSizeKey::from_slice(&k.to_be_bytes()), 0)
            .map(|(v, _, _)| v)
    }
    fn put(&mut self, k: Key, v: Value) {
        self.0
            .insert_or_replace(&vart::FixedSizeKey::from_slice(&k.to_be_bytes()), v, 0, 0)
            .unwrap();
    }
    fn del(&mut self, k: &Key) {
        self.0
            .remove(&vart::FixedSizeKey::from_slice(&k.to_be_bytes()));
    }
    fn len(&self) -> usize {
        self.0.size
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v, _, _) in self.0.iter() {
            f(u64::from_be_bytes(k.try_into().unwrap()), *v);
        }
    }
}

type Db = skipdb::optimistic::OptimisticDb<Key, Value>;
type ReadTx = skipdb::ReadTransaction<Key, Value, Db, txn::HashCm<Key>>;
#[derive(Clone)]
struct DbView {
    transaction: Arc<ReadTx>,
    len: usize,
    revision: u64,
}
impl ReadView for DbView {
    fn revision(&self) -> u64 {
        self.revision
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.transaction.get(k).map(|e| *e.value())
    }
    fn len(&self) -> usize {
        self.len
    }
    fn checksum(&self) -> u64 {
        self.transaction.iter().fold(0u64, |sum, e| {
            sum.wrapping_add(digest(*e.key(), *e.value()))
        })
    }
    fn contents(&self) -> BTreeMap<Key, Value> {
        self.transaction
            .iter()
            .map(|e| (*e.key(), *e.value()))
            .collect()
    }
}
struct SkipDb {
    db: Db,
    current: DbView,
    journal: Compared<HashMap<Key, Value>>,
}
impl SkipDb {
    fn new() -> Self {
        let db = Db::new();
        let current = DbView {
            transaction: Arc::new(db.read()),
            len: 0,
            revision: 0,
        };
        Self {
            db,
            current,
            journal: Compared::new(),
        }
    }
}
impl Engine for SkipDb {
    type View = DbView;
    fn publish(&mut self, ops: Vec<Op>) {
        let mut len = self.current.len;
        if !ops.is_empty() {
            let mut tx = self.db.write_with_capacity(ops.len());
            // Coalesce the state mutation, while preserving every original operation
            // in the journal. Avoid depending on transactional read-your-delete for
            // repeated keys; compute net live count from the pre-batch snapshot.
            let mut unique = BTreeMap::new();
            for op in &ops {
                match op {
                    Op::Put(k, _) | Op::Delete(k) => {
                        unique.insert(*k, op);
                    }
                }
            }
            for (k, op) in unique {
                let existed = self.current.get(&k).is_some();
                match op {
                    Op::Put(k, v) => {
                        if !existed {
                            len += 1;
                        }
                        tx.insert(*k, *v).unwrap();
                    }
                    Op::Delete(k) => {
                        if existed {
                            len -= 1;
                        }
                        tx.remove(*k).unwrap();
                    }
                }
            }
            tx.commit().unwrap();
        }
        if !ops.is_empty() {
            self.current.transaction = Arc::new(self.db.read());
        }
        self.current.len = len;
        self.current.revision += 1;
        // The common journal, without an extra map or hidden snapshot copy.
        self.journal.revision += 1;
        let operations: Arc<[Op]> = ops.into();
        self.journal.count += operations.len();
        self.journal
            .history
            .push_back((self.journal.revision, operations));
        while self.journal.history.len() > 64 || self.journal.count > 1_000_000 {
            self.journal.count -= self.journal.history.pop_front().unwrap().1.len();
        }
    }
    fn snapshot(&self) -> Self::View {
        self.current.clone()
    }
    fn feed(&self, after: u64) -> Vec<Arc<[Op]>> {
        self.journal.feed(after)
    }
    fn feed_summary(&self, after: u64) -> (usize, usize) {
        self.journal.feed_summary(after)
    }
    fn revision(&self) -> u64 {
        self.journal.revision
    }
}

/// Independent MVCC chains with version-bound snapshot pins and periodic collection.
/// All writes in a batch use one lock; readers select the last version <= their pin.
#[derive(Clone)]
struct Mvcc<const GC: usize> {
    shared: Arc<std::sync::RwLock<Chains>>,
    pin: Arc<u64>,
    len: usize,
}
struct Chains {
    entries: HashMap<Key, Vec<(u64, Option<Value>)>>,
    pins: Vec<std::sync::Weak<u64>>,
}
impl<const GC: usize> Map for Mvcc<GC> {
    fn empty() -> Self {
        let pin = Arc::new(0);
        Self {
            shared: Arc::new(std::sync::RwLock::new(Chains {
                entries: HashMap::new(),
                pins: vec![Arc::downgrade(&pin)],
            })),
            pin,
            len: 0,
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        let guard = self.shared.read().unwrap();
        let chain = guard.entries.get(k)?;
        let i = chain.partition_point(|(r, _)| r <= &*self.pin);
        if i == 0 {
            None
        } else {
            chain[i - 1].1
        }
    }
    fn put(&mut self, k: Key, v: Value) {
        self.batch(&[Op::Put(k, v)]);
    }
    fn del(&mut self, k: &Key) {
        self.batch(&[Op::Delete(*k)]);
    }
    fn len(&self) -> usize {
        self.len
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, chain) in &self.shared.read().unwrap().entries {
            let i = chain.partition_point(|(r, _)| r <= &*self.pin);
            if i > 0 {
                if let Some(v) = chain[i - 1].1 {
                    f(*k, v);
                }
            }
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        let rev = *self.pin + 1;
        let next = Arc::new(rev);
        let mut shared = self.shared.write().unwrap();
        shared.pins.push(Arc::downgrade(&next));
        for op in ops {
            let (k, v) = match op {
                Op::Put(k, v) => (*k, Some(*v)),
                Op::Delete(k) => (*k, None),
            };
            let chain = shared.entries.entry(k).or_default();
            let was = chain.last().is_some_and(|(_, v)| v.is_some());
            self.len += usize::from(v.is_some());
            self.len -= usize::from(was);
            chain.push((rev, v));
        }
        self.pin = next;
        if GC > 0 && rev % GC as u64 == 0 {
            let mut oldest = rev;
            shared.pins.retain(|p| {
                if let Some(p) = p.upgrade() {
                    oldest = oldest.min(*p);
                    true
                } else {
                    false
                }
            });
            shared.entries.retain(|_, chain| {
                let n = chain
                    .partition_point(|(r, _)| *r <= oldest)
                    .saturating_sub(1);
                if n > 0 {
                    chain.drain(..n);
                }
                !(chain.len() == 1 && chain[0].0 <= oldest && chain[0].1.is_none())
            });
        }
    }
}

impl<H: std::hash::BuildHasher + Clone + Default + Send + Sync> Map
    for rpds::HashTrieMapSync<Key, Value, H>
{
    fn empty() -> Self {
        Self::new_with_hasher_and_ptr_kind(H::default())
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert_mut(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove_mut(k);
    }
    fn len(&self) -> usize {
        self.size()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
}
impl Map for rpds::RedBlackTreeMapSync<Key, Value> {
    fn empty() -> Self {
        Self::new_sync()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert_mut(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove_mut(k);
    }
    fn len(&self) -> usize {
        self.size()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.iter() {
            f(*k, *v);
        }
    }
}
impl<const N: usize> Map for immutable_chunkmap::map::Map<Key, Value, N> {
    fn empty() -> Self {
        Self::new()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.insert_cow(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.remove_cow(k);
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for (k, v) in self.into_iter() {
            f(*k, *v);
        }
    }
}

/// Independent flat-page design: copy directory, COW only touched hash maps.
#[derive(Clone)]
struct HashPages<const N: usize> {
    pages: Vec<Arc<HashMap<Key, Value>>>,
    len: usize,
}

/// Independent indexed-slot design: reuse the key index across value-only updates;
/// COW packed value pages. Key churn clones the compact index once per batch.
#[derive(Clone)]
struct Slots<const N: usize> {
    index: Arc<hashbrown::HashMap<Key, usize>>,
    pages: Vec<Arc<Vec<Option<(Key, Value)>>>>,
    free: Vec<usize>,
    next: usize,
}
impl<const N: usize> Map for Slots<N> {
    fn empty() -> Self {
        Self {
            index: Arc::new(hashbrown::HashMap::new()),
            pages: Vec::new(),
            free: Vec::new(),
            next: 0,
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        let i = *self.index.get(k)?;
        self.pages[i / N][i % N].map(|(_, v)| v)
    }
    fn put(&mut self, k: Key, v: Value) {
        let i = if let Some(i) = self.index.get(&k) {
            *i
        } else {
            let i = self.free.pop().unwrap_or_else(|| {
                let i = self.next;
                self.next += 1;
                if i / N == self.pages.len() {
                    self.pages.push(Arc::new(vec![None; N]));
                }
                i
            });
            Arc::make_mut(&mut self.index).insert(k, i);
            i
        };
        Arc::make_mut(&mut self.pages[i / N])[i % N] = Some((k, v));
    }
    fn del(&mut self, k: &Key) {
        if let Some(i) = self.index.get(k).copied() {
            Arc::make_mut(&mut self.index).remove(k);
            Arc::make_mut(&mut self.pages[i / N])[i % N] = None;
            self.free.push(i);
        }
    }
    fn len(&self) -> usize {
        self.index.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for page in &self.pages {
            for (k, v) in page.iter().flatten() {
                f(*k, *v);
            }
        }
    }
}
impl<const N: usize> Map for HashPages<N> {
    fn empty() -> Self {
        Self {
            pages: vec![Arc::new(HashMap::new()); N],
            len: 0,
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.pages[page_hash(*k) & (N - 1)].get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        let p = Arc::make_mut(&mut self.pages[page_hash(k) & (N - 1)]);
        if p.insert(k, v).is_none() {
            self.len += 1;
        }
    }
    fn del(&mut self, k: &Key) {
        if Arc::make_mut(&mut self.pages[page_hash(*k) & (N - 1)])
            .remove(k)
            .is_some()
        {
            self.len -= 1;
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for p in &self.pages {
            for (k, v) in p.iter() {
                f(*k, *v);
            }
        }
    }
}
fn page_hash(x: u64) -> usize {
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    (z ^ (z >> 31)) as usize
}

#[derive(Clone)]
struct SortedPages<const N: usize> {
    pages: Vec<Arc<Vec<(Key, Value)>>>,
    len: usize,
}
impl<const N: usize> Map for SortedPages<N> {
    fn empty() -> Self {
        Self {
            pages: vec![Arc::new(Vec::new()); N],
            len: 0,
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        let p = &self.pages[page_hash(*k) & (N - 1)];
        p.binary_search_by_key(k, |(k, _)| *k).ok().map(|i| p[i].1)
    }
    fn put(&mut self, k: Key, v: Value) {
        let p = Arc::make_mut(&mut self.pages[page_hash(k) & (N - 1)]);
        match p.binary_search_by_key(&k, |(k, _)| *k) {
            Ok(i) => p[i].1 = v,
            Err(i) => {
                p.insert(i, (k, v));
                self.len += 1;
            }
        }
    }
    fn del(&mut self, k: &Key) {
        let p = Arc::make_mut(&mut self.pages[page_hash(*k) & (N - 1)]);
        if let Ok(i) = p.binary_search_by_key(k, |(k, _)| *k) {
            p.remove(i);
            self.len -= 1;
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for p in &self.pages {
            for (k, v) in p.iter() {
                f(*k, *v);
            }
        }
    }
}

/// Independent log-over-base design: writes append, reads search newest overlays;
/// periodically compact to an immutable hash map. Retained views pin old chains.
#[derive(Clone)]
struct Overlay<const LIMIT: usize> {
    base: Arc<HashMap<Key, Value>>,
    layers: Vec<Arc<Vec<Op>>>,
    len: usize,
}
impl<const LIMIT: usize> Map for Overlay<LIMIT> {
    fn empty() -> Self {
        Self {
            base: Arc::new(HashMap::new()),
            layers: Vec::new(),
            len: 0,
        }
    }
    fn get(&self, k: &Key) -> Option<Value> {
        for layer in self.layers.iter().rev() {
            for op in layer.iter().rev() {
                match op {
                    Op::Put(key, v) if key == k => return Some(*v),
                    Op::Delete(key) if key == k => return None,
                    _ => {}
                }
            }
        }
        self.base.get(k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.batch(&[Op::Put(k, v)]);
    }
    fn del(&mut self, k: &Key) {
        self.batch(&[Op::Delete(*k)]);
    }
    fn len(&self) -> usize {
        self.len
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        let mut m = (*self.base).clone();
        for l in &self.layers {
            for op in l.iter() {
                match op {
                    Op::Put(k, v) => {
                        m.insert(*k, *v);
                    }
                    Op::Delete(k) => {
                        m.remove(k);
                    }
                }
            }
        }
        for (k, v) in m {
            f(k, v);
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        // Exact live count, including duplicate-key mutations.
        let mut staged = HashMap::new();
        for op in ops {
            let (k, v) = match op {
                Op::Put(k, v) => (*k, Some(*v)),
                Op::Delete(k) => (*k, None),
            };
            let prior = staged.get(&k).copied().unwrap_or_else(|| self.get(&k));
            self.len += usize::from(v.is_some());
            self.len -= usize::from(prior.is_some());
            staged.insert(k, v);
        }
        self.layers.push(Arc::new(ops.to_vec()));
        if self.layers.len() >= LIMIT {
            let base = Arc::make_mut(&mut self.base);
            for l in self.layers.drain(..) {
                for op in l.iter() {
                    match op {
                        Op::Put(k, v) => {
                            base.insert(*k, *v);
                        }
                        Op::Delete(k) => {
                            base.remove(k);
                        }
                    }
                }
            }
        }
    }
}

struct Options {
    engine: String,
    case: String,
    seed: u64,
    records: usize,
    readers: usize,
    rounds: usize,
    verify: bool,
}
fn options() -> Options {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let get = |key: &str, default: &str| {
        args.windows(2)
            .find(|p| p[0] == key)
            .map(|p| p[1].clone())
            .unwrap_or_else(|| default.to_owned())
    };
    Options {
        engine: get("--engine", "rill-4096-64"),
        case: get("--case", "sparse"),
        seed: get("--seed", "4815162342").parse().unwrap(),
        records: get("--records", "20000").parse().unwrap(),
        readers: get("--readers", "20000").parse().unwrap(),
        rounds: get("--rounds", "32").parse().unwrap(),
        verify: args.iter().any(|a| a == "--verify"),
    }
}
fn trace(o: &Options) -> Vec<Vec<Op>> {
    let mut seed = o.seed;
    let count = match o.case.as_str() {
        "idle" | "read" | "scan" | "acquire" | "miss10" | "miss50" | "miss100" => 0,
        "single" => 1,
        "sparse" | "hot" | "churn" | "slow" | "feed" => o.records / 100,
        "burst" => o.records / 2,
        "replace" => o.records,
        _ => panic!("unknown case"),
    };
    (0..o.rounds)
        .map(|r| {
            (0..count)
                .map(|i| {
                    let raw = if o.case == "hot" {
                        rng(&mut seed) % (o.records.max(100) / 100) as u64
                    } else if o.case == "replace" {
                        i as u64
                    } else {
                        rng(&mut seed) % o.records as u64
                    };
                    let k = key(raw);
                    if o.case == "churn" && i % 2 == 0 {
                        Op::Delete(k)
                    } else {
                        Op::Put(k, value(k, r as u64 + 1))
                    }
                })
                .collect()
        })
        .collect()
}
fn run<E: Engine>(mut engine: E, o: &Options) {
    let diagnostic = std::env::var_os("RILL_DIAGNOSTIC").is_some();
    if diagnostic {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(10));
            eprintln!("diagnostic deadline reached");
            std::process::exit(124);
        });
    }
    let phase = |s: &str| {
        if diagnostic {
            eprintln!("phase: {s}");
        }
    };
    assert!(o.records > 0 && o.rounds > 0);
    let initial: Vec<_> = (0..o.records as u64)
        .map(|k| {
            let k = key(k);
            Op::Put(k, value(k, 0))
        })
        .collect();
    phase("initial publish start");
    engine.publish(initial.clone());
    phase("initial publish complete");
    // Warm the actual publication and lookup code on this process, then restore
    // initial values. No warm-up operation is included in the reported interval.
    for r in 0..8 {
        if diagnostic {
            eprintln!("warmup {r}");
        }
        engine.publish(
            (0..o.records.min(128) as u64)
                .map(|k| {
                    let k = key(k);
                    Op::Put(k, value(k, r + 1))
                })
                .collect(),
        );
        let s = engine.snapshot();
        for k in 0..o.records.min(128) as u64 {
            black_box(s.get(&key(k)));
        }
    }
    engine.publish(initial.clone());
    let before = engine.snapshot();
    let input = trace(o);
    let mut seed = o.seed ^ 0x1234;
    let reads: Vec<_> = (0..o.readers)
        .map(|i| {
            let raw = rng(&mut seed) % o.records as u64;
            let percent = match o.case.as_str() {
                "miss10" => 10,
                "miss50" => 50,
                "miss100" => 100,
                _ => 0,
            };
            key(raw
                + if i % 100 < percent {
                    o.records as u64
                } else {
                    0
                })
        })
        .collect();
    if std::env::args().any(|a| a == "--thread-check") {
        let mut expected = BTreeMap::new();
        for op in &initial {
            if let Op::Put(k, v) = op {
                expected.insert(*k, *v);
            }
        }
        let old_expected = expected.clone();
        let barrier = std::sync::Barrier::new(5);
        std::thread::scope(|scope| {
            for worker in 0..4 {
                let old = before.clone();
                let reads = &reads;
                let old_expected = &old_expected;
                let barrier = &barrier;
                scope.spawn(move || {
                    let revision = old.revision();
                    barrier.wait();
                    for _ in 0..o.rounds {
                        for k in reads.iter().skip(worker).step_by(4) {
                            assert_eq!(old.get(k), old_expected.get(k).copied());
                        }
                        assert_eq!(old.revision(), revision);
                    }
                });
            }
            barrier.wait();
            for ops in input {
                for op in &ops {
                    match op {
                        Op::Put(k, v) => {
                            expected.insert(*k, *v);
                        }
                        Op::Delete(k) => {
                            expected.remove(k);
                        }
                    }
                }
                engine.publish(ops);
            }
        });
        assert_eq!(engine.snapshot().contents(), expected);
        println!("{{\"engine\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"verified\":true,\"threads\":4,\"thread_checked\":true}}",o.engine,o.case,o.seed,o.records,o.readers,o.rounds);
        return;
    }

    if o.verify {
        phase("oracle start");
        let mut truth = BTreeMap::new();
        for op in initial {
            if let Op::Put(k, v) = op {
                truth.insert(k, v);
            }
        }
        assert_eq!(before.revision(), engine.revision());
        let mut old = vec![(before.clone(), truth.clone(), before.revision())];
        let mut after = engine.revision();
        for ops in input {
            for op in &ops {
                match op {
                    Op::Put(k, v) => {
                        truth.insert(*k, *v);
                    }
                    Op::Delete(k) => {
                        truth.remove(k);
                    }
                }
            }
            engine.publish(ops.clone());
            phase("oracle published");
            let batches = engine.feed(after);
            assert_eq!(batches.len(), 1);
            assert_eq!(&*batches[0], &ops);
            after = engine.revision();
            let snap = engine.snapshot();
            phase("oracle snapshot");
            assert_eq!(snap.revision(), after);
            phase("oracle length start");
            assert_eq!(snap.len(), truth.len());
            phase("oracle length complete");
            assert_eq!(snap.contents(), truth);
            phase("oracle contents");
            for k in &reads {
                assert_eq!(snap.get(k), truth.get(k).copied());
            }
            phase("oracle point reads");
            for (view, expected, revision) in &old {
                assert_eq!(view.revision(), *revision);
                assert_eq!(view.contents(), *expected);
            }
            if old.len() < 5 {
                old.push((snap, truth.clone(), after));
            }
        }
        println!("{{\"engine\":\"{}\",\"case\":\"{}\",\"seed\":{},\"verified\":true,\"records\":{},\"readers\":{},\"rounds\":{}}}",o.engine,o.case,o.seed,o.records,o.readers,o.rounds);
        return;
    }
    // Initialization, warm-up and trace/read-key generation are outside this interval.
    let mut elapsed = Vec::new();
    let mut publication = 0u128;
    let mut reading = 0u128;
    let mut sum = 0u64;
    let mut held = Vec::new();
    let wall = Instant::now();
    for ops in input {
        let frame = Instant::now();
        let after = engine.revision();
        let start = Instant::now();
        engine.publish(ops);
        publication += start.elapsed().as_nanos();
        let start = Instant::now();
        let snap = engine.snapshot();
        if o.case == "scan" {
            sum = sum.wrapping_add(black_box(snap.checksum()));
        } else if o.case == "acquire" {
            for _ in 0..o.readers {
                black_box(engine.snapshot());
            }
        } else if o.case == "feed" {
            for _ in 0..o.readers {
                let (n, ops) = black_box(engine.feed_summary(black_box(after)));
                sum = sum.wrapping_add((n + ops) as u64);
            }
        } else if o.case != "idle" {
            for k in &reads {
                sum = sum.wrapping_add(
                    black_box(snap.get(black_box(k))).map_or(0, |v| v[0] ^ v[1] ^ v[2] ^ v[3]),
                );
            }
        }
        if o.case == "slow" {
            held.push(snap);
            if held.len() > 16 {
                held.remove(0);
            }
        }
        reading += start.elapsed().as_nanos();
        elapsed.push(frame.elapsed().as_nanos());
    }
    black_box(before);
    black_box(held);
    let total = wall.elapsed().as_nanos();
    let final_view = engine.snapshot();
    let checksum = final_view.checksum();
    elapsed.sort_unstable();
    println!("{{\"engine\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"total_ns\":{},\"publish_ns\":{},\"read_ns\":{},\"p50_ns\":{},\"p99_ns\":{},\"checksum\":{},\"read_checksum\":{},\"final_len\":{}}}",o.engine,o.case,o.seed,o.records,o.readers,o.rounds,total,publication,reading,elapsed[elapsed.len()/2],elapsed[(elapsed.len()*99/100).min(elapsed.len()-1)],checksum,sum,final_view.len());
}

fn main() {
    let o = options();
    if o.engine == "leapfrog-native-counter" {
        let mut map = leapfrog::HashMap::<u64, u64>::new();
        map.insert(1, 7);
        map.remove(&1);
        let prior = map.insert(1, 9);
        let prior_json = prior.map_or_else(|| "null".to_owned(), |v| v.to_string());
        println!("{{\"diagnostic\":\"upstream leapfrog 0.3.3 delete/reinsert metadata\",\"reported_len\":{},\"iterated_len\":{},\"lookup\":{},\"returned_old\":{},\"contract_passed\":{}}}",map.len(),map.iter().count(),map.get(&1).copied().unwrap_or(0),prior_json,map.len()==map.iter().count()&&prior.is_none());
        return;
    }
    if o.engine.starts_with("rillfast-") {
        let parts: Vec<_> = o.engine.split('-').collect();
        let partitions = parts[1].parse().unwrap();
        let branch_size = parts[2].parse().unwrap();
        let store = Store::with_hasher(
            Config {
                partitions,
                branch_size,
                ..Config::default()
            },
            hashbrown::DefaultHashBuilder::default(),
        )
        .unwrap();
        let zero = store.snapshot().cursor();
        run(
            Rill {
                store,
                cursors: vec![zero],
            },
            &o,
        );
        return;
    }
    if o.engine.starts_with("rill-") {
        let parts: Vec<_> = o.engine.split('-').collect();
        let partitions = parts[1].parse().unwrap();
        let branch_size = parts[2].parse().unwrap();
        let store = Store::new(Config {
            partitions,
            branch_size,
            ..Config::default()
        })
        .unwrap();
        let zero = store.snapshot().cursor();
        run(
            Rill {
                store,
                cursors: vec![zero],
            },
            &o,
        );
        return;
    }
    if o.engine == "skipdb" {
        run(SkipDb::new(), &o);
        return;
    }
    macro_rules! dispatch {
        ($ty:ty) => {
            run(Compared::<$ty>::new(), &o)
        };
    }
    match o.engine.as_str() {
        "scc-copy" => dispatch!(scc::HashMap<Key,Value>),
        "array-copy-25-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,16,rill_state::experimental::CopyKeys>)
        }
        "array-copy-25-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,64,rill_state::experimental::CopyKeys>)
        }
        "array-copy-25-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,256,rill_state::experimental::CopyKeys>)
        }
        "array-copy-25-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,1024,rill_state::experimental::CopyKeys>)
        }
        "array-copy-50-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,16,rill_state::experimental::CopyKeys>)
        }
        "array-copy-50-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,64,rill_state::experimental::CopyKeys>)
        }
        "array-copy-50-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,256,rill_state::experimental::CopyKeys>)
        }
        "array-copy-50-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,1024,rill_state::experimental::CopyKeys>)
        }
        "array-copy-70-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16,rill_state::experimental::CopyKeys>)
        }
        "array-copy-70-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,64,rill_state::experimental::CopyKeys>)
        }
        "array-copy-70-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,256,rill_state::experimental::CopyKeys>)
        }
        "array-copy-70-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,1024,rill_state::experimental::CopyKeys>)
        }
        "array-copy-85-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,16,rill_state::experimental::CopyKeys>)
        }
        "array-copy-85-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,64,rill_state::experimental::CopyKeys>)
        }
        "array-copy-85-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,256,rill_state::experimental::CopyKeys>)
        }
        "array-copy-85-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,1024,rill_state::experimental::CopyKeys>)
        }
        "array-shared-25-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,16,rill_state::experimental::SharedKeys>)
        }
        "array-shared-25-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,64,rill_state::experimental::SharedKeys>)
        }
        "array-shared-25-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,256,rill_state::experimental::SharedKeys>)
        }
        "array-shared-25-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,25,1024,rill_state::experimental::SharedKeys>)
        }
        "array-shared-50-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,16,rill_state::experimental::SharedKeys>)
        }
        "array-shared-50-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,64,rill_state::experimental::SharedKeys>)
        }
        "array-shared-50-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,256,rill_state::experimental::SharedKeys>)
        }
        "array-shared-50-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,1024,rill_state::experimental::SharedKeys>)
        }
        "array-shared-70-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16,rill_state::experimental::SharedKeys>)
        }
        "array-shared-70-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,64,rill_state::experimental::SharedKeys>)
        }
        "array-shared-70-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,256,rill_state::experimental::SharedKeys>)
        }
        "array-shared-70-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,1024,rill_state::experimental::SharedKeys>)
        }
        "array-shared-85-16" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,16,rill_state::experimental::SharedKeys>)
        }
        "array-shared-85-64" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,64,rill_state::experimental::SharedKeys>)
        }
        "array-shared-85-256" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,256,rill_state::experimental::SharedKeys>)
        }
        "array-shared-85-1024" => {
            dispatch!(rill_state::experimental::ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,1024,rill_state::experimental::SharedKeys>)
        }
        "scc-capture" => dispatch!(SccCapture),
        "flashmap-copy" => dispatch!(FlashCopy),
        "evmap-copy" => dispatch!(EvmapCopy),
        "left-right-copy" => dispatch!(LeftCopy),
        "contrie-copy" => dispatch!(contrie::ConMap<Key,Value>),
        "leapfrog-copy" => dispatch!(Leap),
        "leapmap-copy" => dispatch!(LeapConcurrent),
        "probe-25-0" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,25,0>)
        }
        "probe-25-16" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,25,16>)
        }
        "probe-25-64" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,25,64>)
        }
        "probe-25-256" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,25,256>)
        }
        "probe-25-1024" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,25,1024>)
        }
        "split-25" => {
            dispatch!(rill_state::experimental::SplitMap<Key,Value,hashbrown::DefaultHashBuilder,25>)
        }
        "probe-50-0" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,50,0>)
        }
        "probe-50-16" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,50,16>)
        }
        "probe-50-64" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,50,64>)
        }
        "probe-50-256" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,50,256>)
        }
        "probe-50-1024" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,50,1024>)
        }
        "split-50" => {
            dispatch!(rill_state::experimental::SplitMap<Key,Value,hashbrown::DefaultHashBuilder,50>)
        }
        "probe-70-0" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,70,0>)
        }
        "probe-70-16" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>)
        }
        "probe-70-64" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,70,64>)
        }
        "probe-70-256" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,70,256>)
        }
        "probe-70-1024" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,70,1024>)
        }
        "split-70" => {
            dispatch!(rill_state::experimental::SplitMap<Key,Value,hashbrown::DefaultHashBuilder,70>)
        }
        "probe-85-0" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,85,0>)
        }
        "probe-85-16" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,85,16>)
        }
        "probe-85-64" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,85,64>)
        }
        "probe-85-256" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,85,256>)
        }
        "probe-85-1024" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,85,1024>)
        }
        "split-85" => {
            dispatch!(rill_state::experimental::SplitMap<Key,Value,hashbrown::DefaultHashBuilder,85>)
        }
        "probe-90-0" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,90,0>)
        }
        "probe-90-16" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,90,16>)
        }
        "probe-90-64" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,90,64>)
        }
        "probe-90-256" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,90,256>)
        }
        "probe-90-1024" => {
            dispatch!(rill_state::experimental::ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,90,1024>)
        }
        "split-90" => {
            dispatch!(rill_state::experimental::SplitMap<Key,Value,hashbrown::DefaultHashBuilder,90>)
        }
        "std-hash" => dispatch!(HashMap<Key,Value>),
        "std-btree" => dispatch!(BTreeMap<Key,Value>),
        "hashbrown" => dispatch!(hashbrown::HashMap<Key,Value>),
        "hashbrown-sip" => {
            dispatch!(hashbrown::HashMap<Key,Value,std::collections::hash_map::RandomState>)
        }
        "indexmap" => dispatch!(indexmap::IndexMap<Key,Value>),
        "btree-slab" => dispatch!(btree_slab::BTreeMap<Key,Value>),
        "imhm" => dispatch!(imhm::Map<Key,Value>),
        "imhm-sip" => dispatch!(imhm::Map<Key,Value,std::collections::hash_map::RandomState>),
        "mvcc-1" => dispatch!(Mvcc<1>),
        "mvcc-16" => dispatch!(Mvcc<16>),
        "mvcc-64" => dispatch!(Mvcc<64>),
        "mvcc-256" => dispatch!(Mvcc<256>),
        "slots-8" => dispatch!(Slots<8>),
        "slots-16" => dispatch!(Slots<16>),
        "slots-32" => dispatch!(Slots<32>),
        "slots-64" => dispatch!(Slots<64>),
        "slots-128" => dispatch!(Slots<128>),
        "slots-256" => dispatch!(Slots<256>),
        "slots-512" => dispatch!(Slots<512>),
        "slots-1024" => dispatch!(Slots<1024>),
        "papaya-copy" => dispatch!(papaya::HashMap<Key,Value>),
        "dashmap-copy" => dispatch!(dashmap::DashMap<Key,Value>),
        "flurry-copy" => dispatch!(flurry::HashMap<Key,Value>),
        "vart" => dispatch!(Vart),
        "im-hash" => dispatch!(im::HashMap<Key,Value>),
        "record-50-0" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,0>)
        }
        "record-50-16" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,16>)
        }
        "record-50-64" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,64>)
        }
        "record-50-256" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,50,256>)
        }
        "record-70-0" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,0>)
        }
        "record-70-16" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>)
        }
        "record-70-64" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,64>)
        }
        "record-70-256" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,256>)
        }
        "record-85-0" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,0>)
        }
        "record-85-16" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,16>)
        }
        "record-85-64" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,64>)
        }
        "record-85-256" => {
            dispatch!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,85,256>)
        }
        "shared-50-0" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,50,0>)
        }
        "shared-50-16" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,50,16>)
        }
        "shared-50-64" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,50,64>)
        }
        "shared-50-256" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,50,256>)
        }
        "shared-70-0" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,70,0>)
        }
        "shared-70-16" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>)
        }
        "shared-70-64" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,70,64>)
        }
        "shared-70-256" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,70,256>)
        }
        "shared-85-0" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,85,0>)
        }
        "shared-85-16" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,85,16>)
        }
        "shared-85-64" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,85,64>)
        }
        "shared-85-256" => {
            dispatch!(rill_state::experimental::SharedMap<Key,Value,hashbrown::DefaultHashBuilder,85,256>)
        }
        "im-ord" => dispatch!(im::OrdMap<Key,Value>),
        "imbl-hash" => dispatch!(imbl::HashMap<Key,Value>),
        "imbl-fast" => dispatch!(ImblFast<Key,Value>),
        "im-fast" => dispatch!(im::HashMap<Key,Value,hashbrown::DefaultHashBuilder>),
        "rpds-fast" => dispatch!(rpds::HashTrieMapSync<Key,Value,hashbrown::DefaultHashBuilder>),
        "imbl-ord" => dispatch!(imbl::OrdMap<Key,Value>),
        "rpds-hash" => dispatch!(rpds::HashTrieMapSync<Key,Value>),
        "rpds-redblack" => dispatch!(rpds::RedBlackTreeMapSync<Key,Value>),
        "chunk-8" => dispatch!(immutable_chunkmap::map::Map<Key,Value,8>),
        "chunk-16" => dispatch!(immutable_chunkmap::map::Map<Key,Value,16>),
        "chunk-32" => dispatch!(immutable_chunkmap::map::Map<Key,Value,32>),
        "chunk-64" => dispatch!(immutable_chunkmap::map::Map<Key,Value,64>),
        "chunk-128" => dispatch!(immutable_chunkmap::map::Map<Key,Value,128>),
        "chunkbatch-8" => dispatch!(ChunkBatch<8>),
        "chunkbatch-16" => dispatch!(ChunkBatch<16>),
        "chunkbatch-32" => dispatch!(ChunkBatch<32>),
        "chunkbatch-64" => dispatch!(ChunkBatch<64>),
        "chunkbatch-128" => dispatch!(ChunkBatch<128>),
        "hashpages-16" => dispatch!(HashPages<16>),
        "hashpages-64" => dispatch!(HashPages<64>),
        "hashpages-256" => dispatch!(HashPages<256>),
        "hashpages-1024" => dispatch!(HashPages<1024>),
        "hashpages-4096" => dispatch!(HashPages<4096>),
        "hashpages-16384" => dispatch!(HashPages<16384>),
        "sortedpages-16" => dispatch!(SortedPages<16>),
        "sortedpages-64" => dispatch!(SortedPages<64>),
        "sortedpages-256" => dispatch!(SortedPages<256>),
        "sortedpages-1024" => dispatch!(SortedPages<1024>),
        "sortedpages-4096" => dispatch!(SortedPages<4096>),
        "sortedpages-16384" => dispatch!(SortedPages<16384>),
        "overlay-1" => dispatch!(Overlay<1>),
        "overlay-2" => dispatch!(Overlay<2>),
        "overlay-4" => dispatch!(Overlay<4>),
        "overlay-8" => dispatch!(Overlay<8>),
        "overlay-16" => dispatch!(Overlay<16>),
        "overlay-32" => dispatch!(Overlay<32>),
        _ => panic!("unknown engine {}", o.engine),
    }
}
