//! Native transactions retain native versions. Export costs stay in publication.
use super::*;
use mvcc::{Database, Transaction, Versioned};
use std::{cell::RefCell, rc::Rc, sync::OnceLock};

#[derive(Clone)]
struct Row {
    key: Key,
    value: Value,
}
impl Versioned for Row {
    type Key = Key;
    const TABLE_NAME: &'static str = "scalar_comparison";
    fn table_id_cell() -> &'static OnceLock<mvcc::__private::TableId> {
        static ID: OnceLock<mvcc::__private::TableId> = OnceLock::new();
        &ID
    }
    fn key(&self) -> Key {
        self.key
    }
    fn key_bytes(&self) -> mvcc::__private::IndexKey {
        mvcc::__private::IndexKey(self.key.to_be_bytes().to_vec())
    }
    fn indexes() -> &'static [mvcc::__private::IndexDesc<Self>] {
        &[]
    }
}
fn open() -> Database {
    let db = Database::open(mvcc::Config::in_memory()).unwrap();
    db.register::<Row>().unwrap();
    db
}
type Tx<'db> = Transaction<'db, mvcc::Snapshot>;
struct Core<'db, const LAST: bool> {
    db: &'db Database,
    revision: u64,
    len: usize,
    history: VecDeque<(u64, Arc<[Op]>)>,
    count: usize,
}
impl<'db, const L: bool> Core<'db, L> {
    fn new(db: &'db Database) -> Self {
        Self {
            db,
            revision: 0,
            len: 0,
            history: VecDeque::new(),
            count: 0,
        }
    }
    fn stage(&self, ops: &[Op]) -> (Tx<'db>, usize) {
        let db: &'db Database = self.db;
        let mut tx = db.begin();
        let mut len = self.len;
        let mut apply = |op: &Op| match op {
            Op::Put(k, v) => {
                if !tx.update::<Row>(k, |row| row.value = *v).unwrap() {
                    tx.insert(Row { key: *k, value: *v }).unwrap();
                    len += 1;
                }
            }
            Op::Delete(k) => {
                len -= usize::from(tx.delete::<Row>(k).unwrap());
            }
        };
        if L {
            let mut final_ops = BTreeMap::new();
            for op in ops {
                match op {
                    Op::Put(k, v) => {
                        final_ops.insert(*k, Some(*v));
                    }
                    Op::Delete(k) => {
                        final_ops.insert(*k, None);
                    }
                }
            }
            for (k, v) in final_ops {
                apply(&v.map_or(Op::Delete(k), |v| Op::Put(k, v)));
            }
        } else {
            for op in ops {
                apply(op);
            }
        }
        (tx, len)
    }
    fn record(&mut self, ops: Arc<[Op]>, len: usize) {
        self.revision += 1;
        self.len = len;
        self.count += ops.len();
        self.history.push_back((self.revision, ops));
        while self.history.len() > 64 || self.count > 1_000_000 {
            self.count -= self.history.pop_front().unwrap().1.len();
        }
    }
    fn offset(&self, after: u64) -> usize {
        let oldest = self.history.front().map_or(self.revision, |(r, _)| r - 1);
        assert!(after >= oldest && after <= self.revision);
        (after - oldest) as usize
    }
    fn feed(&self, after: u64) -> Vec<Arc<[Op]>> {
        self.history
            .iter()
            .skip(self.offset(after))
            .map(|(_, b)| b.clone())
            .collect()
    }
    fn feed_summary(&self, after: u64) -> (usize, usize) {
        let mut n = 0;
        let mut ops = 0;
        for (_, batch) in self.history.iter().skip(self.offset(after)) {
            n += 1;
            ops += black_box(&**batch).len();
        }
        (n, ops)
    }
}
macro_rules! feed_methods {
    () => {
        fn feed(&self, after: u64) -> Vec<Arc<[Op]>> {
            self.core.feed(after)
        }
        fn feed_summary(&self, after: u64) -> (usize, usize) {
            self.core.feed_summary(after)
        }
        fn revision(&self) -> u64 {
            self.core.revision
        }
    };
}
#[derive(Clone)]
struct NativeView<'db> {
    tx: Rc<RefCell<Tx<'db>>>,
    revision: u64,
    len: usize,
}
fn native_contents(tx: &mut Tx<'_>) -> BTreeMap<Key, Value> {
    let mut contents = BTreeMap::new();
    for row in tx.scan::<Row>().unwrap() {
        assert!(contents.insert(row.key, row.value).is_none());
    }
    contents
}
impl ReadView for NativeView<'_> {
    fn revision(&self) -> u64 {
        self.revision
    }
    fn len(&self) -> usize {
        self.len
    }
    fn get(&self, key: &Key) -> Option<Value> {
        self.tx
            .borrow_mut()
            .get::<Row>(key)
            .unwrap()
            .map(|row| row.value)
    }
    fn checksum(&self) -> u64 {
        self.tx
            .borrow_mut()
            .scan::<Row>()
            .unwrap()
            .into_iter()
            .fold(0u64, |sum, row| {
                sum.wrapping_add(digest(row.key, row.value))
            })
    }
    fn contents(&self) -> BTreeMap<Key, Value> {
        native_contents(&mut self.tx.borrow_mut())
    }
}
struct Native<'db, const LAST: bool> {
    core: Core<'db, LAST>,
    current: NativeView<'db>,
}
impl<'db, const L: bool> Native<'db, L> {
    fn new(db: &'db Database) -> Self {
        Self {
            core: Core::new(db),
            current: NativeView {
                tx: Rc::new(RefCell::new(db.begin())),
                revision: 0,
                len: 0,
            },
        }
    }
}
impl<'db, const L: bool> Engine for Native<'db, L> {
    type View = NativeView<'db>;
    fn publish(&mut self, ops: Vec<Op>) {
        let ops: Arc<[Op]> = ops.into();
        let (tx, len) = self.core.stage(&ops);
        tx.commit().unwrap();
        self.core.record(ops, len);
        self.current = NativeView {
            tx: Rc::new(RefCell::new(self.core.db.begin())),
            revision: self.core.revision,
            len,
        };
    }
    fn snapshot(&self) -> Self::View {
        self.current.clone()
    }
    feed_methods!();
}
trait Capture: Map {
    fn with_capacity(len: usize) -> Self;
}
impl Capture for hashbrown::HashMap<Key, Value> {
    fn with_capacity(len: usize) -> Self {
        Self::with_capacity(len)
    }
}
impl Capture for HashMap<Key, Value, hashbrown::DefaultHashBuilder> {
    fn with_capacity(len: usize) -> Self {
        Self::with_capacity_and_hasher(len, Default::default())
    }
}
impl Capture for BTreeMap<Key, Value> {
    fn with_capacity(_: usize) -> Self {
        Self::new()
    }
}
struct Export<'db, M: Capture, const LAST: bool> {
    core: Core<'db, LAST>,
    current: View<M>,
}
impl<'db, M: Capture, const L: bool> Export<'db, M, L> {
    fn new(db: &'db Database) -> Self {
        Self {
            core: Core::new(db),
            current: View(Arc::new(M::with_capacity(0)), 0, std::marker::PhantomData),
        }
    }
}
impl<M: Capture, const L: bool> Engine for Export<'_, M, L> {
    type View = View<M>;
    fn publish(&mut self, ops: Vec<Op>) {
        let ops: Arc<[Op]> = ops.into();
        let (mut tx, len) = self.core.stage(&ops);
        // Materialize the transaction's own writes BEFORE commit. A capture
        // failure drops this transaction and rolls back its staged mutations.
        let next = if ops.is_empty() {
            None
        } else {
            let rows = tx.scan::<Row>().unwrap();
            let mut map = M::with_capacity(rows.len());
            for row in rows {
                map.put(row.key, row.value);
            }
            Some(Arc::new(map))
        };
        tx.commit().unwrap();
        self.core.record(ops, len);
        if let Some(map) = next {
            self.current.0 = map;
        }
        self.current.1 = self.core.revision;
    }
    fn snapshot(&self) -> Self::View {
        self.current.clone()
    }
    feed_methods!();
}
fn native_threads<const L: bool>(
    engine: &mut Native<'_, L>,
    before: NativeView<'_>,
    initial: Vec<Op>,
    input: Vec<Vec<Op>>,
    reads: Vec<Key>,
    o: &Options,
) {
    let expected: BTreeMap<_, _> = initial
        .into_iter()
        .map(|op| match op {
            Op::Put(k, v) => (k, v),
            Op::Delete(_) => unreachable!(),
        })
        .collect();
    assert_eq!(before.contents(), expected);
    let stamp = before.tx.borrow().snapshot();
    let barrier = std::sync::Barrier::new(5);
    let mut final_expected = expected.clone();
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let db = engine.core.db;
            let expected = &expected;
            let reads = &reads;
            let barrier = &barrier;
            scope.spawn(move || {
                // Native epoch guards stay on their creating threads.
                let mut tx = db.begin();
                barrier.wait();
                assert_eq!(tx.snapshot(), stamp);
                for _ in 0..o.rounds {
                    for k in reads.iter().skip(worker).step_by(4) {
                        assert_eq!(
                            tx.get::<Row>(k).unwrap().map(|row| row.value),
                            expected.get(k).copied()
                        );
                    }
                }
                assert_eq!(native_contents(&mut tx), *expected);
            });
        }
        barrier.wait();
        for ops in input {
            for op in &ops {
                match op {
                    Op::Put(k, v) => {
                        final_expected.insert(*k, *v);
                    }
                    Op::Delete(k) => {
                        final_expected.remove(k);
                    }
                }
            }
            engine.publish(ops);
        }
    });
    assert_eq!(engine.snapshot().contents(), final_expected);
    println!("{{\"engine\":\"{}\",\"case\":\"{}\",\"seed\":{},\"records\":{},\"readers\":{},\"rounds\":{},\"verified\":true,\"threads\":4,\"thread_checked\":true,\"native_reader_owned\":true}}",o.engine,o.case,o.seed,o.records,o.readers,o.rounds);
}
fn contracts<E: Engine>(mut engine: E, name: &str) {
    let zero = engine.snapshot();
    let mut old = None;
    for revision in 1..=80 {
        let v = value(1, revision);
        let ops = vec![
            Op::Put(1, value(1, revision + 1000)),
            Op::Delete(u64::MAX),
            Op::Put(1, v),
        ];
        engine.publish(ops.clone());
        assert_eq!(engine.revision(), revision);
        assert_eq!(&*engine.feed(revision - 1)[0], &ops);
        assert_eq!(engine.feed_summary(revision - 1), (1, 3));
        let snapshot = engine.snapshot();
        assert_eq!(snapshot.contents(), BTreeMap::from([(1, v)]));
        assert_eq!(snapshot.len(), 1);
        if revision == 1 {
            old = Some(snapshot);
        }
    }
    assert_eq!(zero.revision(), 0);
    assert!(zero.contents().is_empty());
    let old = old.unwrap();
    assert_eq!(old.revision(), 1);
    assert_eq!(old.get(&1), Some(value(1, 1)));
    assert_eq!(engine.feed_summary(16), (64, 192));
    for cursor in [15, 81] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.feed(cursor))).is_err()
        );
    }
    engine.publish(vec![]);
    assert_eq!(engine.revision(), 81);
    assert!(engine.feed(80)[0].is_empty());
    assert_eq!(engine.snapshot().get(&1), Some(value(1, 80)));
    println!(
        "{{\"engine\":\"{name}\",\"extra_contracts\":true,\"verified\":true,\"revisions\":81}}"
    );
}
fn capture_failure_rolls_back() {
    let db = open();
    let mut core = Core::<false>::new(&db);
    let original: Arc<[Op]> = vec![Op::Put(1, value(1, 1))].into();
    let (tx, len) = core.stage(&original);
    tx.commit().unwrap();
    core.record(original, len);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ops = vec![Op::Put(1, value(1, 2)), Op::Put(2, value(2, 2))];
        let (mut tx, _) = core.stage(&ops);
        assert_eq!(tx.scan::<Row>().unwrap().len(), 2);
        panic!("injected capture failure before native commit");
    }));
    assert!(result.is_err());
    assert_eq!(core.revision, 1);
    assert_eq!(core.len, 1);
    assert_eq!(core.feed_summary(0), (1, 1));
    assert_eq!(
        native_contents(&mut db.begin()),
        BTreeMap::from([(1, value(1, 1))])
    );
    assert_eq!(db.stats().active_transactions, 0);
}
fn entry_native<const L: bool>(o: &Options, kind: &str) {
    let db = open();
    let probe = std::env::args().any(|a| a == "--contract-probe");
    if probe {
        capture_failure_rolls_back();
    }
    macro_rules! exported {
        ($m:ty) => {{
            let engine = Export::<$m, L>::new(&db);
            if probe {
                contracts(engine, &o.engine)
            } else {
                run(engine, o)
            }
        }};
    }
    match kind {
        "scoped" => {
            let engine = Native::<L>::new(&db);
            if probe {
                contracts(engine, &o.engine)
            } else {
                run_with_thread_probe(engine, o, native_threads::<L>)
            }
        }
        "export-hashbrown" => exported!(hashbrown::HashMap<Key,Value>),
        "export-stdfast" => exported!(HashMap<Key,Value,hashbrown::DefaultHashBuilder>),
        "export-btree" => exported!(BTreeMap<Key,Value>),
        _ => panic!("unregistered native export"),
    }
    assert_eq!(db.stats().active_transactions, 0);
}
pub fn entry() {
    let o = options();
    if let Some(tail) = o.engine.strip_prefix("mvcc-native-") {
        let (kind, policy) = tail.rsplit_once('-').unwrap();
        match policy {
            "each" => entry_native::<false>(&o, kind),
            "last" => entry_native::<true>(&o, kind),
            _ => panic!("unregistered native batching"),
        }
        return;
    }
    if o.engine == "rill" || o.engine == "rillfast" {
        if o.engine == "rill" {
            let store = Store::new(Config::default()).unwrap();
            let cursor = store.cursor();
            run(
                Rill {
                    store,
                    cursors: vec![cursor],
                },
                &o,
            );
        } else {
            let store =
                Store::with_hasher(Config::default(), hashbrown::DefaultHashBuilder::default())
                    .unwrap();
            let cursor = store.cursor();
            run(
                Rill {
                    store,
                    cursors: vec![cursor],
                },
                &o,
            );
        }
        return;
    }
    macro_rules! adaptive {
        ($h:ty,$f:ty) => {
            run(
                Compared::<
                    rill_state::experimental::AdaptiveMap<Key, Value, $h, 1, 3, true, 16, $f, true>,
                    NativeRoot<
                        rill_state::experimental::AdaptiveMap<
                            Key,
                            Value,
                            $h,
                            1,
                            3,
                            true,
                            16,
                            $f,
                            true,
                        >,
                    >,
                >::new(),
                &o,
            )
        };
    }
    match o.engine.as_str() {
        "adaptive-inline-fast-1-3-drop-16-reset" => adaptive!(
            hashbrown::DefaultHashBuilder,
            rill_state::experimental::InlineFlat
        ),
        "adaptive-inline-sip-1-3-drop-16-reset" => adaptive!(
            std::collections::hash_map::RandomState,
            rill_state::experimental::InlineFlat
        ),
        "adaptive-reusecache-fast-1-3-drop-16-reset" => adaptive!(
            hashbrown::DefaultHashBuilder,
            rill_state::experimental::ReuseSharedFlat<2>
        ),
        "adaptive-reusecache-sip-1-3-drop-16-reset" => adaptive!(
            std::collections::hash_map::RandomState,
            rill_state::experimental::ReuseSharedFlat<2>
        ),
        _ => super::adaptive_dispatch::reference(&o),
    }
}
