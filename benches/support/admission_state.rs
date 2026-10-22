//! Native guarded lookups copy only the original 32-byte scalar value.
//! No shadow map, extra padded value, unsafe lifetime extension or materialized scan cache.
use super::*;
use std::cell::RefCell;

fn coalesced(ops: &[Op]) -> BTreeMap<Key, Option<Value>> {
    let mut final_ops = BTreeMap::new();
    for op in ops {
        match op {
            Op::Put(k, v) => final_ops.insert(*k, Some(*v)),
            Op::Delete(k) => final_ops.insert(*k, None),
        };
    }
    final_ops
}

struct Chash<const REINSERT: bool, const COPY_SCAN: bool, const LAST: bool>(
    chashmap::CHashMap<Key, Value>,
);
impl<const R: bool, const S: bool, const L: bool> Clone for Chash<R, S, L> {
    fn clone(&self) -> Self {
        if R {
            let next = chashmap::CHashMap::with_capacity(self.0.len());
            // Published backends are immutable. Native retain visits every
            // binding under its bucket lock; true leaves the old table unchanged.
            self.0.retain(|k, v| {
                next.insert(*k, *v);
                true
            });
            Self(next)
        } else {
            Self(self.0.clone())
        }
    }
}
impl<const R: bool, const S: bool, const L: bool> Map for Chash<R, S, L> {
    fn empty() -> Self {
        Self(chashmap::CHashMap::new())
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.get(k).map(|guard| *guard)
    }
    fn put(&mut self, k: Key, v: Value) {
        self.0.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.0.remove(k);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn visit(&self, f: impl FnMut(Key, Value)) {
        if S {
            let mut f = f;
            for (k, v) in self.0.clone() {
                f(k, v);
            }
        } else {
            // The native visitor accepts Fn, while the shared oracle accepts
            // FnMut. This stack-local bridge does not clone keys or values.
            let visitor = RefCell::new(f);
            self.0.retain(|k, v| {
                visitor.borrow_mut()(*k, *v);
                true
            });
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        if L {
            for (k, value) in coalesced(ops) {
                match value {
                    Some(v) => self.put(k, v),
                    None => self.del(&k),
                }
            }
        } else {
            for op in ops {
                match op {
                    Op::Put(k, v) => self.put(*k, *v),
                    Op::Delete(k) => self.del(k),
                }
            }
        }
    }
}

struct Skip<const LAST: bool>(crossbeam_skiplist::SkipMap<Key, Value>);
impl<const L: bool> Clone for Skip<L> {
    fn clone(&self) -> Self {
        let next = crossbeam_skiplist::SkipMap::new();
        for entry in &self.0 {
            next.insert(*entry.key(), *entry.value());
        }
        Self(next)
    }
}
impl<const L: bool> Map for Skip<L> {
    fn empty() -> Self {
        Self(crossbeam_skiplist::SkipMap::new())
    }
    fn get(&self, k: &Key) -> Option<Value> {
        self.0.get(k).map(|entry| *entry.value())
    }
    fn put(&mut self, k: Key, v: Value) {
        self.0.insert(k, v);
    }
    fn del(&mut self, k: &Key) {
        self.0.remove(k);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        for entry in &self.0 {
            f(*entry.key(), *entry.value());
        }
    }
    fn batch(&mut self, ops: &[Op]) {
        if L {
            for (k, value) in coalesced(ops) {
                match value {
                    Some(v) => self.put(k, v),
                    None => self.del(&k),
                }
            }
        } else {
            for op in ops {
                match op {
                    Op::Put(k, v) => self.put(*k, *v),
                    Op::Delete(k) => self.del(k),
                }
            }
        }
    }
}

pub fn entry() {
    let o = options();
    macro_rules! map {
        ($ty:ty) => {
            run(Compared::<$ty>::new(), &o)
        };
    }
    macro_rules! adaptive {
        ($hasher:ty,$flat:ty) => {{
            type Candidate = rill_state::experimental::AdaptiveMap<
                Key,
                Value,
                $hasher,
                1,
                3,
                true,
                16,
                $flat,
                true,
            >;
            run(Compared::<Candidate, NativeRoot<Candidate>>::new(), &o)
        }};
    }
    use rill_state::experimental::{InlineFlat, SharedFlat};
    use std::collections::hash_map::RandomState;
    match o.engine.as_str() {
        "chashmap-native-retain-each" => map!(Chash<false, false, false>),
        "chashmap-native-retain-last" => map!(Chash<false, false, true>),
        "chashmap-native-copy-each" => map!(Chash<false, true, false>),
        "chashmap-native-copy-last" => map!(Chash<false, true, true>),
        "chashmap-reinsert-retain-each" => map!(Chash<true, false, false>),
        "chashmap-reinsert-retain-last" => map!(Chash<true, false, true>),
        "chashmap-reinsert-copy-each" => map!(Chash<true, true, false>),
        "chashmap-reinsert-copy-last" => map!(Chash<true, true, true>),
        "crossbeam-skiplist-each" => map!(Skip<false>),
        "crossbeam-skiplist-last" => map!(Skip<true>),
        "hashbrown" => map!(hashbrown::HashMap<Key, Value>),
        "hashbrown-sip" => map!(hashbrown::HashMap<Key, Value, RandomState>),
        "std-hash" => map!(HashMap<Key, Value>),
        "std-fast" => map!(HashMap<Key, Value, hashbrown::DefaultHashBuilder>),
        "std-btree" => map!(BTreeMap<Key, Value>),
        "imbl-fast" => map!(ImblFast<Key, Value>),
        "rpds-fast" => map!(rpds::HashTrieMapSync<Key, Value, hashbrown::DefaultHashBuilder>),
        "record-paged16" => {
            map!(rill_state::experimental::RecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 16>)
        }
        "adaptive-inline-fast-1-3-drop-16-reset" => {
            adaptive!(hashbrown::DefaultHashBuilder, InlineFlat)
        }
        "adaptive-cache-fast-1-3-drop-16-reset" => {
            adaptive!(hashbrown::DefaultHashBuilder, SharedFlat<2>)
        }
        "adaptive-inline-sip-1-3-drop-16-reset" => adaptive!(RandomState, InlineFlat),
        "adaptive-cache-sip-1-3-drop-16-reset" => adaptive!(RandomState, SharedFlat<2>),
        _ => panic!("engine not registered in admission panel"),
    }
}
