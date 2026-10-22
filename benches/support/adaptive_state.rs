//! Generated fixed policies. Reuses the parent state/revision/feed/trace/oracle harness.
use super::*;
use rill_state::experimental::{AdaptiveMap, FlatMode, InlineFlat, SharedFlat};
impl<
        S: BuildHasher + Clone + Default + Send + Sync,
        const T: usize,
        const D: usize,
        const B: bool,
        const P: usize,
        F: FlatMode<Key, Value, S> + Send + Sync,
        const R: bool,
    > Map for AdaptiveMap<Key, Value, S, T, D, B, P, F, R>
where
    F::Data: Send + Sync,
{
    fn empty() -> Self {
        Self::default()
    }
    fn get(&self, k: &Key) -> Option<Value> {
        AdaptiveMap::get(self, k).copied()
    }
    fn put(&mut self, k: Key, v: Value) {
        self.apply(&[Mutation::Put(k, v)]);
    }
    fn del(&mut self, k: &Key) {
        self.apply(&[Mutation::Delete(*k)]);
    }
    fn len(&self) -> usize {
        AdaptiveMap::len(self)
    }
    fn visit(&self, mut f: impl FnMut(Key, Value)) {
        AdaptiveMap::visit(self, |k, v| f(*k, *v));
    }
    fn batch(&mut self, ops: &[Mutation<Key, Value>]) {
        self.apply(ops);
    }
}
fn policy<
    S: BuildHasher + Clone + Default + Send + Sync,
    F: FlatMode<Key, Value, S> + Send + Sync,
>(
    o: &Options,
    p: &[&str],
) where
    F::Data: Send + Sync,
{
    let threshold = p[2].parse::<usize>().unwrap();
    let delay = p[3].parse::<usize>().unwrap();
    let gate = match p[4] {
        "any" => false,
        "drop" => true,
        _ => panic!("invalid gate"),
    };
    let page = p[5].parse::<usize>().unwrap();
    let reset = match p[6] {
        "oneway" => false,
        "reset" => true,
        _ => panic!("invalid direction policy"),
    };
    macro_rules! call {
        ($t:literal,$d:literal,$b:literal,$p:literal,$r:literal) => {
            run(
                Compared::<
                    AdaptiveMap<Key, Value, S, $t, $d, $b, $p, F, $r>,
                    NativeRoot<AdaptiveMap<Key, Value, S, $t, $d, $b, $p, F, $r>>,
                >::new(),
                o,
            )
        };
    }
    match (threshold, delay, gate, page, reset) {
        (1, 1, false, 16, false) => call!(1, 1, false, 16, false),
        (1, 1, false, 16, true) => call!(1, 1, false, 16, true),
        (1, 1, false, 64, false) => call!(1, 1, false, 64, false),
        (1, 1, false, 64, true) => call!(1, 1, false, 64, true),
        (1, 1, true, 16, false) => call!(1, 1, true, 16, false),
        (1, 1, true, 16, true) => call!(1, 1, true, 16, true),
        (1, 1, true, 64, false) => call!(1, 1, true, 64, false),
        (1, 1, true, 64, true) => call!(1, 1, true, 64, true),
        (1, 3, false, 16, false) => call!(1, 3, false, 16, false),
        (1, 3, false, 16, true) => call!(1, 3, false, 16, true),
        (1, 3, false, 64, false) => call!(1, 3, false, 64, false),
        (1, 3, false, 64, true) => call!(1, 3, false, 64, true),
        (1, 3, true, 16, false) => call!(1, 3, true, 16, false),
        (1, 3, true, 16, true) => call!(1, 3, true, 16, true),
        (1, 3, true, 64, false) => call!(1, 3, true, 64, false),
        (1, 3, true, 64, true) => call!(1, 3, true, 64, true),
        (10, 1, false, 16, false) => call!(10, 1, false, 16, false),
        (10, 1, false, 16, true) => call!(10, 1, false, 16, true),
        (10, 1, false, 64, false) => call!(10, 1, false, 64, false),
        (10, 1, false, 64, true) => call!(10, 1, false, 64, true),
        (10, 1, true, 16, false) => call!(10, 1, true, 16, false),
        (10, 1, true, 16, true) => call!(10, 1, true, 16, true),
        (10, 1, true, 64, false) => call!(10, 1, true, 64, false),
        (10, 1, true, 64, true) => call!(10, 1, true, 64, true),
        (10, 3, false, 16, false) => call!(10, 3, false, 16, false),
        (10, 3, false, 16, true) => call!(10, 3, false, 16, true),
        (10, 3, false, 64, false) => call!(10, 3, false, 64, false),
        (10, 3, false, 64, true) => call!(10, 3, false, 64, true),
        (10, 3, true, 16, false) => call!(10, 3, true, 16, false),
        (10, 3, true, 16, true) => call!(10, 3, true, 16, true),
        (10, 3, true, 64, false) => call!(10, 3, true, 64, false),
        (10, 3, true, 64, true) => call!(10, 3, true, 64, true),
        (100, 1, false, 16, false) => call!(100, 1, false, 16, false),
        (100, 1, false, 16, true) => call!(100, 1, false, 16, true),
        (100, 1, false, 64, false) => call!(100, 1, false, 64, false),
        (100, 1, false, 64, true) => call!(100, 1, false, 64, true),
        (100, 1, true, 16, false) => call!(100, 1, true, 16, false),
        (100, 1, true, 16, true) => call!(100, 1, true, 16, true),
        (100, 1, true, 64, false) => call!(100, 1, true, 64, false),
        (100, 1, true, 64, true) => call!(100, 1, true, 64, true),
        (100, 3, false, 16, false) => call!(100, 3, false, 16, false),
        (100, 3, false, 16, true) => call!(100, 3, false, 16, true),
        (100, 3, false, 64, false) => call!(100, 3, false, 64, false),
        (100, 3, false, 64, true) => call!(100, 3, false, 64, true),
        (100, 3, true, 16, false) => call!(100, 3, true, 16, false),
        (100, 3, true, 16, true) => call!(100, 3, true, 16, true),
        (100, 3, true, 64, false) => call!(100, 3, true, 64, false),
        (100, 3, true, 64, true) => call!(100, 3, true, 64, true),
        _ => panic!("unregistered adaptive policy"),
    }
}
fn dispatch(o: &Options) {
    if let Some(tail) = o.engine.strip_prefix("adaptive-") {
        let p: Vec<_> = tail.split('-').collect();
        assert_eq!(p.len(), 7);
        match (p[0], p[1]) {
            ("inline", "sip") => {
                policy::<std::collections::hash_map::RandomState, InlineFlat>(o, &p)
            }
            ("inline", "fast") => policy::<hashbrown::DefaultHashBuilder, InlineFlat>(o, &p),
            ("shared", "sip") => {
                policy::<std::collections::hash_map::RandomState, SharedFlat<0>>(o, &p)
            }
            ("shared", "fast") => policy::<hashbrown::DefaultHashBuilder, SharedFlat<0>>(o, &p),
            ("cache", "sip") => {
                policy::<std::collections::hash_map::RandomState, SharedFlat<2>>(o, &p)
            }
            ("cache", "fast") => policy::<hashbrown::DefaultHashBuilder, SharedFlat<2>>(o, &p),
            _ => panic!("unregistered flat mode/builder"),
        }
        return;
    }
    reference(o);
}
pub(super) fn reference(o: &Options) {
    macro_rules! control {
        ($m:ty) => {
            run(Compared::<$m>::new(), o)
        };
    }
    match o.engine.as_str() {
        "hashbrown" => control!(hashbrown::HashMap<Key,Value>),
        "hashbrown-sip" => {
            control!(hashbrown::HashMap<Key,Value,std::collections::hash_map::RandomState>)
        }
        "std-fast" => control!(std::collections::HashMap<Key,Value,hashbrown::DefaultHashBuilder>),
        "std-hash" => control!(std::collections::HashMap<Key,Value>),
        "std-btree" => control!(std::collections::BTreeMap<Key,Value>),
        "indexmap-fast" => control!(indexmap::IndexMap<Key,Value,hashbrown::DefaultHashBuilder>),
        "imbl-fast" => {
            control!(imbl::GenericHashMap<Key,Value,hashbrown::DefaultHashBuilder,imbl::shared_ptr::DefaultSharedPtr>)
        }
        "rpds-fast" => control!(rpds::HashTrieMapSync<Key,Value,hashbrown::DefaultHashBuilder>),
        "record-paged16" => {
            control!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>)
        }
        "record-paged64" => {
            control!(rill_state::experimental::RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,64>)
        }
        "inline-owned-index-25" => {
            control!(rill_state::experimental::InlineSliceMap<Key,Value,hashbrown::DefaultHashBuilder,25,rill_state::experimental::OwnedHasher,false>)
        }
        "vector-rb-fast" => {
            control!(vector_map::VectorMap<Key,Value,hashbrown::DefaultHashBuilder,vector_map::Rb>)
        }
        "vector-cowvec-fast" => {
            control!(vector_map::VectorMap<Key,Value,hashbrown::DefaultHashBuilder,vector_map::CowVec>)
        }
        _ => panic!("unregistered reference"),
    }
}
pub(super) fn entry() {
    let mut o = options();
    if !std::env::args().any(|a| a == "--engine") {
        o.engine = "adaptive-inline-fast-1-3-drop-16-reset".into();
    }
    dispatch(&o);
}
