//! Generated fixed policies. Reuses the parent state/revision/feed/trace/oracle harness.
use super::*;
use rill_state::experimental::{AdaptiveMap, FlatMode, InlineFlat, SharedFlat};
impl<
        K: Key,
        V: Value,
        S: BuildHasher + Clone + Default + Send + Sync,
        const T: usize,
        const D: usize,
        const B: bool,
        const P: usize,
        F: FlatMode<K, V, S> + Send + Sync,
        const R: bool,
    > Map<K, V> for AdaptiveMap<K, V, S, T, D, B, P, F, R>
where
    F::Data: Send + Sync,
{
    fn new() -> Self {
        Self::default()
    }
    fn get(&self, k: &K) -> Option<&V> {
        AdaptiveMap::get(self, k)
    }
    fn put(&mut self, k: K, v: V) {
        self.apply(&[Mutation::Put(k, v)]);
    }
    fn del(&mut self, k: &K) {
        self.apply(&[Mutation::Delete(k.clone())]);
    }
    fn len(&self) -> usize {
        AdaptiveMap::len(self)
    }
    fn visit(&self, f: impl FnMut(&K, &V)) {
        AdaptiveMap::visit(self, f);
    }
    fn batch(&mut self, ops: &[Mutation<K, V>]) {
        self.apply(ops);
    }
}
fn policy<
    K: Key,
    V: Value,
    S: BuildHasher + Clone + Default + Send + Sync,
    F: FlatMode<K, V, S> + Send + Sync,
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
            run::<K, V, _>(
                Compared::<
                    K,
                    V,
                    AdaptiveMap<K, V, S, $t, $d, $b, $p, F, $r>,
                    NativeRoot<AdaptiveMap<K, V, S, $t, $d, $b, $p, F, $r>>,
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
fn dispatch<K: Key, V: Value>(o: &Options) {
    if let Some(tail) = o.engine.strip_prefix("adaptive-") {
        let p: Vec<_> = tail.split('-').collect();
        assert_eq!(p.len(), 7);
        match (p[0], p[1]) {
            ("inline", "sip") => {
                policy::<K, V, std::collections::hash_map::RandomState, InlineFlat>(o, &p)
            }
            ("inline", "fast") => policy::<K, V, hashbrown::DefaultHashBuilder, InlineFlat>(o, &p),
            ("shared", "sip") => {
                policy::<K, V, std::collections::hash_map::RandomState, SharedFlat<0>>(o, &p)
            }
            ("shared", "fast") => {
                policy::<K, V, hashbrown::DefaultHashBuilder, SharedFlat<0>>(o, &p)
            }
            ("cache", "sip") => {
                policy::<K, V, std::collections::hash_map::RandomState, SharedFlat<2>>(o, &p)
            }
            ("cache", "fast") => {
                policy::<K, V, hashbrown::DefaultHashBuilder, SharedFlat<2>>(o, &p)
            }
            _ => panic!("unregistered flat mode/builder"),
        }
        return;
    }
    macro_rules! control {
        ($m:ty) => {
            run::<K, V, _>(Compared::<K, V, $m>::new(), o)
        };
    }
    match o.engine.as_str() {
        "hashbrown" => control!(hashbrown::HashMap<K,V>),
        "hashbrown-sip" => {
            control!(hashbrown::HashMap<K,V,std::collections::hash_map::RandomState>)
        }
        "std-fast" => control!(std::collections::HashMap<K,V,hashbrown::DefaultHashBuilder>),
        "std-hash" => control!(std::collections::HashMap<K,V>),
        "indexmap-fast" => control!(indexmap::IndexMap<K,V,hashbrown::DefaultHashBuilder>),
        "imbl-fast" => {
            control!(imbl::GenericHashMap<K,V,hashbrown::DefaultHashBuilder,imbl::shared_ptr::DefaultSharedPtr>)
        }
        "rpds-fast" => control!(rpds::HashTrieMapSync<K,V,hashbrown::DefaultHashBuilder>),
        "record-paged16" => {
            control!(rill_state::experimental::RecordMap<K,V,hashbrown::DefaultHashBuilder,70,16>)
        }
        "record-paged64" => {
            control!(rill_state::experimental::RecordMap<K,V,hashbrown::DefaultHashBuilder,70,64>)
        }
        "inline-owned-index-25" => {
            control!(rill_state::experimental::InlineSliceMap<K,V,hashbrown::DefaultHashBuilder,25,rill_state::experimental::OwnedHasher,false>)
        }
        "vector-rb-fast" => {
            control!(vector_map::VectorMap<K,V,hashbrown::DefaultHashBuilder,vector_map::Rb>)
        }
        "vector-cowvec-fast" => {
            control!(vector_map::VectorMap<K,V,hashbrown::DefaultHashBuilder,vector_map::CowVec>)
        }
        _ => panic!("unregistered reference"),
    }
}
pub(super) fn entry() {
    let mut o = options();
    if !std::env::args().any(|a| a == "--engine") {
        o.engine = "adaptive-inline-fast-1-3-drop-16-reset".into();
    }
    match o.shape.as_str() {
        "u64-inline" => dispatch::<u64, [u64; 4]>(&o),
        "string-inline" => dispatch::<String, [u64; 4]>(&o),
        "u64-owned256" => dispatch::<u64, Vec<u8>>(&o),
        "string-owned256" => dispatch::<String, Vec<u8>>(&o),
        "u64-shared256" => dispatch::<u64, Arc<[u8]>>(&o),
        "string-shared256" => dispatch::<String, Arc<[u8]>>(&o),
        "mixed-owned" => dispatch::<VariableKey, MixedOwned>(&o),
        "mixed-shared" => dispatch::<VariableKey, MixedShared>(&o),
        _ => panic!("unknown shape"),
    }
}
