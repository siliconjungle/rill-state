//! Instrumented payload ownership/reclamation proof. Never a speed benchmark.
#![forbid(unsafe_code)]
#[cfg(feature = "comparisons")]
mod profile {
    use rill_state::{
        experimental::{
            ArrayRecordMap, CopyKeys, InlineSliceMap, OwnedHasher, ProbeMap, RecordMap,
            SharedHasher, SharedKeys, SharedMap,
        },
        Config, Mutation, Snapshot, Store,
    };
    use std::{
        collections::VecDeque,
        hash::BuildHasher,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    type Key = u64;
    type Op = Mutation<Key, Value>;
    #[derive(Default)]
    struct Stats {
        live: AtomicUsize,
        bytes: AtomicUsize,
        peak_live: AtomicUsize,
        peak_bytes: AtomicUsize,
        clones: AtomicUsize,
    }
    struct Value {
        epoch: u64,
        bytes: Vec<u8>,
        stats: Arc<Stats>,
    }
    impl Value {
        fn owned(epoch: u64, bytes: Vec<u8>, stats: Arc<Stats>) -> Self {
            let n = stats.live.fetch_add(1, Ordering::Relaxed) + 1;
            let b = stats.bytes.fetch_add(bytes.len(), Ordering::Relaxed) + bytes.len();
            stats.peak_live.fetch_max(n, Ordering::Relaxed);
            stats.peak_bytes.fetch_max(b, Ordering::Relaxed);
            Self {
                epoch,
                bytes,
                stats,
            }
        }
        fn new(id: u64, epoch: u64, mixed: bool, stats: &Arc<Stats>) -> Self {
            let len = if mixed {
                match id.wrapping_add(epoch) % 32 {
                    0..=7 => 0,
                    8..=11 => 8,
                    12..=15 => 32,
                    16..=23 => 128,
                    24..=27 => 512,
                    28..=30 => 2048,
                    _ => 4096,
                }
            } else {
                256
            };
            Self::owned(epoch, vec![epoch as u8; len], stats.clone())
        }
    }
    impl Clone for Value {
        fn clone(&self) -> Self {
            let bytes = self.bytes.clone();
            self.stats.clones.fetch_add(1, Ordering::Relaxed);
            Self::owned(self.epoch, bytes, self.stats.clone())
        }
    }
    impl Drop for Value {
        fn drop(&mut self) {
            self.stats.live.fetch_sub(1, Ordering::Relaxed);
            self.stats
                .bytes
                .fetch_sub(self.bytes.len(), Ordering::Relaxed);
        }
    }
    trait Map: Clone {
        fn new() -> Self;
        fn get(&self, k: &Key) -> Option<&Value>;
        fn len(&self) -> usize;
        fn batch(&mut self, ops: &[Op]);
    }
    macro_rules! maps {
        ($ty:ty,$new:expr) => {
            impl Map for $ty {
                fn new() -> Self {
                    $new
                }
                fn get(&self, k: &Key) -> Option<&Value> {
                    self.get(k)
                }
                fn len(&self) -> usize {
                    self.len()
                }
                fn batch(&mut self, ops: &[Op]) {
                    for op in ops {
                        match op {
                            Mutation::Put(k, v) => {
                                self.insert(*k, v.clone());
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
    maps!(hashbrown::HashMap<Key,Value>,hashbrown::HashMap::new());
    type Imbl = imbl::GenericHashMap<
        Key,
        Value,
        hashbrown::DefaultHashBuilder,
        imbl::shared_ptr::DefaultSharedPtr,
    >;
    maps!(
        Imbl,
        Imbl::with_hasher(hashbrown::DefaultHashBuilder::default())
    );
    macro_rules! paged {
        ($ty:ty) => {
            impl Map for $ty {
                fn new() -> Self {
                    Self::default()
                }
                fn get(&self, k: &Key) -> Option<&Value> {
                    self.get(k)
                }
                fn len(&self) -> usize {
                    self.len()
                }
                fn batch(&mut self, ops: &[Op]) {
                    self.apply(ops);
                }
            }
        };
    }
    paged!(ProbeMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>);
    paged!(SharedMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>);
    paged!(RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16>);
    paged!(RecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,0>);
    paged!(ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16,CopyKeys>);
    paged!(ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,256,CopyKeys>);
    paged!(ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,16,SharedKeys>);
    paged!(ArrayRecordMap<Key,Value,hashbrown::DefaultHashBuilder,70,256,SharedKeys>);
    paged!(InlineSliceMap<Key,Value,hashbrown::DefaultHashBuilder,25,SharedHasher,true>);
    paged!(InlineSliceMap<Key,Value,hashbrown::DefaultHashBuilder,50,OwnedHasher,false>);
    paged!(InlineSliceMap<Key,Value,hashbrown::DefaultHashBuilder,70,SharedHasher,false>);
    paged!(InlineSliceMap<Key,Value,hashbrown::DefaultHashBuilder,85,OwnedHasher,true>);
    trait Read: Clone {
        fn revision(&self) -> u64;
        fn len(&self) -> usize;
        fn get(&self, k: &Key) -> Option<&Value>;
    }
    struct View<M>(Arc<M>, u64);
    impl<M> Clone for View<M> {
        fn clone(&self) -> Self {
            Self(self.0.clone(), self.1)
        }
    }
    impl<M: Map> Read for View<M> {
        fn revision(&self) -> u64 {
            self.1
        }
        fn len(&self) -> usize {
            self.0.len()
        }
        fn get(&self, k: &Key) -> Option<&Value> {
            self.0.get(k)
        }
    }
    impl<S: BuildHasher> Read for Snapshot<Key, Value, S> {
        fn revision(&self) -> u64 {
            self.cursor().revision()
        }
        fn len(&self) -> usize {
            self.len()
        }
        fn get(&self, k: &Key) -> Option<&Value> {
            self.get(k)
        }
    }
    trait Engine {
        type View: Read;
        fn commit(&mut self, ops: Vec<Op>);
        fn view(&self) -> Self::View;
        fn history(&self) -> (usize, usize);
    }
    struct Compared<M> {
        view: View<M>,
        history: VecDeque<Arc<[Op]>>,
        operations: usize,
    }
    impl<M: Map> Compared<M> {
        fn new() -> Self {
            Self {
                view: View(Arc::new(M::new()), 0),
                history: VecDeque::new(),
                operations: 0,
            }
        }
    }
    impl<M: Map> Engine for Compared<M> {
        type View = View<M>;
        fn commit(&mut self, ops: Vec<Op>) {
            let ops: Arc<[Op]> = ops.into();
            if !ops.is_empty() {
                let mut map = (*self.view.0).clone();
                map.batch(&ops);
                self.view.0 = Arc::new(map);
            }
            self.view.1 += 1;
            self.operations += ops.len();
            self.history.push_back(ops);
            while self.history.len() > 64 || self.operations > 1_000_000 {
                self.operations -= self.history.pop_front().unwrap().len();
            }
        }
        fn view(&self) -> Self::View {
            self.view.clone()
        }
        fn history(&self) -> (usize, usize) {
            (self.history.len(), self.operations)
        }
    }
    struct Native(Store<Key, Value, hashbrown::DefaultHashBuilder>);
    impl Native {
        fn new() -> Self {
            Self(
                Store::with_hasher(
                    Config {
                        history_batches: 64,
                        history_operations: 1_000_000,
                        ..Config::default()
                    },
                    hashbrown::DefaultHashBuilder::default(),
                )
                .unwrap(),
            )
        }
    }
    impl Engine for Native {
        type View = Snapshot<Key, Value, hashbrown::DefaultHashBuilder>;
        fn commit(&mut self, ops: Vec<Op>) {
            self.0.commit(ops).unwrap();
        }
        fn view(&self) -> Self::View {
            self.0.snapshot()
        }
        fn history(&self) -> (usize, usize) {
            self.0.retained_history()
        }
    }
    fn key(id: u64) -> Key {
        id.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
    }
    fn check(view: &impl Read, records: usize, changed: usize, rounds: usize) {
        assert_eq!(view.revision(), rounds as u64 + 1);
        assert_eq!(view.len(), records);
        let total = rounds * changed;
        for id in 0..records {
            let epoch = if id >= total {
                0
            } else {
                (id + (total - 1 - id) / records * records) / changed + 1
            };
            let value = view.get(&key(id as u64)).unwrap();
            assert_eq!(value.epoch, epoch as u64);
            assert!(value.bytes.iter().all(|b| *b == epoch as u8));
        }
    }
    struct Options {
        engine: String,
        records: usize,
        changed: usize,
        rounds: usize,
        window: usize,
        pin: bool,
        mixed: bool,
    }
    fn run(mut engine: impl Engine, o: Options) {
        assert!(
            o.records > 0
                && o.changed > 0
                && o.changed <= o.records
                && o.rounds * o.changed >= o.records
        );
        let stats = Arc::new(Stats::default());
        engine.commit(
            (0..o.records)
                .map(|id| Mutation::Put(key(id as u64), Value::new(id as u64, 0, o.mixed, &stats)))
                .collect(),
        );
        let pinned = o.pin.then(|| engine.view());
        let mut held = VecDeque::new();
        for round in 0..o.rounds {
            // Stream one batch at a time: no future trace or duplicate oracle values
            // may keep otherwise reclaimable payloads alive in this memory proof.
            engine.commit(
                (0..o.changed)
                    .map(|i| {
                        let id = (round * o.changed + i) % o.records;
                        Mutation::Put(
                            key(id as u64),
                            Value::new(id as u64, round as u64 + 1, o.mixed, &stats),
                        )
                    })
                    .collect(),
            );
            if o.window > 0 {
                held.push_back((engine.view(), round + 1));
                while held.len() > o.window {
                    held.pop_front();
                }
            }
        }
        check(&engine.view(), o.records, o.changed, o.rounds);
        for (view, rounds) in &held {
            check(view, o.records, o.changed, *rounds);
        }
        if let Some(view) = &pinned {
            check(view, o.records, o.changed, 0);
        }
        let history = engine.history();
        assert!(history.0 <= 64 && history.1 <= 1_000_000);
        let end_live = stats.live.load(Ordering::Relaxed);
        let end_bytes = stats.bytes.load(Ordering::Relaxed);
        drop(pinned);
        let unpinned_live = stats.live.load(Ordering::Relaxed);
        let unpinned_bytes = stats.bytes.load(Ordering::Relaxed);
        // The fully replaced initial state and its old journal batch are no
        // longer reachable from recent snapshots after enough complete cycles.
        if o.pin && o.rounds > o.window + o.records.div_ceil(o.changed) && o.rounds > 64 {
            assert_eq!(end_live - unpinned_live, o.records);
        }
        drop(held);
        let after_window = stats.live.load(Ordering::Relaxed);
        drop(engine);
        assert_eq!(stats.live.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes.load(Ordering::Relaxed), 0);
        println!("{{\"engine\":\"{}\",\"records\":{},\"changed\":{},\"rounds\":{},\"window\":{},\"pin_initial\":{},\"mixed\":{},\"live_values_end\":{},\"owned_payload_bytes_end\":{},\"live_values_after_unpin\":{},\"owned_payload_bytes_after_unpin\":{},\"live_values_after_window_drop\":{},\"peak_live_values\":{},\"peak_owned_payload_bytes\":{},\"value_clones\":{},\"history_batches\":{},\"history_operations\":{},\"after_all_drop\":0,\"verified\":true,\"instrumented\":true}}",o.engine,o.records,o.changed,o.rounds,o.window,o.pin,o.mixed,end_live,end_bytes,unpinned_live,unpinned_bytes,after_window,stats.peak_live.load(Ordering::Relaxed),stats.peak_bytes.load(Ordering::Relaxed),stats.clones.load(Ordering::Relaxed),history.0,history.1);
    }
    pub fn main() {
        let args: Vec<_> = std::env::args().collect();
        if args.iter().any(|a| a == "--layout-only") {
            println!(
                "{{\"instrumented_value_bytes\":{}}}",
                std::mem::size_of::<Value>()
            );
            return;
        }
        let get = |name: &str, default: &str| {
            args.windows(2)
                .find(|p| p[0] == name)
                .map(|p| p[1].clone())
                .unwrap_or_else(|| default.into())
        };
        let o = Options {
            engine: get("--engine", "rillfast"),
            records: get("--records", "20000").parse().unwrap(),
            changed: get("--changed", "200").parse().unwrap(),
            rounds: get("--rounds", "256").parse().unwrap(),
            window: get("--window", "16").parse().unwrap(),
            pin: get("--pin-initial", "true").parse().unwrap(),
            mixed: get("--mixed", "false").parse().unwrap(),
        };
        match o.engine.as_str() {
            "inline-shared-span-25" => run(
                Compared::<
                    InlineSliceMap<
                        Key,
                        Value,
                        hashbrown::DefaultHashBuilder,
                        25,
                        SharedHasher,
                        true,
                    >,
                >::new(),
                o,
            ),
            "inline-owned-index-50" => run(
                Compared::<
                    InlineSliceMap<
                        Key,
                        Value,
                        hashbrown::DefaultHashBuilder,
                        50,
                        OwnedHasher,
                        false,
                    >,
                >::new(),
                o,
            ),
            "inline-shared-index-70" => run(
                Compared::<
                    InlineSliceMap<
                        Key,
                        Value,
                        hashbrown::DefaultHashBuilder,
                        70,
                        SharedHasher,
                        false,
                    >,
                >::new(),
                o,
            ),
            "inline-owned-span-85" => run(
                Compared::<
                    InlineSliceMap<
                        Key,
                        Value,
                        hashbrown::DefaultHashBuilder,
                        85,
                        OwnedHasher,
                        true,
                    >,
                >::new(),
                o,
            ),
            "array-copy16" => run(
                Compared::<
                    ArrayRecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 16, CopyKeys>,
                >::new(),
                o,
            ),
            "array-copy256" => run(
                Compared::<
                    ArrayRecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 256, CopyKeys>,
                >::new(),
                o,
            ),
            "array-shared16" => run(
                Compared::<
                    ArrayRecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 16, SharedKeys>,
                >::new(),
                o,
            ),
            "array-shared256" => run(
                Compared::<
                    ArrayRecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 256, SharedKeys>,
                >::new(),
                o,
            ),
            "hashbrown" => run(Compared::<hashbrown::HashMap<Key, Value>>::new(), o),
            "imbl-fast" => run(Compared::<Imbl>::new(), o),
            "probe-paged16" => run(
                Compared::<ProbeMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 16>>::new(),
                o,
            ),
            "shared-paged16" => run(
                Compared::<SharedMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 16>>::new(),
                o,
            ),
            "record-paged16" => run(
                Compared::<RecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 16>>::new(),
                o,
            ),
            "record-flat" => run(
                Compared::<RecordMap<Key, Value, hashbrown::DefaultHashBuilder, 70, 0>>::new(),
                o,
            ),
            "rillfast" => run(Native::new(), o),
            _ => panic!("unknown engine"),
        }
    }
}
fn main() {
    #[cfg(feature = "comparisons")]
    profile::main();
    #[cfg(not(feature = "comparisons"))]
    eprintln!("Enable --features comparisons for the payload-retention proof.");
}
