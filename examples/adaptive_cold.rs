//! First-use representation/cache costs over identical mixed-density raw traces.
//! Full value/old-root checks are untimed; CPU caches are not flushed.
#![forbid(unsafe_code)]
#[cfg(feature = "comparisons")]
mod cold {
    use rill_state::{
        experimental::{AdaptiveMap, FlatMode, InlineFlat, RecordMap, ReuseSharedFlat, SharedFlat},
        Mutation,
    };
    use std::{
        collections::{BTreeMap, HashMap},
        fmt::Debug,
        hash::Hash,
        hint::black_box,
        sync::Arc,
        time::Instant,
    };
    type Builder = hashbrown::DefaultHashBuilder;
    trait Key: Clone + Hash + Eq + Ord + Debug {
        fn make(id: u64) -> Self;
    }
    impl Key for u64 {
        fn make(id: u64) -> Self {
            id.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
        }
    }
    impl Key for String {
        fn make(id: u64) -> Self {
            format!("key-{id:016x}-{:016x}-{:016x}", id.rotate_left(13), !id)
        }
    }
    trait Value: Clone + Eq + Debug {
        fn make(id: u64, epoch: u64) -> Self;
        fn touch(&self) -> u64;
    }
    impl Value for [u64; 4] {
        fn make(id: u64, epoch: u64) -> Self {
            [id, epoch, !id, id.wrapping_add(epoch)]
        }
        fn touch(&self) -> u64 {
            self[0].rotate_left(7) ^ self[1].wrapping_mul(317) ^ self[2] ^ self[3].rotate_left(23)
        }
    }
    fn bytes(id: u64, epoch: u64) -> Vec<u8> {
        let mut b = vec![epoch as u8; 256];
        b[..8].copy_from_slice(&id.to_le_bytes());
        b[248..].copy_from_slice(&epoch.to_le_bytes());
        b
    }
    fn touch(b: &[u8]) -> u64 {
        u64::from_le_bytes(b[..8].try_into().unwrap())
            ^ u64::from_le_bytes(b[248..].try_into().unwrap())
    }
    impl Value for Vec<u8> {
        fn make(id: u64, epoch: u64) -> Self {
            bytes(id, epoch)
        }
        fn touch(&self) -> u64 {
            touch(self)
        }
    }
    impl Value for Arc<[u8]> {
        fn make(id: u64, epoch: u64) -> Self {
            bytes(id, epoch).into()
        }
        fn touch(&self) -> u64 {
            touch(self)
        }
    }

    trait Backend<K: Key, V: Value>: Clone {
        fn new() -> Self;
        fn get(&self, k: &K) -> Option<&V>;
        fn len(&self) -> usize;
        fn apply(&mut self, ops: &[Mutation<K, V>]);
        fn visit(&self, f: impl FnMut(&K, &V));
        fn paged(&self) -> bool {
            false
        }
    }
    macro_rules! flat {
        ($ty:ty) => {
            impl<K: Key, V: Value> Backend<K, V> for $ty {
                fn new() -> Self {
                    Self::default()
                }
                fn get(&self, k: &K) -> Option<&V> {
                    self.get(k)
                }
                fn len(&self) -> usize {
                    self.len()
                }
                fn apply(&mut self, ops: &[Mutation<K, V>]) {
                    for op in ops {
                        match op {
                            Mutation::Put(k, v) => {
                                if let Some(old) = self.get_mut(k) {
                                    *old = v.clone();
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
                fn visit(&self, mut f: impl FnMut(&K, &V)) {
                    for (k, v) in self.iter() {
                        f(k, v);
                    }
                }
            }
        };
    }
    flat!(HashMap<K,V,Builder>);
    flat!(BTreeMap<K,V>);
    flat!(hashbrown::HashMap<K,V,Builder>);
    type Imbl<K, V> = imbl::GenericHashMap<K, V, Builder, imbl::shared_ptr::DefaultSharedPtr>;
    flat!(Imbl<K,V>);
    impl<K: Key, V: Value, const P: usize> Backend<K, V> for RecordMap<K, V, Builder, 70, P> {
        fn new() -> Self {
            Self::default()
        }
        fn get(&self, k: &K) -> Option<&V> {
            self.get(k)
        }
        fn len(&self) -> usize {
            self.len()
        }
        fn apply(&mut self, ops: &[Mutation<K, V>]) {
            self.apply(ops)
        }
        fn visit(&self, mut f: impl FnMut(&K, &V)) {
            for (k, v) in self.iter() {
                f(k, v);
            }
        }
        fn paged(&self) -> bool {
            true
        }
    }
    trait Map<K: Key, V: Value>: Clone {
        fn new() -> Self;
        fn get(&self, k: &K) -> Option<&V>;
        fn len(&self) -> usize;
        fn apply(&mut self, ops: &[Mutation<K, V>]);
        fn visit(&self, f: impl FnMut(&K, &V));
        fn paged(&self) -> bool {
            false
        }
        fn cached(&self) -> usize {
            0
        }
    }
    struct Cow<M>(Arc<M>);
    impl<M> Clone for Cow<M> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<K: Key, V: Value, M: Backend<K, V>> Map<K, V> for Cow<M> {
        fn new() -> Self {
            Self(Arc::new(M::new()))
        }
        fn get(&self, k: &K) -> Option<&V> {
            self.0.get(k)
        }
        fn len(&self) -> usize {
            self.0.len()
        }
        fn apply(&mut self, ops: &[Mutation<K, V>]) {
            if !ops.is_empty() {
                Arc::make_mut(&mut self.0).apply(ops);
            }
        }
        fn visit(&self, f: impl FnMut(&K, &V)) {
            self.0.visit(f)
        }
        fn paged(&self) -> bool {
            self.0.paged()
        }
    }
    impl<
            K: Key,
            V: Value,
            const D: usize,
            const P: usize,
            F: FlatMode<K, V, Builder>,
            const R: bool,
        > Map<K, V> for AdaptiveMap<K, V, Builder, 1, D, true, P, F, R>
    {
        fn new() -> Self {
            Self::default()
        }
        fn get(&self, k: &K) -> Option<&V> {
            self.get(k)
        }
        fn len(&self) -> usize {
            self.len()
        }
        fn apply(&mut self, ops: &[Mutation<K, V>]) {
            self.apply(ops)
        }
        fn visit(&self, f: impl FnMut(&K, &V)) {
            self.visit(f)
        }
        fn paged(&self) -> bool {
            self.is_paged()
        }
        fn cached(&self) -> usize {
            self.cached_records()
        }
    }
    struct Options {
        engine: String,
        shape: String,
        records: usize,
        seed: u64,
    }
    fn rng(x: &mut u64) -> u64 {
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        *x
    }
    fn check<K: Key, V: Value, M: Map<K, V>>(map: &M, epochs: &[u64]) {
        assert_eq!(map.len(), epochs.len());
        for (id, &epoch) in epochs.iter().enumerate() {
            assert_eq!(
                map.get(&K::make(id as u64)),
                Some(&V::make(id as u64, epoch))
            );
        }
        for id in epochs.len()..epochs.len() + 4 {
            assert!(map.get(&K::make(id as u64)).is_none());
        }
    }
    fn scans<K: Key, V: Value, M: Map<K, V>>(map: &M, expected: u64) -> ([u128; 3], [usize; 3]) {
        let mut times = [0; 3];
        let mut sizes = [0; 3];
        for i in 0..3 {
            let mut sum = 0u64;
            let mut count = 0;
            let start = Instant::now();
            map.visit(|k, v| {
                black_box(k);
                sum = sum.wrapping_add(black_box(v.touch()));
                count += 1;
            });
            times[i] = start.elapsed().as_nanos();
            assert_eq!(sum, expected);
            assert_eq!(count, map.len());
            sizes[i] = map.cached();
        }
        (times, sizes)
    }
    struct Sample {
        step: usize,
        kind: &'static str,
        ops: usize,
        before: bool,
        after: bool,
        batch_ns: u128,
        scan_ns: [u128; 3],
        cache: [usize; 3],
        sum: u64,
    }
    fn emit(o: &Options, s: Sample) {
        let Sample {
            step,
            kind,
            ops,
            before,
            after,
            batch_ns,
            scan_ns,
            cache,
            sum,
        } = s;
        println!("{{\"kind\":\"adaptive-cold-mixed-density\",\"engine\":\"{}\",\"shape\":\"{}\",\"seed\":{},\"records\":{},\"step\":{},\"phase\":\"{}\",\"operations\":{},\"before_paged\":{},\"after_paged\":{},\"batch_ns\":{},\"scan1_ns\":{},\"scan2_ns\":{},\"scan3_ns\":{},\"cache_after_scan1\":{},\"cache_after_scan2\":{},\"cache_after_scan3\":{},\"checksum\":{},\"verified\":true}}",o.engine,o.shape,o.seed,o.records,step,kind,ops,before,after,batch_ns,scan_ns[0],scan_ns[1],scan_ns[2],cache[0],cache[1],cache[2],sum);
    }
    fn run<K: Key, V: Value, M: Map<K, V>>(mut map: M, o: &Options) {
        assert!(o.records >= 1000);
        let mut epochs = vec![0u64; o.records];
        let initial: Vec<_> = (0..o.records)
            .map(|id| Mutation::Put(K::make(id as u64), V::make(id as u64, 0)))
            .collect();
        map.apply(&initial);
        drop(initial);
        check(&map, &epochs);
        let expected = |epochs: &[u64]| {
            epochs.iter().enumerate().fold(0u64, |s, (id, &e)| {
                s.wrapping_add(V::make(id as u64, e).touch())
            })
        };
        let sum = expected(&epochs);
        let (times, cache) = scans(&map, sum);
        emit(
            o,
            Sample {
                step: 0,
                kind: "initial",
                ops: 0,
                before: map.paged(),
                after: map.paged(),
                batch_ns: 0,
                scan_ns: times,
                cache,
                sum,
            },
        );
        let initial_root = map.clone();
        let mut seed = o.seed;
        for step in 1..=15usize {
            let large = step % 5 == 0;
            let count = if large {
                o.records.div_ceil(2)
            } else {
                o.records / 100
            };
            // Generate identical complete inputs outside every measured batch.
            let ids: Vec<usize> = if large {
                let mut ids: Vec<_> = (0..o.records).collect();
                for i in (1..ids.len()).rev() {
                    let j = rng(&mut seed) as usize % (i + 1);
                    ids.swap(i, j);
                }
                ids.truncate(count);
                ids
            } else {
                (0..count)
                    .map(|_| rng(&mut seed) as usize % o.records)
                    .collect()
            };
            let ops: Vec<_> = ids
                .iter()
                .map(|&id| Mutation::Put(K::make(id as u64), V::make(id as u64, step as u64)))
                .collect();
            let old = map.clone();
            let old_epochs = epochs.clone();
            let before = map.paged();
            let start = Instant::now();
            let mut staged = map.clone();
            staged.apply(&ops);
            map = staged;
            let batch_ns = start.elapsed().as_nanos();
            for id in ids {
                epochs[id] = step as u64;
            }
            check(&map, &epochs);
            check(&old, &old_epochs);
            check(&initial_root, &vec![0; o.records]);
            let sum = expected(&epochs);
            let (times, cache) = scans(&map, sum);
            emit(
                o,
                Sample {
                    step,
                    kind: if large { "large" } else { "small" },
                    ops: count,
                    before,
                    after: map.paged(),
                    batch_ns,
                    scan_ns: times,
                    cache,
                    sum,
                },
            );
            // Releasing readers is deliberately outside this raw pinned-batch interval.
            drop(old);
        }
    }
    fn policy<K: Key, V: Value, F: FlatMode<K, V, Builder>>(o: &Options, p: &[&str]) {
        let delay = p[3].parse::<usize>().unwrap();
        let page = p[5].parse::<usize>().unwrap();
        let reset = p[6] == "reset";
        assert_eq!(p[2], "1");
        assert_eq!(p[4], "drop");
        assert!(p[6] == "reset" || p[6] == "oneway");
        macro_rules! call {
            ($d:literal,$p:literal,$r:literal) => {
                run::<K, V, _>(
                    AdaptiveMap::<K, V, Builder, 1, $d, true, $p, F, $r>::default(),
                    o,
                )
            };
        }
        match (delay, page, reset) {
            (1, 16, false) => call!(1, 16, false),
            (1, 16, true) => call!(1, 16, true),
            (1, 64, false) => call!(1, 64, false),
            (1, 64, true) => call!(1, 64, true),
            (3, 16, false) => call!(3, 16, false),
            (3, 16, true) => call!(3, 16, true),
            (3, 64, false) => call!(3, 64, false),
            (3, 64, true) => call!(3, 64, true),
            _ => panic!("unknown policy"),
        }
    }
    fn dispatch<K: Key, V: Value>(o: &Options) {
        if let Some(tail) = o.engine.strip_prefix("adaptive-") {
            let p: Vec<_> = tail.split('-').collect();
            assert_eq!(p.len(), 7);
            assert_eq!(p[1], "fast");
            match p[0] {
                "inline" => policy::<K, V, InlineFlat>(o, &p),
                "shared" => policy::<K, V, SharedFlat<0>>(o, &p),
                "cache" => policy::<K, V, SharedFlat<2>>(o, &p),
                "reuse" => policy::<K, V, ReuseSharedFlat<0>>(o, &p),
                "reusecache" => policy::<K, V, ReuseSharedFlat<2>>(o, &p),
                _ => panic!("unknown mode"),
            };
            return;
        }
        match o.engine.as_str() {
            "hashbrown" => run::<K, V, _>(Cow::<hashbrown::HashMap<K, V>>::new(), o),
            "std-fast" => run::<K, V, _>(Cow::<HashMap<K, V, Builder>>::new(), o),
            "std-btree" => run::<K, V, _>(Cow::<BTreeMap<K, V>>::new(), o),
            "imbl-fast" => run::<K, V, _>(Cow::<Imbl<K, V>>::new(), o),
            "record-paged16" => run::<K, V, _>(Cow::<RecordMap<K, V, Builder, 70, 16>>::new(), o),
            "record-paged64" => run::<K, V, _>(Cow::<RecordMap<K, V, Builder, 70, 64>>::new(), o),
            _ => panic!("unknown control"),
        }
    }
    pub fn main() {
        let args: Vec<_> = std::env::args().collect();
        let get = |n: &str, d: &str| {
            args.windows(2)
                .find(|w| w[0] == n)
                .map(|w| w[1].clone())
                .unwrap_or_else(|| d.into())
        };
        let o = Options {
            engine: get("--engine", "adaptive-cache-fast-1-3-drop-16-reset"),
            shape: get("--shape", "u64-owned256"),
            records: get("--records", "20000").parse().unwrap(),
            seed: get("--seed", "4815162342").parse().unwrap(),
        };
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
}
#[cfg(feature = "comparisons")]
fn main() {
    cold::main();
}
#[cfg(not(feature = "comparisons"))]
fn main() {
    eprintln!("enable --features comparisons");
    std::process::exit(2);
}
